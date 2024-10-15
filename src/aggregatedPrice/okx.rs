use std::env;
use std::error::Error;
use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use dotenv::dotenv;
use futures_util::{StreamExt, SinkExt};
use mysql::{params, Opts, Pool, PooledConn};
use mysql::prelude::Queryable;
use reqwest::{Client, Proxy};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream};
use crate::aggregatedPrice::pricedata::PriceData;
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::Exchange::Exchange;

pub async fn fetch_okx_mark_price(tx: mpsc::Sender<PriceData>, pair: CryptoPair, conn :&mut PooledConn) -> Result<(), Box<dyn Error>> {
    let mut inst_id = format!("{}-{}", pair.token1, pair.token2);
    println!("inst_id is {}", inst_id);

    // 创建 WebSocket 连接
    let url = "wss://ws.okx.com:8443/ws/v5/public";
    let (mut socket, _) = connect_async(url).await.expect("Failed to connect");

    println!("Connected to OKX WebSocket");

    // 订阅标记价格
    let subscribe_message = json!({
        "op": "subscribe",
        "args": [{
            "channel": "mark-price",
            "instId": inst_id
        }]
    });

    // 发送订阅请求
    socket.send(Message::Text(subscribe_message.to_string())).await?;
    println!("Subscription request sent for {} mark-price", inst_id);

    let heartbeat = pair.heartbeat;
    let deviation_threshold = pair.deviation_threshold;
    let mut last_price: f64 = 0.0;

    // 初始心跳定时器，使用 tokio::time::sleep
    let mut heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
    let mut price_data = PriceData {
        id: 2,
        source: "okx".to_string(),
        price: 0.0,
    };

    let mut send_status;
    // 循环接收 WebSocket 消息
    loop {
        tokio::select! {
            // 处理 WebSocket 消息
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let data: serde_json::Value = serde_json::from_str(&text)?;

                        // 检查是否是标记价格数据
                        if let Some(channel) = data["arg"]["channel"].as_str() {
                            if channel == "mark-price" && data["data"].is_array() && !data["data"].as_array().unwrap().is_empty() {
                                let mark_price_data = &data["data"][0];
                                let mark_px = mark_price_data["markPx"].as_str().unwrap_or("0"); // 获取标记价格
                                let price = mark_px.parse::<f64>().unwrap_or(0.0); // 解析价格
                                println!("Mark Price for {} : {} from okx", inst_id, price);

                                // 检查价格变化是否超过阈值
                                price_data.price = price;
                                if (price - last_price).abs() > deviation_threshold {
                                    if let Err(e) = tx.send(price_data.clone()).await {
                                        eprintln!("发送价格时出错: {:?}", e);
                                        send_status = "failed";
                                    } else {
                                        println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                        last_price = price; // 更新上次推送的价格
                                        send_status = "succeed";

                                        // 重置心跳定时器
                                        heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
                                    }

                                    conn.exec_drop(
                                    r"INSERT INTO DexRecord (dex_name, send_price, send_status, pair_name, timestamp) VALUES (:dex_name, :send_price, :send_status, :pair_name, :timestamp)",
                                    params! {
                                        "dex_name" => "OKX".to_string(),
                                        "send_price" => price,
                                        "send_status" => send_status,
                                            "pair_name" => format!("{}-{}", pair.token1, pair.token2),
                                            "timestamp" => Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                    },
                                ).unwrap();
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        println!("Received binary data, which is unexpected.");
                    }
                    _ => {}
                }
            }

            // 处理心跳定时器触发
            _ = heartbeat_timer.as_mut() => {
                tx.send(price_data.clone()).await?;
                println!("Heartbeat 触发推送: {:.2}", last_price);

                // 重置心跳计时器
                heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
            }
        }
    }

}

#[derive(Debug)]
pub struct OKX{
    pub name: String,
    pub id: u8,
    pub client: Option<Client>,
    pub ws: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    pub pair: CryptoPair,
}


impl OKX {
    pub fn new(proxy_url: &str, name: String, id: u8, pair: CryptoPair, ) -> Result<Self, Box<dyn Error>> {

        Ok(OKX { client:None, id, name, pair, ws: None, })
    }
}


#[async_trait]
impl Exchange for OKX {
    async fn connect(&mut self, wss_url: &str) -> Result<(), Box<dyn Error>> {
        let (mut socket, _) = connect_async(wss_url).await.expect("Failed to connect");

        let mut inst_id = format!("{}-{}", self.pair.token1, self.pair.token2);
        println!("inst_id is {}", inst_id);

        println!("Connected to OKX WebSocket");
        let subscribe_message = json!({
            "op": "subscribe",
            "args": [{
                "channel": "mark-price",
                "instId": inst_id
            }]
        }).to_string();

        self.ws = Some(socket);
        if let Some(socket) = self.ws.as_mut() {
            if let Err(e) = socket.send(Message::Text(subscribe_message.to_string())).await{
                eprintln!("Send Error: {:?}", e);
            }
        }
        println!("Connected to OKX WebSocket");
        Ok(())
    }

    async fn fetch_price(
        &mut self,
        tx: mpsc::Sender<PriceData>,
        conn: &mut PooledConn,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(socket) = self.ws.as_mut() {
            println!("Start fetch price in OKX");

            let heartbeat = self.pair.heartbeat;
            let deviation_threshold = self.pair.deviation_threshold;
            let mut last_price: f64 = 0.0;

            // 初始心跳定时器，使用 tokio::time::sleep
            let mut heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
            let mut price_data = PriceData {
                id: 2,
                source: "okx".to_string(),
                price: 0.0,
            };
            let mut send_status;

            loop {
                tokio::select! {
                // 处理 WebSocket 消息
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            let data: serde_json::Value = serde_json::from_str(&text)?;

                            // 检查是否是标记价格数据
                            if let Some(channel) = data["arg"]["channel"].as_str() {
                                if channel == "mark-price" && data["data"].is_array() && !data["data"].as_array().unwrap().is_empty() {
                                    let mark_price_data = &data["data"][0];
                                    let mark_px = mark_price_data["markPx"].as_str().unwrap_or("0"); // 获取标记价格
                                    let price = mark_px.parse::<f64>().unwrap_or(0.0); // 解析价格
                                    println!("Mark Price for {}-{} : {} from okx", self.pair.token1, self.pair.token2, price);

                                    // 检查价格变化是否超过阈值
                                    price_data.price = price;
                                    if (price - last_price).abs() > deviation_threshold {
                                        if let Err(e) = tx.send(price_data.clone()).await {
                                            eprintln!("发送价格时出错: {:?}", e);
                                            send_status = "failed";
                                        } else {
                                            println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                            last_price = price; // 更新上次推送的价格
                                            send_status = "succeed";

                                            // 重置心跳定时器
                                            heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
                                        }

                                        conn.exec_drop(
                                        r"INSERT INTO DexRecord (dex_name, send_price, send_status, pair_name, timestamp) VALUES (:dex_name, :send_price, :send_status, :pair_name, :timestamp)",
                                        params! {
                                            "dex_name" => "OKX".to_string(),
                                            "send_price" => price,
                                            "send_status" => send_status,
                                                "pair_name" => format!("{}-{}", self.pair.token1, self.pair.token2),
                                                "timestamp" => Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                        },
                                    ).unwrap();
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Binary(_))) => {
                            println!("Received binary data, which is unexpected.");
                        }
                        _ => {}
                    }
                }

                // 处理心跳定时器触发
                _ = heartbeat_timer.as_mut() => {
                    tx.send(price_data.clone()).await?;
                    println!("Heartbeat 触发推送: {:.2}", last_price);

                    // 重置心跳计时器
                    heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
                }
            }
            }
        }
        Ok(())
    }
}



#[tokio::test]
async fn test_price() {
    let (price_tx, mut price_rx) = mpsc::channel::<PriceData>(32); // 用于接收聚合后的价格'
    test_fetch_price(price_tx).await.expect("Failed to fetch price");

    while let Some(received_price_data) = price_rx.recv().await {
        println!("price is :{}", received_price_data);
    }
}

pub async fn test_fetch_price(price_tx: mpsc::Sender<PriceData>) -> Result<(), Box<dyn Error>> {

    //连接数据库
    dotenv().ok();
    let _sql_url = env::var("MYSQL_URL").unwrap_or_else(|_| env::var("mysql_url").expect("MYSQL_URL is not set"));
    let opts = Opts::from_url(&_sql_url)?;
    let pool = Arc::new(Pool::new(opts).unwrap()); // 使用 Arc 包裹 Pool
    let mut conn = pool.get_conn().unwrap();


    let crypto_pair = CryptoPair {
        token1: "BTC".to_string(),
        token2: "USDT".to_string(),
        heartbeat: 30,
        deviation_threshold: 5.0,
        oracle_contract: "".to_string(),
        decimal: 0,
        api_key: "".to_string(),
        private_key: "".to_string(),
    };

    let mut okx = OKX::new(
        "http://172.17.112.1:7890",
        "OKX Exchange".to_string(),
        3,
        crypto_pair,
    )?;

    okx
        .connect("wss://ws.okx.com:8443/ws/v5/public")
        .await
        .expect("connect failed");
    okx.fetch_price(price_tx, &mut conn).await.expect("fetch failed");

    Ok(())
}
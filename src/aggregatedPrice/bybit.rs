use std::env;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use serde_json::{json, Value};
use std::error::Error;
use std::pin::Pin;
use std::sync::Arc;
use chrono::Utc;
use mysql::{params, Opts, Pool, PooledConn};
use mysql::prelude::Queryable;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;
use reqwest_websocket::{Message, RequestBuilderExt, WebSocket};
use reqwest::{Client, Proxy};
use async_trait::async_trait;
use dotenv::dotenv;
use crate::aggregatedPrice::Exchange::Exchange;

pub async fn fetch_bybit_price(tx: mpsc::Sender<PriceData>, pair: CryptoPair, conn: &mut PooledConn) -> Result<(), Box<dyn Error>> {
    // 设置代理
    let proxy = Proxy::https("http://172.17.112.1:7890")?;
    // 创建带有代理的 reqwest 客户端
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .danger_accept_invalid_certs(true)
        .build()?;

    // 创建 WebSocket 连接
    let response = client.get("wss://stream.bybit.com/v5/public/spot").upgrade().send().await?;
    let mut websocket = response.into_websocket().await?;

    let subscribe_message = json!({
        "op": "subscribe",
        "args": [format!("tickers.{}{}",pair.token1,pair.token2)]
    }).to_string();
    websocket.send(Message::Text(subscribe_message)).await?;

    let heartbeat = pair.heartbeat;
    let deviation_threshold = pair.deviation_threshold;
    let mut last_price: f64 = 0.0;

    // 初始心跳定时器，使用 tokio::time::sleep
    let mut heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
    let mut price_data = PriceData {
        id: 3,
        source: "bybit".to_string(),
        price: 0.0,
    };
    let mut send_status;

    loop {
        tokio::select! {
            message = websocket.try_next() => {
                match message? {
                    Some(Message::Text(text)) => {
                        if let Ok(event_data) = serde_json::from_str::<Value>(&text) {
                            if let Some(data) = event_data.get("data") {
                                if let Some(last_price_str) = data["lastPrice"].as_str() {
                                    let price = last_price_str.parse::<f64>().unwrap();
                                    println!("Mark Price for {}-{} : {:.2} from bybit", pair.token1, pair.token2, price);

                                    price_data.price = price;
                                    if (price - last_price).abs() > deviation_threshold {
                                        if let Err(e) = tx.send(price_data.clone()).await {
                                            eprintln!("发送价格时出错: {:?}", e);
                                            send_status = "failed";
                                        } else {
                                            println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                            last_price = price; // 更新上次推送的价格
                                            send_status = "succeed";

                                            // 重置心跳计时器
                                            heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
                                        }
                                        // 插入数据到数据库
                                        conn.exec_drop(
                                            r"INSERT INTO DexRecord (dex_name, send_price, send_status, pair_name, timestamp) VALUES (:dex_name, :send_price, :send_status, :pair_name, :timestamp)",
                                            params! {
                                                "dex_name" => "Bybit".to_string(),
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
                    }
                    Some(Message::Binary(_)) => println!("接收到意外的二进制数据"),
                    _ => {},
                }
            }
            _ = heartbeat_timer.as_mut() => {
                if let Err(e) = tx.send(price_data.clone()).await {
                    eprintln!("发送心跳推送时出错: {:?}", e);
                    break;
                } else {
                    println!("Heartbeat 触发推送: {:.2}", last_price);

                    // 重置心跳计时器
                    heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
                }
            }
        }
    }
    Ok(())
}


#[derive(Debug)]
pub struct BYBIT {
    pub name: String,
    pub id: u8,
    pub client: Client,
    pub ws: Option<WebSocket>,
    pub pair: CryptoPair,
}

impl BYBIT {
    pub fn new(
        proxy_url: &str,
        name: String,
        id: u8,
        pair: CryptoPair,
    ) -> Result<Self, Box<dyn Error>> {
        let proxy = Proxy::https(proxy_url)?;

        let client = reqwest::Client::builder()
            .proxy(proxy)
            .danger_accept_invalid_certs(true)
            .build()?;

        Ok(BYBIT {
            client,
            id,
            name,
            pair,
            ws: None,
        })
    }
}

#[async_trait]
impl Exchange for BYBIT {
    async fn connect(&mut self, wss_url: &str) -> Result<(), Box<dyn Error>> {
        let response = self.client.get(wss_url).upgrade().send().await?;
        let mut websocket = response.into_websocket().await?;

        let subscribe_message = json!({
            "op": "subscribe",
            "args": [format!("tickers.{}{}",self.pair.token1, self.pair.token2)]
        })
            .to_string();

        self.ws = Some(websocket);

        if let Some(ws) = self.ws.as_mut() {
            ws.send(Message::Text(subscribe_message)).await?;
        }
        println!("Connected to Bybit WebSocket");
        Ok(())
    }

    async fn fetch_price(
        &mut self,
        tx: mpsc::Sender<PriceData>,
        conn: &mut PooledConn,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(websocket) = self.ws.as_mut() {
            println!("Start fetch price in Bybit");

            let mut last_price = 0.0;
            let heartbeat = self.pair.heartbeat;
            let deviation_threshold = self.pair.deviation_threshold;

            let mut heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
            let mut price_data = PriceData {
                id: 3,
                source: "bybit".to_string(),
                price: 0.0,
            };
            let mut send_status;

            loop {
                tokio::select! {
                    message = websocket.try_next() => {
                        match message? {
                            Some(Message::Text(text)) => {
                                if let Ok(event_data) = serde_json::from_str::<Value>(&text) {
                                    if let Some(data) = event_data.get("data") {
                                        if let Some(last_price_str) = data["lastPrice"].as_str() {
                                            let price = last_price_str.parse::<f64>().unwrap();
                                            println!("Mark Price for {}-{} : {:.2} from bybit", self.pair.token1, self.pair.token2, price);

                                            price_data.price = price;
                                            if (price - last_price).abs() > deviation_threshold {
                                                if let Err(e) = tx.send(price_data.clone()).await {
                                                    eprintln!("发送价格时出错: {:?}", e);
                                                    send_status = "failed";
                                                } else {
                                                    println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                                    last_price = price; // 更新上次推送的价格
                                                    send_status = "succeed";

                                                    // 重置心跳计时器
                                                    heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
                                                }
                                                // 插入数据到数据库
                                            conn.exec_drop(
                                                r"INSERT INTO DexRecord (dex_name, send_price, send_status, pair_name, timestamp) VALUES (:dex_name, :send_price, :send_status, :pair_name, :timestamp)",
                                                params! {
                                                "dex_name" => "Bybit".to_string(),
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
                            }
                            Some(Message::Binary(_)) => println!("接收到意外的二进制数据"),
                            _ => {},
                        }
                    }
                    _ = heartbeat_timer.as_mut() => {
                    if let Err(e) = tx.send(price_data.clone()).await {
                        eprintln!("发送心跳推送时出错: {:?}", e);
                        break;
                    } else {
                        println!("Heartbeat 触发推送: {:.2}", last_price);

                        // 重置心跳计时器
                        heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
                    }
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
    print!("wuxizhi1");
    test_fetch_price(price_tx).await;

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

    let mut bybit = BYBIT::new(
        "http://172.17.112.1:7890",
        "Bybit Exchange".to_string(),
        3,
        crypto_pair,
    )
        .unwrap();

    bybit
        .connect("wss://stream.bybit.com/v5/public/spot")
        .await
        .expect("connect failed");
    bybit.fetch_price(price_tx, &mut conn).await.expect("fetch failed");

    Ok(())
}
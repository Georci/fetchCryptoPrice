use futures_util::{SinkExt, StreamExt, TryStreamExt};
use reqwest::{Client, Proxy};
use reqwest_websocket::{Message, RequestBuilderExt};
use serde_json::{json, Value};
use std::error::Error;
use chrono::Utc;
use mysql::{params, PooledConn};
use mysql::prelude::Queryable;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use crate::aggregatedPrice::{pricedata::PriceData, cryptopair::CryptoPair};

pub async fn fetch_kraken_price(tx: mpsc::Sender<PriceData>, pair: CryptoPair, conn: &mut PooledConn) -> Result<(), Box<dyn Error>> {
    // 设置代理
    let proxy = Proxy::https("http://10.255.255.254:7890")?;
    let client = Client::builder().proxy(proxy).danger_accept_invalid_certs(true).build()?;

    // 创建 WebSocket 连接
    let response = client.get("wss://ws.kraken.com/v2").upgrade().send().await?;
    let mut websocket = response.into_websocket().await?;

    println!("Connected to kraken WebSocket");

    // 订阅 Kraken 交易数据
    let symbol = format!("{}/{}", pair.token1, pair.token2);
    let subscribe_message = json!({
        "method": "subscribe",
        "params": { "channel": "trade", "symbol": [symbol] }
    });
    websocket.send(Message::Text(subscribe_message.to_string())).await?;

    // 初始化参数
    let mut last_price = 0.0;
    let heartbeat = pair.heartbeat;
    let deviation_threshold = pair.deviation_threshold;
    let mut price_data = PriceData { id: 3, source: "kraken".to_string(), price: 0.0 };
    let mut heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));

    let mut send_status;
    // 循环接收 WebSocket 消息
    loop {
        tokio::select! {
            // 处理 WebSocket 消息
            message = websocket.try_next() => {
                if let Ok(Some(Message::Text(text))) = message {
                    if let Ok(event_data) = serde_json::from_str::<Value>(&text) {
                        if event_data["channel"] == "trade" {
                            if let Some(data_array) = event_data["data"].as_array() {
                                if let Some(trade_data) = data_array.get(0) {
                                    if let Some(price) = trade_data["price"].as_f64() {
                                        price_data.price = price;
                                        println!("Mark Price for {}-{} : {:.2} from kraken", pair.token1, pair.token2, price);

                                        // 检查价格变化是否超过阈值
                                        if (price - last_price).abs() > deviation_threshold {
                                            if let Err(e) = tx.send(price_data.clone()).await{
                                                eprintln!("发送价格时出错：{}", e);
                                                send_status = "failed";
                                            } else {
                                                println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                                last_price = price;
                                                send_status = "succeed";

                                                // 重置心跳定时器
                                                heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
                                            }
                                            conn.exec_drop(
                                                r"INSERT INTO DexRecord (dex_name, send_price, send_status, pair_name, timestamp) VALUES (:name, :send_price, :send_status, :pair_name, :timestamp)",
                                                params! {
                                                    "dex_name" => "KRAKEN",
                                                    "send_price" => price,
                                                    "send_status" => send_status,
                                                    "pair_name" => format!("{}-{}", pair.token1, pair.token2),
                                                    "timestamp" => Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                                }
                                            ).unwrap()
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 处理心跳定时器触发
            _ = heartbeat_timer.as_mut() => {
                tx.send(price_data.clone()).await?;
                println!("Heartbeat 触发推送: {:.2}", last_price);

                // 重置心跳定时器
                heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
            }
        }
    }
}

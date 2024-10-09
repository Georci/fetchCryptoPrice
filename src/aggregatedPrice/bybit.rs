use futures_util::{SinkExt, StreamExt, TryStreamExt};
use reqwest::Proxy;
use serde_json::{json, Value};
use std::error::Error;
use std::pin::Pin;
use chrono::Utc;
use mysql::{params, PooledConn};
use mysql::prelude::Queryable;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use reqwest_websocket::{Message, RequestBuilderExt};
use tokio_tungstenite::connect_async;
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;

pub async fn fetch_bybit_price(tx: mpsc::Sender<PriceData>, pair: CryptoPair, conn: &mut PooledConn) -> Result<(), Box<dyn Error>> {
    // 设置代理
    let proxy = Proxy::https("http://172.17.112.1:7890")?;
    // 创建带有代理的 reqwest 客户端
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .danger_accept_invalid_certs(true)
        .build()?;

    // 创建 WebSocket 连接
    let response = client.get("wss://stream-testnet.bybit.com/v5/public/spot").upgrade().send().await?;
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

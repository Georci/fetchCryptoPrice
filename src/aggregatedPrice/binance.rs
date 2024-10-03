use futures_util::{StreamExt, TryStreamExt};
use reqwest::Proxy;
use serde_json::Value;
use std::error::Error;
use std::pin::Pin;
use reqwest_websocket::{Message, RequestBuilderExt};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;

pub async fn fetch_binance_price(tx: mpsc::Sender<PriceData>, pair: CryptoPair) -> Result<(), Box<dyn Error>> {
    // 设置代理
    let proxy = Proxy::https("http://172.17.112.1:7890")?; // 写死的代理地址

    // 创建带有代理的 reqwest 客户端
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .danger_accept_invalid_certs(true)
        .build()?;

    // WebSocket URL
    let url = format!("wss://stream.binance.com:9443/ws/{}{}@trade", pair.token1, pair.token2).to_lowercase();
    println!("开始连接 Binance WebSocket: {}", url);

    // 创建 WebSocket 连接并升级
    let response = client.get(&url).upgrade().send().await?;
    let mut websocket = response.into_websocket().await?;

    println!("已连接到 Binance WebSocket {}/{} 流", pair.token1, pair.token2);

    let heartbeat = pair.heartbeat;
    let deviation_threshold = pair.deviation_threshold;
    let mut last_price: f64 = 0.0;

    // 初始心跳定时器
    let mut heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
    let mut price_data = PriceData {
        id: 1,
        source: "binance".to_string(),
        price: 0.0,
    };

    // 循环接收 WebSocket 消息
    loop {
        tokio::select! {
        // 处理 WebSocket 消息
        message = websocket.try_next() => {
            match message? {
                Some(Message::Text(text)) => {
                    // 解析 JSON 消息并提取价格
                    if let Ok(event_data) = serde_json::from_str::<Value>(&text) {
                        if let Some(price_str) = event_data["p"].as_str() {
                            let price = price_str.parse::<f64>().unwrap();
                            println!("Mark Price for {}-{} : {:.2} from binance", pair.token1, pair.token2, price);

                            // 检查价格变化是否超过阈值
                            price_data.price = price;
                            if (price - last_price).abs() > deviation_threshold {
                                if let Err(e) = tx.send(price_data.clone()).await {
                                    eprintln!("发送价格时出错: {:?}", e);
                                } else {
                                    println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                    last_price = price; // 更新上次推送的价格

                                    // 重置心跳计时器
                                    heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
                                }
                            }
                        }
                    }
                }
                Some(Message::Binary(_)) => println!("接收到意外的二进制数据"),
                _ => {},
            }
        }

        // 处理心跳定时器触发
        _ = heartbeat_timer.as_mut() => {
            if let Err(e) = tx.send(price_data.clone()).await {
                eprintln!("发送心跳推送时出错: {:?}", e);
            } else {
                println!("Heartbeat 触发推送: {:.2}", last_price);

                // 重置心跳计时器
                heartbeat_timer = Box::pin(sleep(Duration::from_secs(heartbeat)));
            }
        }
    }
    }
}

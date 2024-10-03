use std::error::Error;
use futures_util::{StreamExt, SinkExt};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use crate::aggregatedPrice::pricedata::PriceData;
use crate::aggregatedPrice::cryptopair::CryptoPair;

pub async fn fetch_okx_mark_price(tx: mpsc::Sender<PriceData>, pair: CryptoPair) -> Result<(), Box<dyn Error>> {
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
                                    } else {
                                        println!("价格变化超过阈值，推送价格: {:.2} -> {:.2}", last_price, price);
                                        last_price = price; // 更新上次推送的价格

                                        // 重置心跳定时器
                                        heartbeat_timer = Box::pin(tokio::time::sleep(Duration::from_secs(heartbeat)));
                                    }
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

use mysql::{params, PooledConn};
use mysql::prelude::Queryable;
use tokio::sync::mpsc;
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;
use chrono::Utc;

// Ken：聚合各交易所的价格
pub async fn merge_price(mut merge_rx: mpsc::Receiver<PriceData>, price_tx: mpsc::Sender<f64>, crypto_pair:&CryptoPair, conn:&mut PooledConn) -> Result<(), Box<dyn std::error::Error>> {
    let mut price_datas = vec![];

    let mut send_status;
    // 不断地从异步通道中获取多个交易所的价格
    while let Some(received_price_data) = merge_rx.recv().await {
        println!("Received price from source:{}-{}: {}", &crypto_pair.token1, &crypto_pair.token2, received_price_data);
        price_datas.push(received_price_data);

        // 聚合的逻辑(todo!:这里聚合的逻辑肯定还差一点的，第一什么时候进行聚合？直接以数组长度来判断还是太粗糙了，等三个交易所的数据全部到齐一组再推？以及是否需要定义一个推送到main的heartbeat)
        if price_datas.len() > 2 {
            // 取中位数
            let mut prices:Vec<f64> = price_datas.iter().map(|data| data.price).collect();
            prices.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let aggregated_price = if prices.len() % 2 == 1 {
                prices[prices.len() / 2]
            } else {
                let mid = prices.len() / 2;
                (prices[mid - 1] + prices[mid]) / 2.0
            };
            // 将聚合结果发送到 main，并处理发送错误
            match price_tx.send(aggregated_price).await {
                Ok(_) => {
                    println!("Aggregated price sent successfully: {:.2}", aggregated_price);
                    send_status = "succeed";
                }
                Err(e) => {
                    eprintln!("Failed to send aggregated price: {:?}", e);
                    send_status = "failed";
                    price_datas.clear(); // 如果发送失败，可以选择清空数据
                    break; // 如果发送失败，退出循环避免继续发送
                }
            }
            // 清空已处理的价格
            price_datas.clear();
            send_status = "succeed";
            conn.exec_drop(
                r"INSERT INTO AggregatedPrice (pair_name, aggregated_price, send_status, timestamp) VALUES (:pair_name, :aggregated_price, :send_status, :timestamp)",
                params! {
                    "pair_name" => format!("{}-{}", &crypto_pair.token1, &crypto_pair.token2),
                    "aggregated_price" => aggregated_price,
                    "send_status" => send_status,
                    "timestamp" => Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                },
            ).unwrap();
        }
    }

    Ok(())
}
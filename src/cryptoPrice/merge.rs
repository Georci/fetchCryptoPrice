use tokio::sync::mpsc;
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;

// Ken：聚合各交易所的价格(但是估计要传入代币对的名称，同时传入的应该还有heartbeat、deviation threshold)
// Q1:但是如果隔一段时间访问一次，其实更新的速度可能就慢了，所以还是得使用ws一直监听，
//     监听的时候有两种情况可以被推送到外层函数merge_price，1.推送的时间到了(所以heartbeat、deviation还是进内层函数) 2.当前价格与上一次推送的价格差值过大
//  answer:现在暂定使用多线程
// Q2:解决传递数据的问题之后，但是底层函数获取价格之后怎样将价格数据传递到该函数呢？或者换一个说法，当前函数怎么知道啥时候应该聚合价格呢？
//  answer:暂定使用异步通道吧，merge函数创建异步通道，fetch_price函数将价格推送到该通道中
pub async fn merge_price(mut merge_rx: mpsc::Receiver<PriceData>, price_tx: mpsc::Sender<f64>, crypto_pair:&CryptoPair) -> Result<(), Box<dyn std::error::Error>> {
    let mut price_datas = vec![];

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
            // 将聚合结果发送到 main，处理可能的发送错误
            if let Err(e) = price_tx.send(aggregated_price).await {
                eprintln!("Failed to send aggregated price: {:?}", e);
                price_datas.clear();;  // 如果发送失败，可以选择退出或者其他方式处理
            }
            // 清空已处理的价格
            price_datas.clear();
        }
    }

    Ok(())
}
use tokio::sync::mpsc;
use futures::StreamExt;
use ethers::{
    core::{types::TransactionRequest},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    utils,
    prelude::*
};
use eyre::Result;
use std::convert::TryFrom;
use dotenv::dotenv;
use std::env;
use std::str::FromStr;
use ethers::types::Bytes;
use hex_literal::hex;

use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;
mod aggregatedPrice; // 引入模块


static NTHREADS: i32 = 3;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crypto_pair1 = CryptoPair {
        token1: "BTC".to_string(),
        token2: "USDT".to_string(),
        heartbeat: 60,
        deviation_threshold: 5.0,
        oracle_contract: "0x837E1D61B95E8ed90563D8723605586E8f80D2BF".to_string(),
    };
    let crypto_pair2 = CryptoPair {
        token1: "ETH".to_string(),
        token2: "USDT".to_string(),
        heartbeat: 60,
        deviation_threshold: 5.0,
        oracle_contract:"0xf155f4e958c09506e418b75f6274a2f7c4faeaa4".to_string(),
    };
    let crypto_pair3 = CryptoPair {
        token1: "BNB".to_string(),
        token2: "USDT".to_string(),
        heartbeat: 60,
        deviation_threshold: 5.0,
        oracle_contract:"0xa0d6e07cd3daa0c29a1c6991dcd9f05445ea22bf".to_string(),
    };

    let crypto_pairs = vec![crypto_pair1, crypto_pair2, crypto_pair3];

    for crypto_pair in crypto_pairs {
        // 这个通道的tx是merge函数，rx是发送交易的函数
        let (price_tx, mut price_rx) = mpsc::channel::<f64>(32); // 用于接收聚合后的价格
        let price_tx = price_tx.clone();

        // 这个通道的tx是多个fetch_price函数，rx是merge函数
        let (merge_tx, merge_rx) = mpsc::channel::<PriceData>(32); // 用于传递原始价格
        let merge_tx_clone = merge_tx.clone();

        let crypto_pair_clone = crypto_pair.clone(); // 克隆 crypto_pair

        // 启动聚合任务
        tokio::spawn(async move {
            if let Err(e) = merge_price(merge_rx, price_tx, &crypto_pair_clone).await {
                eprintln!("Error in merge for {}: {}", crypto_pair_clone, e);
            }
        });

        let crypto_pair_clone = crypto_pair.clone(); // 克隆 crypto_pair
        let decimal:u8 = 3;
        // 启动发送交易的任务
        tokio::spawn(async move {
            if let Err(e) = send_tx(price_rx, &crypto_pair_clone, decimal).await{
                eprintln!("Error in send for {}/{} price to {},{}", crypto_pair_clone.token1, crypto_pair_clone.token2, crypto_pair_clone.oracle_contract, e);
            }
        });

        // 启动 OKX 价格获取任务
        let crypto_pair_clone = crypto_pair.clone(); // 再次克隆 crypto_pair
        let merge_tx_clone = merge_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = aggregatedPrice::okx::fetch_okx_mark_price(merge_tx_clone, crypto_pair_clone).await {
                eprintln!("Error: {:?}", e);
            }
        });

        // 启动 Binance 价格获取任务
        let crypto_pair_clone = crypto_pair.clone(); // 再次克隆 crypto_pair
        let merge_tx_clone = merge_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = aggregatedPrice::binance::fetch_binance_price(merge_tx_clone, crypto_pair_clone).await {
                eprintln!("Error in binance for {:?}", e);
            };
        });

        // 启动 Kraken 价格获取任务
        let crypto_pair_clone = crypto_pair.clone(); // 再次克隆 crypto_pair
        let merge_tx_clone = merge_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = aggregatedPrice::kraken::fetch_kraken_price(merge_tx_clone, crypto_pair_clone).await {
                eprintln!("Error: {:?}", e);
            }
        });
    }
    // 保持程序运行，不让 main 提前结束
    tokio::signal::ctrl_c().await?;
    Ok(())
}


// Ken：聚合各交易所的价格(但是估计要传入代币对的名称，同时传入的应该还有heartbeat、deviation threshold)
// Q1:但是如果隔一段时间访问一次，其实更新的速度可能就慢了，所以还是得使用ws一直监听，
//     监听的时候有两种情况可以被推送到外层函数merge_price，1.推送的时间到了(所以heartbeat、deviation还是进内层函数) 2.当前价格与上一次推送的价格差值过大
//  answer:现在暂定使用多线程
// Q2:解决传递数据的问题之后，但是底层函数获取价格之后怎样将价格数据传递到该函数呢？或者换一个说法，当前函数怎么知道啥时候应该聚合价格呢？
//  answer:暂定使用异步通道吧，merge函数创建异步通道，fetch_price函数将价格推送到该通道中
async fn merge_price(mut merge_rx: mpsc::Receiver<PriceData>, price_tx: mpsc::Sender<f64>, crypto_pair:&CryptoPair) -> Result<(), Box<dyn std::error::Error>> {
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
            // 将聚合结果发送到 main
            price_tx.send(aggregated_price).await.unwrap();
            // 清空已处理的价格
            price_datas.clear();
        }
    }

    Ok(())
}

pub async fn send_tx(mut price_rx:mpsc::Receiver<f64>, crypto_pair:&CryptoPair, decimal:u8) -> Result<(), Box<dyn std::error::Error>> {

    while let Some(price) = price_rx.recv().await{
        let price = 111.22;
        let scale_factor = 10_u64.pow(decimal.into());
        let scaled_price = (price * scale_factor as f64).round() as u64;

        // 初始化32字节的 `data` 数组并填充 scaled_price
        let mut data = vec![0u8; 32];
        let scaled_price_bytes = scaled_price.to_be_bytes();
        data[24..32].copy_from_slice(&scaled_price_bytes); // 将 price 转换为大端字节序并填充

        // 添加函数选择器，前四个字节为函数选择器
        let mut call_data = Vec::new();
        let function_selector = hex!("82b8ebc7"); // 将函数选择器从 hex 字符串转换为字节数组
        call_data.extend_from_slice(&function_selector); // 添加函数选择器到 data 前面
        call_data.extend_from_slice(&data); // 添加编码后的 data

        // 转换为 `Bytes` 类型，准备发送
        let call_data = Bytes::from(call_data);
        println!("完整的调用数据是: {:?}", call_data);

        let contract_address = crypto_pair.oracle_contract.clone();
        let infura_api_key = "https://sepolia.infura.io/v3/d0584eed4fa442cf853cf9d81acef8c8";
        let provider = Provider::<Http>::try_from(infura_api_key).unwrap();

        let chain_id = provider.get_chainid().await?;
        println!("Using chain id: {}", chain_id);

        // define the signer
        // for simplicity replace the private key (without 0x), ofc it always recommended to load it from an .env file or external vault
        let private_key = "a1825c59ad9a0160630c3ae5839d0bdb2d0bfee13c44376e68f35a8747f120aa";
        let wallet: LocalWallet = private_key
            .parse::<LocalWallet>()?
            .with_chain_id(chain_id.as_u64());
        let to_address:H160 = contract_address.parse()?;

        // connect the wallet to the provider
        let client = SignerMiddleware::new(provider, wallet.clone());

        // craft the transaction
        // it knows to figure out the default gas value and determine the next nonce so no need to explicitly add them unless you want to
        let tx = TransactionRequest::new()
            .to(to_address)
            .value(U256::from(0))
            .data(call_data.clone())
            .from(wallet.address());
        // send it!
        let pending_tx = client.send_transaction(tx, None).await?;
        println!("tx sended!");

    }
    Ok(())
}

#[tokio::test]
pub async fn test_sendTx() {
    let crypto_pair1 = CryptoPair {
        token1: "BTC".to_string(),
        token2: "USDT".to_string(),
        heartbeat: 60,
        deviation_threshold: 5.0,
        oracle_contract: "0x837E1D61B95E8ed90563D8723605586E8f80D2BF".to_string(),
    };
    let crypto_pair_clone = crypto_pair1.clone(); // 克隆 crypto_pair
    let decimal:u8 = 3;

    let (price_tx, mut price_rx) = mpsc::channel::<f64>(32); // 用于接收聚合后的价格

    send_tx(price_rx, &crypto_pair_clone, decimal).await.expect("send tx failed");
}


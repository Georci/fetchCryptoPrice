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
mod transaction;
mod cryptoPrice;

// todo!:10.2至少完成两件事，第一件事测试当前代码是否能够完成既定目标，第二件事将代码进行模块化、封装化
// todo!:10.3负责将整个系统的流程跑起来
// todo!:10.4准备汇报事宜

static NTHREADS: i32 = 3;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let crypto_pairs = default_crypto_pair();

    for crypto_pair in crypto_pairs {
        // 这个通道的tx是merge函数，rx是发送交易的函数
        let (price_tx, mut price_rx) = mpsc::channel::<f64>(32); // 用于接收聚合后的价格

        // 这个通道的tx是多个fetch_price函数，rx是merge函数
        let (merge_tx, merge_rx) = mpsc::channel::<PriceData>(32); // 用于传递原始价格
        let merge_tx_clone = merge_tx.clone();

        let crypto_pair_clone = crypto_pair.clone(); // 克隆 crypto_pair

        // 启动聚合任务
        tokio::spawn(async move {
            if let Err(e) = cryptoPrice::merge::merge_price(merge_rx, price_tx, &crypto_pair_clone).await {
                eprintln!("Error in merge for {}: {}", crypto_pair_clone, e);
            }
        });

        let crypto_pair_clone = crypto_pair.clone(); // 克隆 crypto_pair
        let decimal:u8 = 3;
        // 启动发送交易的任务
        tokio::spawn(async move {
            while let Some(aggregated_price) = price_rx.recv().await {
                println!("Received aggregated price: {}", aggregated_price);
                // 这里添加发送交易的逻辑
                if let Err(e) = transaction::sendtx::send_tx_with_rpc(aggregated_price, &crypto_pair_clone, decimal, &crypto_pair_clone.api_key).await {
                    eprintln!("Error in send for {}/{} price to {},{}", crypto_pair_clone.token1, crypto_pair_clone.token2, crypto_pair_clone.oracle_contract, e);
                }
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

//创建默认的代币对
fn default_crypto_pair() -> Vec<CryptoPair> {
    let crypto_pair1 = CryptoPair::new(
        "BTC",
        "USDT",
        60,
        200.0,
        "0x837E1D61B95E8ed90563D8723605586E8f80D2BF",
        "https://sepolia.infura.io/v3/d0584eed4fa442cf853cf9d81acef8c8"
    );
    let crypto_pair2 = CryptoPair::new(
        "ETH",
        "USDT",
        60,
        5.0,
        "0xf155f4e958c09506e418b75f6274a2f7c4faeaa4",
        "https://sepolia.infura.io/v3/cf20c46fa7024fcc94d981f817b45eec"
    );
    let crypto_pair3 = CryptoPair::new(
        "BNB",
        "USDT",
        60,
        3.0,
        "0xa0d6e07cd3daa0c29a1c6991dcd9f05445ea22bf",
        "https://sepolia.infura.io/v3/00983e02ce59404a99a647b68ec58889"
    );

    let crypto_pairs = vec![crypto_pair1, crypto_pair2, crypto_pair3];
    crypto_pairs
}











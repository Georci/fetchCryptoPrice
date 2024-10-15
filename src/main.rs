use tokio::sync::mpsc;
use futures::StreamExt;
use ethers::{
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    prelude::*
};
use eyre::Result;
use std::convert::TryFrom;
use dotenv::dotenv;
use std::env;
use std::error::Error;
use std::str::FromStr;
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;
mod aggregatedPrice; // 引入模块
mod transaction;
mod cryptoPrice;
use std::sync::Arc;
use log::error;
use mysql::{params, Opts, Pool, PooledConn};
use mysql::prelude::Queryable;
use tokio::task::JoinHandle;
use crate::aggregatedPrice::Exchange::Exchange;
use crate::aggregatedPrice::bybit::BYBIT;
use crate::aggregatedPrice::okx::OKX;
use crate::aggregatedPrice::binance::BINANCE;

/// todo!:我这里还有几个点需要解决：1对整体代码进行解耦、封装 2.考虑使用更好的聚合价格规则 3.rust编程

static NTHREADS: i32 = 3;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    dotenv().ok();
    let _sql_url = env::var("MYSQL_URL").unwrap_or_else(|_| env::var("mysql_url").expect("MYSQL_URL is not set"));
    let opts = Opts::from_url(&_sql_url)?;
    let pool = Arc::new(Pool::new(opts).unwrap()); // 使用 Arc 包裹 Pool

    let proxy_url = env::var("PROXY_URL").expect("PROXY_URL is not set");

    let crypto_pairs = aggregatedPrice::cryptopair::default_crypto_pair();

    for crypto_pair in crypto_pairs {
        /// 这个通道的tx是merge函数，rx是发送交易的函数
        let (price_tx, mut price_rx) = mpsc::channel::<f64>(32); // 用于接收聚合后的价格

        /// 这个通道的tx是多个fetch_price函数，rx是merge函数
        let (merge_tx, merge_rx) = mpsc::channel::<PriceData>(32); // 用于传递原始价格
        let merge_tx_clone = merge_tx.clone();

        let crypto_pair_clone = crypto_pair.clone(); // 克隆 crypto_pair

        // 启动聚合任务
        let pool_clone = Arc::clone(&pool); // 克隆 Arc<Pool>，让每个任务使用自己的连接
        tokio::spawn(async move {
            let mut conn = pool_clone.get_conn().unwrap();
            if let Err(e) = cryptoPrice::merge::merge_price(merge_rx, price_tx, &crypto_pair_clone, &mut conn).await {
                eprintln!("Error in merge for {}: {}", crypto_pair_clone, e);
            }
        });

        let crypto_pair_clone = crypto_pair.clone(); // 克隆 crypto_pair
        let pool_clone = Arc::clone(&pool); // 克隆 Arc<Pool>，让每个任务使用自己的连接
        // 启动发送交易的任务
        tokio::spawn(async move {
            while let Some(aggregated_price) = price_rx.recv().await {
                let mut conn = pool_clone.get_conn().unwrap();
                println!("Received aggregated price: {}", aggregated_price);
                // 这里添加发送交易的逻辑
                if let Err(e) = transaction::sendtx::send_tx_with_rpc(aggregated_price, &crypto_pair_clone, &crypto_pair_clone.api_key, &mut conn).await {
                    eprintln!("Error in send for {}/{} price to {},{}", crypto_pair_clone.token1, crypto_pair_clone.token2, crypto_pair_clone.oracle_contract, e);
                }
            }
        });

        // 启动 OKX 价格获取任务
        let pool_clone = Arc::clone(&pool); // 克隆 Arc<Pool>，让每个任务使用自己的连接
        let crypto_pair_clone = crypto_pair.clone(); // 再次克隆 crypto_pair
        let merge_tx_clone = merge_tx.clone();
        tokio::spawn(async move {
            let mut conn = pool_clone.get_conn().unwrap();
            let mut okx = OKX::new("http://172.17.112.1:7890", "Okx Exchange".to_string(), 1, crypto_pair_clone).unwrap();
            okx.connect("wss://ws.okx.com:8443/ws/v5/public")
                .await
                .expect("connect failed");

            if let Err(e) = okx.fetch_price(merge_tx_clone, &mut conn).await {
                eprintln!("Error: {:?}", e);
            }
        });

        // 启动 Binance 价格获取任务
        let pool_clone = Arc::clone(&pool); // 克隆 Arc<Pool>，让每个任务使用自己的连接
        let crypto_pair_clone = crypto_pair.clone(); // 再次克隆 crypto_pair
        let merge_tx_clone = merge_tx.clone();
        tokio::spawn(async move {
            let mut conn = pool_clone.get_conn().unwrap();
            let mut binance = BINANCE::new("http://172.17.112.1:7890", "Binance Exchange".to_string(), 2, crypto_pair_clone).unwrap();
            let url_string= format!("wss://stream.binance.com:9443/ws/{}{}@trade", binance.pair.token1, binance.pair.token2).to_lowercase();
            let url = url_string.as_str();
            binance.connect(url)
                .await
                .expect("connect failed");

            if let Err(e) = binance.fetch_price(merge_tx_clone, &mut conn).await {
                eprintln!("Error: {:?}", e);
            };
        });

        // 启动 bybit 价格获取任务
        let pool_clone = Arc::clone(&pool); // 克隆 Arc<Pool>，让每个任务使用自己的连接
        let crypto_pair_clone = crypto_pair.clone(); // 再次克隆 crypto_pair
        let merge_tx_clone = merge_tx.clone();
        let proxy_url_clone = proxy_url.clone();
        // tokio::spawn(async move {
        //     let mut conn = pool_clone.get_conn().unwrap();
        //     let mut bybit = BYBIT::new("http://172.17.112.1:7890", "Bybit Exchange".to_string(), 3, crypto_pair_clone).unwrap();
        //     bybit.connect("wss://stream.bybit.com/v5/public/spot")
        //          .await
        //          .expect("connect failed");
        //
        //     if let Err(e) = bybit.fetch_price(merge_tx_clone, &mut conn).await {
        //         eprintln!("Error: {:?}", e);
        //     }
        // });

        let bybit_handle = start_exchange_task::<BYBIT,_>(
            pool_clone,
            crypto_pair_clone,
            merge_tx_clone,
            "BYBIT".to_string(),
            3,
            "wss://stream.bybit.com/v5/public/spot".to_string(),
            proxy_url_clone,
            BYBIT::new, // 传入构造函数
        ).await;


    }
    // 保持程序运行，不让 main 提前结束
    tokio::signal::ctrl_c().await?;
    Ok(())
}


pub async fn start_exchange_task<E, F>(
    pool: Arc<Pool>,
    crypto_pair: CryptoPair,
    merge_tx: mpsc::Sender<PriceData>,
    exchange_name: String,
    exchange_id: u8,
    ws_url: String,
    proxy_url: String,
    exchange_constructor: F,
) -> JoinHandle<()>
where
    E: Exchange + Send + 'static,
    F: Fn(&str, String, u8, CryptoPair) -> Result<E, Box<dyn Error>> + Send + Sync + 'static,
{
    let pool_clone = Arc::clone(&pool);
    let crypto_pair_clone = crypto_pair.clone();
    let merge_tx_clone = merge_tx.clone();
    println!("wuxizhi1");
    tokio::spawn(async move {
        if let Err(e) = async {
            let mut conn = pool_clone.get_conn().map_err(|e| {
                error!("[{}] 获取数据库连接失败: {:?}", exchange_name, e);
                e
            })?;

            // 使用传入的构造函数创建交易所实例
            let mut exchange = exchange_constructor(
                &*proxy_url,
                exchange_name.to_string(),
                exchange_id,
                crypto_pair_clone.clone(),
            ).map_err(|e| {
                    error!("[{}] 创建交易所实例失败: {:?}",exchange_name, e);
                    e
                })?;

            exchange.connect(&ws_url).await.map_err(|e| {
                error!("[{}] 连接 WebSocket 失败: {:?}",exchange_name, e);
                e
            })?;

            exchange.fetch_price(merge_tx_clone, &mut conn)
                .await
                .map_err(|e| {
                    error!("[{}] 获取价格失败: {:?}",exchange_name, e);
                    e
                })?;

            Ok::<(), Box<dyn std::error::Error>>(())
        }.await
        {
            error!("[{}] 任务执行失败: {:?}", exchange_name, e);
        }
    })
}








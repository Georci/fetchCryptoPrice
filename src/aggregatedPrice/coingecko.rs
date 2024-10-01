use std::time::Duration;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::Proxy;
use tokio::sync::mpsc;
use crate::aggregatedPrice::cryptopair::CryptoPair;

mod constants {
    #[derive(Debug)]
    pub struct ErrorResponse {
        pub error: String,
        pub status: String,
    }

    impl std::fmt::Display for ErrorResponse {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Error {}: {}", self.status, self.error)
        }
    }
}

pub async fn fetch_coingecko_price(tx: mpsc::Sender<f64>, pair: CryptoPair) {
    // 因为不支持ws，所以我们用循环实现 假长连接
    loop {
        let mut token1 = pair.token1.clone();
        let mut token2 = pair.token2.clone();
        let price = get_price(&token1,&token2).await;
        match price {
            Ok(price_usd) => {
                println!("Price: {:?}", price_usd);
            }
            Err(error) => {
                println!("Error: {}", error);
            }
        }
    }
}

async fn get_price(token1: &str, token2:&str) -> Result<f64, reqwest::Error> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}",
        token1,
        token2
    ).to_lowercase();

    // 创建 headers 并添加 API key
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("x-cg-demo-api-key", HeaderValue::from_static("CG-2arx6wF7DWhZaqPQgrk3VKe9"));

    // 设置代理
    let proxy = Proxy::https("http://172.17.112.1:7890")?;  // 替换为你的代理地址

    // 创建带有代理的客户端
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .build()?;

    println!("client is {:?}", client);
    println!("url is {}",url);
    let res: serde_json::Value = client
        .get(&url)
        .send()
        .await?
        .json()
        .await?;

    Ok(res[token1][token2].to_owned().as_f64().unwrap())
}

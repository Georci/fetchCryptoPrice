use std::fmt;

#[derive(Clone, Debug)]
pub struct CryptoPair {
    pub token1: String,
    pub token2: String,
    pub heartbeat: u64,
    pub deviation_threshold: f64,
    pub oracle_contract: String,
    pub decimal: u8,
    pub api_key: String,
    pub private_key: String,
}

impl CryptoPair {
    pub fn new(token1: &str, token2: &str, heartbeat: u64, deviation_threshold: f64, oracle_contract: &str, decimal: u8, api_key:&str, private_key:&str) -> Self {
        CryptoPair {
            token1: token1.to_string(),
            token2: token2.to_string(),
            heartbeat,
            deviation_threshold,
            oracle_contract: oracle_contract.to_string(),
            decimal,
            api_key: api_key.to_string(),
            private_key:private_key.to_string()
        }
    }

    pub fn display(&self) {
        println!("Token1: {}, Token2: {}, heartbeat: {}, deviation: {}, oracle: {}", self.token1, self.token2, self.heartbeat, self.deviation_threshold, self.oracle_contract);
    }
    
}

impl fmt::Display for CryptoPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "cryptopair token1 is {}, token2 is {}",
            self.token1, self.token2
        )
    }
}




//创建默认的代币对
pub fn default_crypto_pair() -> Vec<CryptoPair> {
    let crypto_pair1 = CryptoPair::new(
        "BTC",
        "USDT",
        60,
        200.0,
        "0x837E1D61B95E8ed90563D8723605586E8f80D2BF",
        3,
        "https://sepolia.infura.io/v3/d0584eed4fa442cf853cf9d81acef8c8",
        "a1825c59ad9a0160630c3ae5839d0bdb2d0bfee13c44376e68f35a8747f120aa"
    );
    let crypto_pair2 = CryptoPair::new(
        "ETH",
        "USDT",
        60,
        5.0,
        "0x950258baBd7be6a00887Ef9398f1aF32082c09BB",
        3,
        "https://sepolia.infura.io/v3/cf20c46fa7024fcc94d981f817b45eec",
        "93d6c14f4cc86537ee20884629f2e936453066104a4d555ffb8cf31bf4af7917"
    );
    let crypto_pair3 = CryptoPair::new(
        "BNB",
        "USDT",
        60,
        3.0,
        "0xa0d6e07cd3daa0c29a1c6991dcd9f05445ea22bf",
        3,
        "https://sepolia.infura.io/v3/00983e02ce59404a99a647b68ec58889",
        "68148ec7e4cc61385dc9ae3eaeaca457ff6cc5e9c9156e4d035ae6470ab7b491"
    );

    let crypto_pairs = vec![crypto_pair1, crypto_pair2, crypto_pair3];
    crypto_pairs
}

pub fn get_eth_usdc_pair() ->  Vec<CryptoPair>{
    let crypto_pair1 = CryptoPair::new(
        "ETH",
        "USDT",
        60,
        5.0,
        "0x950258baBd7be6a00887Ef9398f1aF32082c09BB",
        3,
        "https://sepolia.infura.io/v3/cf20c46fa7024fcc94d981f817b45eec",
        "93d6c14f4cc86537ee20884629f2e936453066104a4d555ffb8cf31bf4af7917"
    );
    let crypto_pairs = vec![crypto_pair1];
    crypto_pairs
}
use std::fmt;

#[derive(Clone)]
pub struct CryptoPair {
    pub token1: String,
    pub token2: String,
    pub heartbeat: u64,
    pub deviation_threshold: f64,
    pub oracle_contract: String,
    pub api_key: String,
    pub private_key: String,
}

impl CryptoPair {
    pub fn new(token1: &str, token2: &str, heartbeat: u64, deviation_threshold: f64, oracle_contract: &str, api_key:&str, private_key:&str) -> Self {
        CryptoPair {
            token1: token1.to_string(),
            token2: token2.to_string(),
            heartbeat,
            deviation_threshold,
            oracle_contract: oracle_contract.to_string(),
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

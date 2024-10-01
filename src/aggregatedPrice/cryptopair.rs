use std::fmt;

#[derive(Clone)]
pub struct CryptoPair {
    pub(crate) token1: String,
    pub(crate) token2: String,
    pub(crate) heartbeat: u64,
    pub(crate) deviation_threshold: f64,
    pub(crate) oracle_contract: String,
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

use std::fmt;
use crate::aggregatedPrice::cryptopair::CryptoPair;

#[derive(Clone,Debug)]
pub struct PriceData {
    pub id: u8,
    pub source: String,  // 来源，例如交易所名称
    pub price: f64,      // 价格
}

impl fmt::Display for PriceData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "price from: {}, id: {}, price: {}",
            self.source, self.id, self.price
        )
    }
}
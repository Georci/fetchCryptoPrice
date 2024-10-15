use std::error::Error;
use std::future::Future;
use mysql::PooledConn;
use tokio::sync::mpsc;
use crate::aggregatedPrice::cryptopair::CryptoPair;
use crate::aggregatedPrice::pricedata::PriceData;
use async_trait::async_trait;

#[async_trait]
pub trait Exchange {

    async fn connect(&mut self, wss_url: &str) -> Result<(), Box<dyn Error>>;

    async fn fetch_price(&mut self, tx: mpsc::Sender<PriceData>, conn: &mut PooledConn) -> Result<(), Box<dyn Error>>;
}

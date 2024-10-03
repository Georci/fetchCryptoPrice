use tokio::sync::mpsc;
use ethers::{
    core::{types::TransactionRequest},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    prelude::*
};
use std::convert::TryFrom;
use ethers::types::Bytes;
use hex_literal::hex;

use crate::aggregatedPrice::cryptopair::CryptoPair;

pub async fn send_tx(mut price_rx:mpsc::Receiver<f64>, crypto_pair:&CryptoPair, decimal:u8) -> Result<(), Box<dyn std::error::Error>> {

    while let Some(price) = price_rx.recv().await{
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
        let nonce = client.get_transaction_count(wallet.address(), None).await?;

        let tx = TransactionRequest::new()
            .to(to_address)
            .value(U256::from(0))
            .data(call_data.clone())
            .from(wallet.address())
            .nonce(nonce);
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
        api_key: "".to_string(),
    };
    let crypto_pair_clone = crypto_pair1.clone(); // 克隆 crypto_pair
    let decimal:u8 = 3;

    let (price_tx, mut price_rx) = mpsc::channel::<f64>(32); // 用于接收聚合后的价格

    send_tx(price_rx, &crypto_pair_clone, decimal).await.expect("send tx failed");
}

// 封装发送交易的函数
pub async fn send_tx_with_rpc(
    price:f64,
    crypto_pair: &CryptoPair,
    decimal: u8,
    infura_api_key: &str, // 使用 Arc 让多个任务共享同一个 Provider
) -> eyre::Result<(), Box<dyn std::error::Error>> {
    
        let scale_factor = 10_u64.pow(decimal.into());
        let scaled_price = (price * scale_factor as f64).round() as u64;

        let mut data = vec![0u8; 32];
        let scaled_price_bytes = scaled_price.to_be_bytes();
        data[24..32].copy_from_slice(&scaled_price_bytes);

        let mut call_data = Vec::new();
        let function_selector = hex!("82b8ebc7");
        call_data.extend_from_slice(&function_selector);
        call_data.extend_from_slice(&data);
        let call_data = Bytes::from(call_data);

        let provider = Provider::<Http>::try_from(infura_api_key)?;

        let contract_address = crypto_pair.oracle_contract.clone();
        let to_address: H160 = contract_address.parse()?;
        let chain_id = provider.get_chainid().await?;
        let private_key = "a1825c59ad9a0160630c3ae5839d0bdb2d0bfee13c44376e68f35a8747f120aa";
        let wallet: LocalWallet = private_key.parse::<LocalWallet>()?.with_chain_id(chain_id.as_u64());

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let nonce = client.get_transaction_count(wallet.address(), None).await?;
        // 获取当前区块的 base fee
        let block = provider.get_block(BlockId::Number(BlockNumber::Latest)).await?;
        let base_fee_per_gas = block
            .ok_or_else(|| eyre::eyre!("No block found"))?
            .base_fee_per_gas
            .ok_or_else(|| eyre::eyre!("No base fee found"))?;

        // 设置 maxFeePerGas 和 maxPriorityFeePerGas
        let max_fee_per_gas = base_fee_per_gas * 2; // 可以根据需求调整倍率
        let max_priority_fee_per_gas = U256::from(2_000_000_000u64); // 通常设置为 2 Gwei

        let tx = Eip1559TransactionRequest::new()
            .to(to_address)
            .value(U256::from(0))
            .data(call_data.clone())
            .from(wallet.address())
            .nonce(nonce)
            .max_fee_per_gas(max_fee_per_gas)
            .max_priority_fee_per_gas(max_priority_fee_per_gas);

        let pending_tx = client.send_transaction(tx, None).await?;
        println!("交易已发送，交易哈希: {:?}", pending_tx);

    Ok(())
}

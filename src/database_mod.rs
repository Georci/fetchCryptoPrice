use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use dotenv::dotenv;
use mysql::{params, Pool, PooledConn};
use mysql::prelude::Queryable;


pub fn test() -> PooledConn{
    let url = "mysql://ken:5753qwer@172.26.227.159:3306/offchain_oracle";
    let pool = Pool::new(url).unwrap();

    let conn = pool.get_conn().unwrap();
    conn
}

#[test]
pub fn test_addInfo() {
    let mut conn = test();

    conn.exec_drop(
        r"INSERT INTO DexRecord (dex_name, send_price, send_status) VALUES (:dex_name, :send_price, :send_status)",
        params! {
            "dex_name" => "Binance",       // 交易所名称
            "send_price" => 50000.0_f32,   // 推送价格
            "send_status" => "succeed",    // 发送状态
        },
    ).unwrap();
}

#[test]
pub fn delete_record() {
    let mut conn = test();

    conn.exec_drop(
        r"DELETE FROM DexRecord WHERE dex_name = :dex_name",
        params! {
            "dex_name" => "Binance",
        },
    ).unwrap();
}

#[test]
pub fn update_record() {
    let mut conn = test();

    conn.exec_drop(
        r"UPDATE DexRecord SET send_status = :new_status WHERE dex_name = :dex_name",
        params! {
            "new_status" => "failed",
            "dex_name" => "Binance",
        },
    ).unwrap();
}

#[test]
pub fn query_records() {
    let mut conn = test();

    conn.exec_drop(
        r"INSERT INTO DexRecord (dex_name, send_price, send_status) VALUES (:dex_name, :send_price, :send_status)",
        params! {
            "dex_name" => "Binance",       // 交易所名称
            "send_price" => 50000.0_f32,   // 推送价格
            "send_status" => "succeed",    // 发送状态
        },
    ).unwrap();

    let selected_records: Vec<(u32, String, f32, String)> = conn.query(
        "SELECT id, dex_name, send_price, send_status FROM DexRecord"
    ).unwrap();

    for (id, dex_name, send_price, send_status) in selected_records {
        println!("ID: {}, Dex: {}, Price: {}, Status: {}", id, dex_name, send_price, send_status);
    }
}

use std::io::Write;
use std::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    listen: String
}

impl Config {
    fn open() -> Result<Self> {
        let body = std::fs::read_to_string("config.json")
            .context("could not open config.json")?;
        Ok(serde_json::from_str::<Config>(body.as_str())?)
    }
}

fn time() -> Result<u64> {
    let off = 1704063600_000; // unix*1000:   1/1/2024 00:00
    Ok((SystemTime::now().duration_since(UNIX_EPOCH)?
        .as_millis() - off)
        .try_into()?)
}

// [time as LE u64], [time as BE u64]

fn accept_connection(listener: &TcpListener) -> Result<()> {
    let (mut stream, _) = listener.accept()?;
    let now = time()?;
    stream.write(&now.to_le_bytes())?;
    stream.write(&now.to_be_bytes())?;
    stream.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let cfg = Config::open()?;
    let listener = TcpListener::bind(cfg.listen.as_str())?;
    println!("Listening on {}", cfg.listen.as_str());
    loop {
        accept_connection(&listener)?;
    }
}

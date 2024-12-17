use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Write};
use std::sync::Arc;
use anyhow::{Context, Error};
use clickhouse::{Client, Row};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use anyhow::Result;

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
pub struct DBConfig {
    pub url: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
pub struct Config {
    pub listen: String,
    pub default_measure_interval_ms: u64,
    pub db: DBConfig,
}

// TODO: use proper logger
pub fn err(msg: &str) {
    eprintln!("ERR:  {msg}");
}

// TODO: use proper logger
pub fn log(msg: &str) {
    eprintln!("LOG:  {msg}");
}

// TODO: use proper logger
pub fn warn(msg: &str) {
    eprintln!("WARN: {msg}");
}


// ========================================================================== //

pub async fn add_dev(state: Arc<State>, dev: Device) -> anyhow::Result<()> {
    if get_dev(state.clone(), &dev.uuid).await.is_ok() {
        return Err(Error::msg("cannot register device with same uuid twice"));
    }

    let mut ins = state.db.inserter("devices.active")?;
    ins.write(&dev)?;
    ins.end().await?;

    Ok(())
}

pub async fn get_dev(state: Arc<State>, uuid: &Uuid) -> anyhow::Result<Device> {
    let dev = state.db.query(r"
            SELECT *
            FROM devices.active
            WHERE uuid = toUUID(?) AND now() > (last_alive + INTERVAL 10 SECONDS)")
        .bind(uuid.to_string())
        .fetch_one::<Device>()
        .await?;
    Ok(dev)
}

pub async fn update_dev(state: Arc<State>, dev: Device) -> anyhow::Result<()> {
    state.db.query(r"
            UPDATE devices.active
            SET type = ?, last_alive = ?
            WHERE uuid = ?")
        .bind(dev.r#type.as_str())
        .bind(dev.last_alive)
        .bind(dev.uuid.as_u64_pair())
        .execute()
        .await?;

    Ok(())
}

#[derive(Row, Serialize, Deserialize, Clone, Debug)]
pub struct Device {
    #[serde(with = "clickhouse::serde::uuid")]
    pub uuid: Uuid,
    #[serde(with = "clickhouse::serde::time::datetime")]
    pub last_alive: OffsetDateTime,
    pub r#type: String,
}

pub struct State {
    pub config: Config,
    pub db: Client
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("State{}")
    }
}

impl Config {
    pub fn connect_db(&self) -> Result<Client> {
        let mut db = Client::default()
            .with_url(self.db.url.as_str())
            .with_option("async_insert", "1")
            .with_option("wait_for_async_insert", "0");
        if let Some(x) = &self.db.user {
            db = db.with_user(x.as_str());
        }
        if let Some(x) = &self.db.password {
            db = db.with_password(x.as_str());
        }
        if let Some(x) = &self.db.headers {
            for (k, v) in x {
                db = db.with_header(k.as_str(), v.as_str());
            }
        }
        Ok(db)
    }

    pub fn open() -> Result<Self> {
        let body = std::fs::read_to_string("config.json")
            .context("could not open config.json")?;
        Ok(serde_json::from_str::<Config>(body.as_str())?)
    }
}
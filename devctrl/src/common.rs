use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Write};
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Context, Error};
use clickhouse::{Client, Row};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use anyhow::Result;
use rhai::Engine;
use crate::scripting::EventCallbacks;

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
            WHERE uuid = toUUID(?) AND now() < (last_alive + INTERVAL 10 SECONDS)")
        .bind(uuid.to_string())
        .fetch_one::<Device>()
        .await?;
    Ok(dev)
}

pub async fn update_dev(state: Arc<State>, dev: Device) -> anyhow::Result<()> {
    state.db.query(r"
            ALTER TABLE devices.active
            UPDATE type = ?, last_alive = ?
            WHERE uuid = toUUID(?)")
        .bind(dev.r#type.as_str())
        .bind(dev.last_alive.unix_timestamp())
        .bind(dev.uuid.to_string())
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
    pub db: Client,
    pub scripts: Mutex<Vec<Arc<EventCallbacks>>>,
    pub engine: Engine
}

impl State {
    pub fn clone_scripts(self: &Arc<Self>) -> Result<Vec<Arc<EventCallbacks>>> {
        if let Ok(scripts) = self.scripts.try_lock() {
            Ok(scripts.deref().clone())
        } else {
            Err(anyhow!("cannot lock scripts"))
        }
    }
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

/// broadcast data to all scripts
pub async fn on_data(state: Arc<State>, device: Option<&Uuid>, channel: &str, data: &[f32]) -> Result<()> {
    let scripts = state.clone_scripts()?;
    for x in scripts
        .into_iter()
        .map(|s| {
            let data = data.to_vec();
            let channel = channel.to_string();
            let device = device.map(|x| x.clone());
            let state = state.clone();
            tokio::spawn(async move {
                s.on_msg(state, device, channel, data)
            })
        }) {
        x.await??;
    }
    Ok(())
}

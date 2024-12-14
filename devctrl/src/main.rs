use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use serde::{Deserialize, Serialize};
use anyhow::{Error, Result, Context};
use clickhouse::{Client, Row};
use schemars::{schema_for, JsonSchema};
use time::OffsetDateTime;
use uuid::Uuid;
// ========================================================================== //

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
struct DBConfig {
    url: String,
    user: Option<String>,
    password: Option<String>,
    headers: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
struct Config {
    listen: String,
    dev_timeout_ms: u64,
    default_measure_interval_ms: u64,
    db: DBConfig,
}

// ========================================================================== //

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
enum RequestData {
    Startup {
        r#dev_type: String
    },

    KeepAlive,

    Transmit {
        channel: String,
        data: Vec<f32>
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct Request {
    uuid: String,
    rtc_unix: Option<i64>,
    data: RequestData
}

// ========================================================================== //

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
enum ResponseData {
    Err(String),

    Configure {
        sensor_interval: u64 
    },

    KeepAliveConfirm,

    Transmit {
        channel: String,
        data: Vec<f32>
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct Response {
    success: bool,
    rtc_unix: i64,
    data: ResponseData
}

// ========================================================================== //

// TODO: use proper logger
fn err(msg: &str) {
    eprintln!("ERR: {msg}");
}

// TODO: use proper logger
fn log(msg: &str) {
    eprintln!("LOG: {msg}");
}

async fn all_dev(state: Arc<State>) -> Result<Vec<Device>> {
    state.db.query(r"SELECT * FROM devices.active")
        .fetch_all()
}

async fn add_dev(state: Arc<State>, dev: Device) -> Result<()> {
    let count = state.db.query(r"
            SELECT CAST(COUNT() AS UInt32)
            FROM devices.active
            WHERE uuid = ?")
        .bind(dev.uuid.as_u64_pair())
        .fetch_one::<u32>()
        .await?;

    if count > 0 {
        return Err(Error::msg("cannot register device with same uuid twice"));
    }

    let mut ins = state.db.inserter("devices.active")?;
    ins.write(&dev)?;
    ins.end().await?;

    Ok(())
}

async fn get_dev(state: Arc<State>, uuid: &Uuid) -> Result<Device> {
    let dev = state.db.query(r"
            SELECT *
            FROM devices.active
            WHERE uuid = ?")
        .bind(uuid.as_u64_pair())
        .fetch_one::<Device>()
        .await?;
    Ok(dev)
}

async fn update_dev(state: Arc<State>, dev: Device) -> Result<()> {
    state.db.query(r"
            UPDATE devices.active
            SET type = ?, last_alive = ? WHERE uuid = ?")
        .bind(dev.r#type.as_str())
        .bind(dev.last_alive)
        .bind(dev.uuid.as_u64_pair())
        .execute()
        .await?;

    Ok(())
}

#[derive(Row, Serialize, Deserialize, Clone, Debug)]
struct Device {
    #[serde(with = "clickhouse::serde::uuid")]
    uuid: Uuid,
    #[serde(with = "clickhouse::serde::time::datetime")]
    last_alive: OffsetDateTime,
    r#type: String,
}

struct State {
    config: Config,
    db: Client
}

impl Config {
    fn connect_db(&self) -> Result<Client> {
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

    fn open() -> Result<Self> {
        let body = std::fs::read_to_string("config.json")
            .context("could not open config.json")?;
        Ok(serde_json::from_str::<Config>(body.as_str())?)
    }
}

async fn respond<T: Serialize>(stream: &mut TcpStream, data: &T) -> Result<()> {
    let json = serde_json::to_string(data)?;
    stream.write_all(json.as_bytes()).await?;
    Ok(())
}

async fn serve_inner(state: Arc<State>, stream: &mut TcpStream, data: &str) -> Result<()> {
    let req = serde_json::from_str::<Request>(data)?;
    let req_time = req.rtc_unix.ok_or(Error::msg("missing rtc_unix"))
        .and_then(|x| OffsetDateTime::from_unix_timestamp(x).map_err(|_| Error::msg("invalid rtc_unix")))
        .unwrap_or_else(|_| OffsetDateTime::now_utc());
    let uuid = Uuid::try_parse(req.uuid.as_str())?;

    match req.data {
        RequestData::Startup { dev_type } => {
            let dev = Device {
                uuid,
                last_alive: OffsetDateTime::now_utc(),
                r#type: dev_type
            };
            add_dev(state.clone(), dev).await?;

            let _ = respond(stream, &Response {
                success: true,
                rtc_unix: OffsetDateTime::now_utc().unix_timestamp(),
                data: ResponseData::Configure {
                    sensor_interval: state.config.default_measure_interval_ms
                }
            });
        }

        RequestData::KeepAlive => {
            let mut old = get_dev(state.clone(), &uuid).await?;
            old.last_alive = req_time;
            update_dev(state.clone(), old).await?;

            let _ = respond(stream, &Response {
                success: true,
                rtc_unix: OffsetDateTime::now_utc().unix_timestamp(),
                data: ResponseData::KeepAliveConfirm
            });
        }

        RequestData::Transmit { channel, data } => {
            err("device tried to transmit data via this service. not yet implemented");
            return Err(Error::msg("not yet implemented"))
        }
    }

    Ok(())
}

async fn serve(state: Arc<State>, stream: TcpStream) {
    let mut stream = stream;
    log(format!("connection: {:?}", stream.peer_addr()).as_str());

    let mut data = String::new();
    if let Err(_) = stream.read_to_string(&mut data).await {
        err("failed to read tcp data");
        return;
    }

    if let Err(e) = serve_inner(state, &mut stream, data.as_str()).await {
        err("request / response failed");
        let _ = respond(&mut stream, &Response {
            success: false,
            rtc_unix: OffsetDateTime::now_utc().unix_timestamp(),
            data: ResponseData::Err(e.to_string())
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::open().inspect_err(|_| {
        println!("expected config schema:");
        let schema = schema_for!(Config);
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    })?;
    let db = config.connect_db()?;
    let state = Arc::new(State {
        config,
        db
    });

    std::fs::write("response.schema.json",
                   serde_json::to_string_pretty(&schema_for!(Response))?)?;
    std::fs::write("request.schema.json",
                   serde_json::to_string_pretty(&schema_for!(Request))?)?;

    let listener = TcpListener::bind(state.config.listen.as_str()).await?;
    log(format!("listening on {}", state.config.listen).as_str());
    loop {
        let stream = listener.accept().await;
        if let Ok((stream, _)) = stream {
            tokio::spawn(serve(state.clone(), stream));
        } else {
            err("tcp stream connect err");
        }
    }
}

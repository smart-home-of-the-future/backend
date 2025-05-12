mod scripting;
mod common;

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, tcp::OwnedWriteHalf};
use serde::{Deserialize, Serialize};
use anyhow::{Error, Result, Context};
use clickhouse::{Client, Row};
use schemars::{schema_for, JsonSchema};
use time::OffsetDateTime;
use uuid::Uuid;
use common::*;
use crate::scripting::{add_script, create_engine};

// ========================================================================== //

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
enum RequestData {
    Startup {
        r#dev_type: String
    },

    /// transmit packets that are 5 seconds ago from the last transmit packet also act as KeepAlive
    /// startup also counts as KeepAlive
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

async fn respond<T: Serialize>(stream: &mut OwnedWriteHalf, data: &T) -> Result<()> {
    let json = serde_json::to_string(data)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

async fn serve_inner(state: Arc<State>, stream: &mut OwnedWriteHalf, data: &str, last_implicit_keepalive: &mut OffsetDateTime) -> Result<()> {
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
            if (req_time - *last_implicit_keepalive).as_seconds_f64() >= 5.0 {
                *last_implicit_keepalive = req_time;
                let mut old = get_dev(state.clone(), &uuid).await?;
                old.last_alive = req_time;
                update_dev(state.clone(), old).await?;
            }
            on_data(state.clone(), Some(&uuid), channel.as_str(), data.as_slice()).await?;
        }
    }

    Ok(())
}

async fn serve(state: Arc<State>, stream: TcpStream) {
    let addr = stream.peer_addr().unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    log(format!("connection: {:?}", addr).as_str());

    let mut last_implicit_keepalive = OffsetDateTime::now_utc();

    loop {
        let mut data = String::new();
        match reader.read_line(&mut data).await {
            Err(_) => {
                err("failed to read tcp data");
                return;
            }

            Ok(n) => {
                if n == 0 {
                    log("con done");
                    return;
                }
            }
        }

        if let Err(e) = serve_inner(state.clone(), &mut writer, data.as_str(), &mut last_implicit_keepalive).await {
            err(format!("request / response failed: {}", e).as_str());
            let _ = respond(&mut writer, &Response {
                success: false,
                rtc_unix: OffsetDateTime::now_utc().unix_timestamp(),
                data: ResponseData::Err(e.to_string())
            });
        }
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
        db,
        scripts: Mutex::new(Vec::new()),
        engine: create_engine(),
    });

    std::fs::write("response.schema.json",
                   serde_json::to_string_pretty(&schema_for!(Response))?)?;
    std::fs::write("request.schema.json",
                   serde_json::to_string_pretty(&schema_for!(Request))?)?;
    
    for dir in std::fs::read_dir("scripts")? {
        let path = dir?.path();
        let path = path.to_str().context("wtf")?;
        let str = std::fs::read_to_string(&path)?;
        add_script(state.clone(), str.as_str())?;
        log(format!("registered script {}", path).as_str());
    }

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

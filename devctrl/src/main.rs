mod scripting;
mod common;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, tcp::OwnedWriteHalf};
use serde::{Deserialize, Serialize};
use anyhow::{Error, Result, Context};
use clickhouse::{Client, Row};
use schemars::{schema_for, JsonSchema};
use time::OffsetDateTime;
use uuid::Uuid;
use common::*;

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

async fn respond<T: Serialize>(stream: &mut OwnedWriteHalf, data: &T) -> Result<()> {
    let json = serde_json::to_string(data)?;
    stream.write_all(json.as_bytes()).await?;
    Ok(())
}

async fn serve_inner(state: Arc<State>, stream: &mut OwnedWriteHalf, data: &str) -> Result<()> {
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
    let addr = stream.peer_addr().unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    log(format!("connection: {:?}", addr).as_str());

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

        log(data.as_str());

        if let Err(e) = serve_inner(state.clone(), &mut writer, data.as_str()).await {
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

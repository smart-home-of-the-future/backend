use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use serde::{Deserialize, Serialize};
use anyhow::{Error, Result, Context};
use schemars::{schema_for, JsonSchema};

// ========================================================================== //

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
struct Config {
    listen: String,
    dev_timeout_ms: u64,
    default_measure_interval_ms: u64,
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
    data: ResponseData
}

// ========================================================================== //

fn err(msg: &str) {
    eprintln!("ERR: {msg}");
}

fn log(msg: &str) {
    eprintln!("LOG: {msg}");
}

async fn all_dev(state: Arc<State>) -> Result<Vec<Device>> {
    let mut out = Vec::new();
    state.testing_devices.scan(|_, d| {
        if let Ok(elapsed) = d.last_alive.elapsed() {
            if elapsed.as_millis() < state.config.dev_timeout_ms.into() {
                out.push(d.clone())
            }
        }
    });
    Ok(out)
}

async fn add_dev(state: Arc<State>, dev: Device) -> Result<()> {
    if let Ok(d) = get_dev(state.clone(), dev.uuid.as_str()).await {
        if d.last_alive.elapsed()?.as_millis() < state.config.dev_timeout_ms.into() {
            return Err(Error::msg("cannot register device with same name twice"));
        }
    }
    let _ = state.testing_devices.insert_async(dev.uuid.clone(), dev).await;
    Ok(())
}

async fn get_dev(state: Arc<State>, uuid: &str) -> Result<Device> {
    let e = state.testing_devices.get_async(&uuid.to_string())
        .await
        .context("device not found")?;
    Ok(e.clone())
}

async fn update_dev(state: Arc<State>, dev: Device) -> Result<()> {
    let _ = state.testing_devices.insert_async(dev.uuid.clone(), dev).await;
    Ok(())
}

// ========================================================================== //

#[derive(Clone, Debug)]
struct Device {
    uuid: String,
    last_alive: std::time::SystemTime,
    dev_type: String,
}

struct State {
    config: Config,
    testing_devices: scc::HashMap<String, Device>,
}

fn open_config() -> Result<Config> {
    let body = std::fs::read_to_string("config.json")
        .context("could not open config.json")?;
    Ok(serde_json::from_str::<Config>(body.as_str())?)
}

async fn respond<T: Serialize>(stream: &mut TcpStream, data: &T) -> Result<()> {
    let json = serde_json::to_string(data)?;
    stream.write_all(json.as_bytes()).await?;
    Ok(())
}

async fn serve_inner(state: Arc<State>, stream: &mut TcpStream, data: &str) -> Result<()> {
    let req = serde_json::from_str::<Request>(data)?;

    match req.data {
        RequestData::Startup { dev_type } => {
            let dev = Device {
                uuid: req.uuid,
                last_alive: std::time::SystemTime::now(),
                dev_type
            };
            add_dev(state.clone(), dev).await?;

            let _ = respond(stream, &Response {
                success: true,
                data: ResponseData::Configure {
                    sensor_interval: state.config.default_measure_interval_ms
                }
            });
        }

        RequestData::KeepAlive => {
            let mut old = get_dev(state.clone(), req.uuid.as_str()).await?;
            old.last_alive = std::time::SystemTime::now();
            update_dev(state.clone(), old).await?;

            let _ = respond(stream, &Response {
                success: true,
                data: ResponseData::KeepAliveConfirm
            });
        }

        RequestData::Transmit { channel, data } => {
            err("device tried to transmit data via this service. not yet implemented");
            return Err(Error::msg("not yet implemented"))
        }
    }

    // TODO: REMOVE
    println!("all devices: {:#?}", all_dev(state.clone()).await?);

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
            data: ResponseData::Err(e.to_string())
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = open_config().inspect_err(|_| {
        println!("expected config schema:");
        let schema = schema_for!(Config);
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    })?;
    let state = Arc::new(State {
        config,
        testing_devices: scc::HashMap::new() 
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

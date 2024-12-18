use std::fmt::Debug;
use std::ops::DerefMut;
use std::sync::Arc;
use rhai::{Array, CustomType, Engine, FnPtr, TypeBuilder, AST};
use uuid::Uuid;
use anyhow::{anyhow, Result};
use crate::common;
use crate::common::State;

#[derive(Clone)]
pub struct EventCallbacks {
    ast: AST,
    on_msg: Option<FnPtr>,
    only_listen: Option<Vec<String>>,
}


#[derive(Debug, Clone)]
struct RhaiCtx {
    ctx: Arc<State>
}

impl CustomType for RhaiCtx {
    fn build(mut builder: TypeBuilder<Self>) {
        builder
            .with_name("CTX")
            .with_fn("log", |_: &mut Self, msg: &str| {
                common::log(msg);
            }).with_fn("warn", |_: &mut Self, msg: &str| {
                common::warn(msg);
            }).with_fn("err", |_: &mut Self, msg: &str| {
                common::err(msg);
            });
    }
}

impl EventCallbacks {
    pub fn on_msg(&self, ctx: Arc<State>, device: Option<Uuid>, channel: String, data: Vec<f32>) -> Result<()> {
        if let Some(on_msg) = &self.on_msg {
            if let Some(only) = &self.only_listen {
                if !only.contains(&channel) {
                    return Ok(());
                }
            }
            let device = device.map(|x| x.to_string());
            on_msg.call(&ctx.engine, &self.ast, (RhaiCtx { ctx: ctx.clone() }, device, channel, data))?;
        }
        Ok(())
    }
    
    fn from_str(source: &str, engine: &Engine) -> Result<Self> {
        let ast = engine.compile(source)?;
        let mut callbacks = engine.eval_ast::<rhai::Map>(&ast)?;
        Ok(EventCallbacks {
            ast,
            
            on_msg: callbacks.remove("on_msg")
                .and_then(|x| x.try_cast::<FnPtr>()),
            
            only_listen: callbacks.remove("only_channels")
                .and_then(|x| x.try_cast::<Array>())
                .and_then(|x| x.into_iter()
                    .map(|x| x.try_cast::<String>())
                    .collect()),
        })
    }
}

pub fn create_engine() -> Engine {
    let mut engine = Engine::new();
    engine.register_type::<RhaiCtx>();
    engine.on_debug(|text, _, _| common::log(text));
    engine.on_print(|text| common::log(text));
    engine
}

pub fn add_script(state: Arc<State>, script: &str) -> Result<()> {
    let script = EventCallbacks::from_str(script, &state.engine)?;
    if let Ok(mut lock) = state.scripts.lock() {
        lock.deref_mut().push(Arc::new(script));
        Ok(())
    } else { 
        Err(anyhow!("Script lock is poisoned"))
    }
}
use std::sync::Arc;
use rhai::{Array, CustomType, Dynamic, Engine, EvalAltResult, FnPtr, TypeBuilder, AST};
use uuid::Uuid;
use anyhow::Result;
use crate::common;
use crate::common::State;

#[derive(Debug, Clone)]
pub struct EventCallbacks {
    ast: AST,
    on_msg: Option<FnPtr>,
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
    pub fn on_msg(&self, engine: &Engine, ctx: Arc<State>, device: &Uuid, channel: String, data: &[f32]) -> Result<()> {
        if let Some(on_msg) = &self.on_msg {
            let ctx = RhaiCtx { ctx };
            let device = device.to_string();
            on_msg.call(engine, &self.ast, (ctx, device, channel, data.to_vec()))?;
        }
        Ok(())
    }
    
    pub fn from_str(source: &str, engine: &Engine) -> Result<Self> {
        let ast = engine.compile(source)?;
        let mut callbacks = engine.eval_ast::<rhai::Map>(&ast)?;
        Ok(EventCallbacks {
            ast,
            on_msg: callbacks.remove("on_msg")
                .and_then(|x| x.try_cast::<FnPtr>())
        })
    }
}

pub fn create_engine() -> Engine {
    let mut engine = Engine::new();
    engine.register_type::<RhaiCtx>();
    engine
}
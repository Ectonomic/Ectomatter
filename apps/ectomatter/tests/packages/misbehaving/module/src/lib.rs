//! A plugin that misbehaves on purpose, each way a package can: it loops
//! forever (`spin`), takes all the memory it can (`grow`), crashes (`crash`),
//! or works for a while first (`slow`); and otherwise answers (`ping`), says
//! how many times it has been met since it last started (`met`), and, on a
//! timer, spins (`spin-later`).

use std::cell::Cell;

use serde_json::{Value, json};

wit_bindgen::generate!({ path: "../../../../../../wit", world: "core" });

use ectomatter::core::host;

thread_local! {
    static MET: Cell<u32> = const { Cell::new(0) };
}

/// A loop the compiler can't take out.
fn spin_for(limit: Option<u64>) -> u64 {
    let mut n: u64 = 0;
    loop {
        n = std::hint::black_box(n.wrapping_add(1));
        if limit.is_some_and(|l| n >= l) {
            return n;
        }
    }
}

fn answer(method: &str, p: &Value) -> Result<Value, String> {
    match method {
        "initialize" => {
            MET.with(|m| m.set(m.get() + 1));
            ectomatter_protocol::meet(p, "Misbehaving", env!("CARGO_PKG_VERSION"))
        }
        "ping" => Ok(json!("pong")),
        "met" => Ok(json!(MET.with(Cell::get))),
        "spin" => Ok(json!(spin_for(None))),
        "slow" => Ok(json!(spin_for(Some(p["loops"].as_u64().unwrap_or(1_000_000))))),
        "grow" => {
            let mut all: Vec<Vec<u8>> = Vec::new();
            loop {
                all.push(vec![1u8; 16 << 20]);
                std::hint::black_box(&all);
            }
        }
        "crash" => panic!("asked to crash"),
        "spin-later" => {
            host::after(0, 1);
            Ok(Value::Null)
        }
        "shutdown" => Ok(Value::Null),
        _ => Err(format!("Misbehaving doesn’t answer {method}.")),
    }
}

struct Plugin;

impl Guest for Plugin {
    fn call(method: String, params: String) -> Result<String, String> {
        let p: Value = if params.is_empty() { Value::Null } else { serde_json::from_str(&params).map_err(|e| format!("{method}: {e}"))? };
        answer(&method, &p).map(|v| v.to_string())
    }

    fn timer(_tag: u32) {
        spin_for(None);
    }
}

export!(Plugin);

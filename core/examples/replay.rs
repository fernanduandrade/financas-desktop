//! Repete um roteiro de chamadas (scenario.json) e grava as respostas, para comparar com a versão Python.
//! Uso: cargo run -p financas-core --example replay -- scenario.json saida.json
use chrono::NaiveDateTime;
use financas_core::{api, dates, reminders, Db};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

fn sub(v: &Value, vars: &HashMap<String, Value>) -> Value {
    match v {
        Value::String(s) if s.starts_with('@') => vars[&s[1..]].clone(),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, x)| (k.clone(), sub(x, vars))).collect())
        }
        other => other.clone(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let work = std::env::temp_dir().join(format!("financas-replay-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let mut db = Db::open(&work.join("financas.db"), &work).unwrap();
    db.set_today(Some(dates::parse("2026-11-25").unwrap()));
    let seed = financas_core::seed();
    db.first_run_import(&seed).unwrap();

    let steps: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let mut vars: HashMap<String, Value> = HashMap::new();
    let mut out = Vec::new();
    let mut last_export: Option<Value> = None;
    for st in steps {
        let m = st["method"].as_str().unwrap();
        let mut path = st["path"].as_str().unwrap().to_string();
        for (k, v) in &vars {
            let vs = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            path = path.replace(&format!("@{k}"), &vs);
        }
        let body = st.get("body").map(|b| sub(b, &vars));
        let res: Result<Value, String> = match m {
            "REMIND" => {
                let now = NaiveDateTime::parse_from_str(&path, "%Y-%m-%dT%H:%M:%S").unwrap();
                reminders::check_reminders(&db, now, st.get("force").and_then(|f| f.as_str())).map(|ns| {
                    Value::Array(ns.into_iter().map(|n| json!({
                        "title": n.title, "message": n.message, "kind": n.kind,
                        "actions": n.actions.iter().map(|a| json!([a.label, a.route])).collect::<Vec<_>>()
                    })).collect())
                })
            }
            "POSTBACKUP" => db
                .import_all(last_export.as_ref().unwrap().as_object().unwrap())
                .map(|_| json!({"ok": true})),
            _ => api::route(&db, m, &path, body),
        };
        let res = match res {
            Ok(v) => v,
            Err(e) => json!({"__error__": e}),
        };
        if m == "GET" && path == "backup" {
            last_export = Some(res.clone());
        }
        if let (Some(name), Some(id)) = (st.get("save").and_then(|s| s.as_str()), res.get("id")) {
            vars.insert(name.to_string(), id.clone());
        }
        out.push(res);
    }
    let _: Map<String, Value> = Map::new();
    std::fs::write(&args[2], serde_json::to_string(&out).unwrap()).unwrap();
    println!("ok {}", out.len());
}

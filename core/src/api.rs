//! Roteador da API: as mesmas rotas do `server.py`, mas chamadas direto (sem HTTP).
//! O frontend continua chamando `api(method, path, body)`; só muda o transporte.
use crate::db::{as_int, need, Db, Obj, R};
use serde_json::{json, Value};
use std::collections::HashMap;

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or(())
                {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// parse_qs do Python: ignora valores vazios e fica com o primeiro de cada chave.
pub fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in q.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        let (k, v) = (pct_decode(k), pct_decode(v));
        if !v.is_empty() {
            out.entry(k).or_insert(v);
        }
    }
    out
}

fn body_obj(body: &Option<Value>) -> Obj {
    match body {
        Some(Value::Object(o)) => o.clone(),
        _ => Obj::new(),
    }
}

fn qint(q: &HashMap<String, String>, k: &str) -> R<Option<i64>> {
    match q.get(k) {
        None => Ok(None),
        Some(s) => s
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("valor inválido em '{k}'")),
    }
}

fn qreq<'a>(q: &'a HashMap<String, String>, k: &str) -> R<&'a str> {
    q.get(k)
        .map(|s| s.as_str())
        .ok_or_else(|| format!("campo obrigatório: '{k}'"))
}

fn rid_or_err(rid: Option<i64>) -> R<i64> {
    rid.ok_or_else(|| "rota inválida".to_string())
}

/// Atende uma chamada. `None` vira `{"ok": true}`, como no servidor Python.
pub fn route(db: &Db, method: &str, path: &str, body: Option<Value>) -> R<Value> {
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let q = parse_query(query);
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let r = parts[0];
    let rid = parts
        .get(1)
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse::<i64>().ok());
    let m = method.to_ascii_uppercase();
    let ok = || Ok(json!({"ok": true}));

    match (r, m.as_str()) {
        ("summary", "GET") => return Ok(Value::Object(db.summary()?)),
        ("meta", "GET") => {
            return Ok(json!({
                "accounts": db.list_accounts(true)?,
                "categories": db.list_categories()?,
                "settings": db.get_settings()?,
                "today": crate::dates::iso(db.today()),
                "data_dir": db.data_dir.to_string_lossy(),
            }))
        }
        _ => {}
    }

    match r {
        "transactions" => match m.as_str() {
            "GET" => {
                return Ok(Value::Object(db.list_transactions(
                    qreq(&q, "start")?,
                    qreq(&q, "end")?,
                    qint(&q, "account")?,
                    qint(&q, "category")?,
                    q.get("q").map(|s| s.as_str()),
                )?))
            }
            "POST" => return Ok(json!({"id": db.create_transaction(&body_obj(&body))?})),
            "PUT" => {
                db.update_transaction(
                    rid_or_err(rid)?,
                    &body_obj(&body),
                    q.get("scope").map(|s| s.as_str()).unwrap_or("one"),
                )?;
                return ok();
            }
            "DELETE" => {
                db.delete_transaction(
                    rid_or_err(rid)?,
                    q.get("scope").map(|s| s.as_str()).unwrap_or("one"),
                )?;
                return ok();
            }
            _ => {}
        },
        "transfers" if m == "POST" => {
            let b = body_obj(&body);
            if need(&b, "from_id")? == need(&b, "to_id")? {
                return Err("Escolha contas diferentes".into());
            }
            let amount = as_int(Some(need(&b, "amount_cents")?)).ok_or("valor inválido")?;
            let desc = b
                .get("description")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("Transferência");
            let paid = b.get("paid").cloned().unwrap_or(json!(1));
            let id = db.create_transfer(
                &b["from_id"],
                &b["to_id"],
                amount,
                need(&b, "date")?,
                desc,
                &paid,
            )?;
            return Ok(json!({"id": id}));
        }
        "invoices" => {
            if m == "GET" {
                let card = qint(&q, "card")?.ok_or("campo obrigatório: 'card'")?;
                if let Some(due) = q.get("due") {
                    return Ok(json!({"items": db.invoice_items(card, due)?}));
                }
                return Ok(json!({"invoices": db.list_invoices(card)?}));
            }
            let b = body_obj(&body);
            let last = *parts.last().unwrap_or(&"");
            if m == "POST" && last == "pay" {
                let card = as_int(Some(need(&b, "card_id")?)).ok_or("cartão inválido")?;
                let due = need(&b, "due")?
                    .as_str()
                    .ok_or("vencimento inválido")?
                    .to_string();
                let amount = as_int(b.get("amount_cents"));
                let id = db.pay_invoice(
                    card,
                    &due,
                    need(&b, "account_id")?,
                    need(&b, "date")?,
                    amount,
                )?;
                return Ok(json!({"id": id}));
            }
            if m == "POST" && last == "unpay" {
                let card = as_int(Some(need(&b, "card_id")?)).ok_or("cartão inválido")?;
                let due = need(&b, "due")?
                    .as_str()
                    .ok_or("vencimento inválido")?
                    .to_string();
                db.unpay_invoice(card, &due)?;
                return ok();
            }
        }
        "recurrences" => match m.as_str() {
            "GET" => return Ok(json!({"items": db.list_recurrences()?})),
            "POST" | "PUT" => {
                let mut b = body_obj(&body);
                if let Some(id) = rid {
                    b.insert("id".into(), json!(id));
                }
                return Ok(json!({"id": db.save_recurrence(&b)?}));
            }
            "DELETE" => {
                db.stop_recurrence(
                    rid_or_err(rid)?,
                    q.get("future").map(|s| s.as_str()).unwrap_or("1") == "1",
                )?;
                return ok();
            }
            _ => {}
        },
        "accounts" => match m.as_str() {
            "POST" | "PUT" => {
                let mut b = body_obj(&body);
                if let Some(id) = rid {
                    b.insert("id".into(), json!(id));
                }
                let target = b.remove("balance_cents");
                let aid = db.save_account(&b)?;
                let is_conta = b.get("kind").and_then(|v| v.as_str()).unwrap_or("conta") == "conta";
                if let Some(t) = target.filter(|v| !v.is_null()) {
                    if is_conta {
                        db.set_balance(&aid, as_int(Some(&t)).ok_or("saldo inválido")?)?;
                    }
                }
                return Ok(json!({"id": aid}));
            }
            "DELETE" => return Ok(json!({"resultado": db.delete_account(rid_or_err(rid)?)?})),
            _ => {}
        },
        "categories" => match m.as_str() {
            "POST" | "PUT" => {
                let mut b = body_obj(&body);
                if let Some(id) = rid {
                    b.insert("id".into(), json!(id));
                }
                return Ok(json!({"id": db.save_category(&b)?}));
            }
            "DELETE" => return Ok(json!({"resultado": db.delete_category(rid_or_err(rid)?)?})),
            _ => {}
        },
        "settings" => {
            if m == "GET" {
                return Ok(Value::Object(db.get_settings()?));
            }
            db.save_settings(&body_obj(&body))?;
            return ok();
        }
        "reset" if m == "POST" => {
            let b = body_obj(&body);
            if b.get("confirm").and_then(|v| v.as_str()) != Some("APAGAR") {
                return Err("Para apagar tudo, confirme digitando APAGAR".into());
            }
            let path = db.reset_all()?;
            return Ok(json!({"ok": true, "backup": path.to_string_lossy()}));
        }
        "backup" => {
            if m == "GET" {
                return Ok(Value::Object(db.export_all()?));
            }
            if m == "POST" {
                db.import_all(&body_obj(&body))?;
                return ok();
            }
        }
        _ => {}
    }
    Err("rota inválida".into())
}

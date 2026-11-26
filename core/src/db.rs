//! Banco de dados e regras de negócio do Finanças Local — porta fiel do `db.py`.
//!
//! Valores sempre em centavos (inteiro com sinal: despesa negativa, receita positiva).
//! Datas em ISO (AAAA-MM-DD). As linhas saem como objetos JSON com os mesmos campos
//! que a versão Python devolvia, para o frontend não precisar mudar.

use crate::dates::*;
use chrono::{Datelike, Duration, Local, NaiveDate};
use rusqlite::types::{Value as SqlV, ValueRef};
use rusqlite::{params_from_iter, Connection};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub type Obj = Map<String, Value>;
pub type R<T> = Result<T, String>;

pub const APP_NAME: &str = "FinancasLocal";

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('conta','cartao')),
  initial_cents INTEGER NOT NULL DEFAULT 0,
  closing_day INTEGER, due_day INTEGER, limit_cents INTEGER,
  payment_account_id INTEGER,
  color TEXT, archived INTEGER NOT NULL DEFAULT 0, sort INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS categories (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('despesa','receita')),
  parent_id INTEGER, color TEXT, archived INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS recurrences (
  id INTEGER PRIMARY KEY,
  description TEXT NOT NULL,
  amount_cents INTEGER NOT NULL,
  account_id INTEGER NOT NULL,
  category_id INTEGER,
  day INTEGER NOT NULL,
  start_date TEXT NOT NULL,
  end_date TEXT,
  generated_until TEXT,
  active INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS transactions (
  id INTEGER PRIMARY KEY,
  description TEXT NOT NULL,
  amount_cents INTEGER NOT NULL,
  date TEXT NOT NULL,
  account_id INTEGER NOT NULL,
  category_id INTEGER,
  paid INTEGER NOT NULL DEFAULT 1,
  notes TEXT, tags TEXT,
  kind TEXT NOT NULL DEFAULT 'normal',
  series_id TEXT, inst_index INTEGER, inst_total INTEGER,
  recurrence_id INTEGER,
  invoice_due TEXT,
  transfer_id TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX IF NOT EXISTS ix_tx_date ON transactions(date);
CREATE INDEX IF NOT EXISTS ix_tx_acc ON transactions(account_id, invoice_due);
CREATE TABLE IF NOT EXISTS invoices (
  card_id INTEGER NOT NULL, due TEXT NOT NULL,
  paid INTEGER NOT NULL DEFAULT 0, paid_at TEXT, payment_tx_id INTEGER,
  PRIMARY KEY (card_id, due)
);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT);
"#;

pub const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("lembrete_diario", "21:00"),
    ("lembrete_diario_ativo", "1"),
    ("aviso_contas_hora", "09:00"),
    ("aviso_contas_dias", "2"),
    ("revisao_semanal_ativo", "1"),
    ("revisao_semanal_hora", "19:00"),
    ("aviso_estilo", "proprio"),
    ("tremer_tela", "1"),
    ("tremor_intensidade", "medio"),
    ("som_aviso", "1"),
    ("aviso_fechamento_ativo", "1"),
    ("aviso_fechamento_dias", "1"),
    ("ntfy_topico", ""),
    ("ntfy_servidor", "https://ntfy.sh"),
    ("porta", "8765"),
];

pub const TX_FIELDS: &[&str] = &[
    "description",
    "amount_cents",
    "date",
    "account_id",
    "category_id",
    "paid",
    "notes",
    "tags",
    "kind",
    "series_id",
    "inst_index",
    "inst_total",
    "recurrence_id",
    "invoice_due",
    "transfer_id",
];

pub const TABLES: &[&str] = &[
    "accounts",
    "categories",
    "recurrences",
    "transactions",
    "invoices",
    "settings",
];

// ------------------------------------------------------------------ pastas

/// Mesma pasta da versão Python: %APPDATA%\FinancasLocal no Windows,
/// $XDG_DATA_HOME/financas-local (ou ~/.local/share/financas-local) no Linux.
pub fn default_data_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let d = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join(APP_NAME)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("financas-local")
    };
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn default_db_path(data_dir: &Path) -> PathBuf {
    std::env::var_os("FINANCAS_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("financas.db"))
}

// ------------------------------------------------------------------ ajudantes de valores

macro_rules! p {
    () => { Vec::<SqlV>::new() };
    ($($x:expr),+ $(,)?) => { vec![$(SqlV::from($x)),+] };
}

/// Valor JSON -> valor SQLite (como o sqlite3 do Python faria).
pub fn sql(v: &Value) -> SqlV {
    match v {
        Value::Null => SqlV::Null,
        Value::Bool(b) => SqlV::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => SqlV::Integer(i),
            None => SqlV::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => SqlV::Text(s.clone()),
        other => SqlV::Text(other.to_string()),
    }
}

fn from_ref(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Value::Null,
    }
}

/// "Verdade" no sentido do Python (0, "", None e False são falsos).
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// int(x) do Python para números e textos numéricos.
pub fn as_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::Bool(b) => Some(*b as i64),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn as_str(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// str(v) do Python, usado em save_settings.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        other => other.to_string(),
    }
}

pub fn need<'a>(d: &'a Obj, k: &str) -> R<&'a Value> {
    d.get(k).ok_or_else(|| format!("campo obrigatório: '{k}'"))
}

fn need_int(d: &Obj, k: &str) -> R<i64> {
    as_int(Some(need(d, k)?)).ok_or_else(|| format!("valor inválido em '{k}'"))
}

fn need_str(d: &Obj, k: &str) -> R<String> {
    as_str(Some(need(d, k)?)).ok_or_else(|| format!("valor inválido em '{k}'"))
}

fn e(err: rusqlite::Error) -> String {
    err.to_string()
}

fn int_of(o: &Obj, k: &str) -> i64 {
    as_int(o.get(k)).unwrap_or(0)
}

fn str_of(o: &Obj, k: &str) -> String {
    as_str(o.get(k)).unwrap_or_default()
}

// ------------------------------------------------------------------ Db

pub struct Db {
    pub conn: Connection,
    pub data_dir: PathBuf,
    today_override: Option<NaiveDate>,
}

impl Db {
    /// Abre (ou cria) o banco no caminho padrão.
    pub fn open_default() -> R<Db> {
        let dir = default_data_dir();
        let path = default_db_path(&dir);
        Db::open(&path, &dir)
    }

    pub fn open(db_path: &Path, data_dir: &Path) -> R<Db> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path).map_err(e)?;
        Self::init(conn, data_dir)
    }

    pub fn open_in_memory(data_dir: &Path) -> R<Db> {
        Self::init(Connection::open_in_memory().map_err(e)?, data_dir)
    }

    fn init(conn: Connection, data_dir: &Path) -> R<Db> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(e)?;
        conn.execute_batch("PRAGMA foreign_keys=OFF;").map_err(e)?;
        conn.execute_batch(SCHEMA).map_err(e)?;
        // migração v2: bandeira e categoria do cartão (Visa Platinum, Mastercard Black…)
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(accounts)")
            .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(1))?.collect())
            .map_err(e)?;
        for c in ["network", "tier"] {
            if !cols.iter().any(|x| x == c) {
                conn.execute_batch(&format!("ALTER TABLE accounts ADD COLUMN {c} TEXT")).map_err(e)?;
            }
        }
        // migração v2: ícone escolhido para a categoria (vazio = ícone automático pelo nome)
        let ccols: Vec<String> = conn
            .prepare("PRAGMA table_info(categories)")
            .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(1))?.collect())
            .map_err(e)?;
        if !ccols.iter().any(|x| x == "icon") {
            conn.execute_batch("ALTER TABLE categories ADD COLUMN icon TEXT").map_err(e)?;
        }
        for (k, v) in DEFAULT_SETTINGS {
            conn.execute(
                "INSERT OR IGNORE INTO settings(key,value) VALUES(?,?)",
                [k, v],
            )
            .map_err(e)?;
        }
        let today_override = std::env::var("FINANCAS_HOJE")
            .ok()
            .and_then(|s| parse(&s).ok());
        Ok(Db {
            conn,
            data_dir: data_dir.to_path_buf(),
            today_override,
        })
    }

    /// Fixa a data de "hoje" (testes e conferência).
    pub fn set_today(&mut self, d: Option<NaiveDate>) {
        self.today_override = d;
    }

    pub fn today(&self) -> NaiveDate {
        self.today_override
            .unwrap_or_else(|| Local::now().date_naive())
    }

    // ---------------------------------------------------------- SQL

    pub fn rows(&self, sql_: &str, args: Vec<SqlV>) -> R<Vec<Obj>> {
        let mut st = self.conn.prepare_cached(sql_).map_err(e)?;
        let cols: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        let mut rs = st.query(params_from_iter(args)).map_err(e)?;
        let mut out = Vec::new();
        while let Some(row) = rs.next().map_err(e)? {
            let mut o = Obj::new();
            for (i, c) in cols.iter().enumerate() {
                o.insert(c.clone(), from_ref(row.get_ref(i).map_err(e)?));
            }
            out.push(o);
        }
        Ok(out)
    }

    pub fn one(&self, sql_: &str, args: Vec<SqlV>) -> R<Option<Obj>> {
        Ok(self.rows(sql_, args)?.into_iter().next())
    }

    fn scalar(&self, sql_: &str, args: Vec<SqlV>) -> R<i64> {
        self.conn
            .query_row(sql_, params_from_iter(args), |r| r.get::<_, Option<i64>>(0))
            .map(|v| v.unwrap_or(0))
            .map_err(e)
    }

    fn exec(&self, sql_: &str, args: Vec<SqlV>) -> R<i64> {
        self.conn.execute(sql_, params_from_iter(args)).map_err(e)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Bloco atômico (SAVEPOINT aninha sem problema).
    fn atomic<T>(&self, f: impl FnOnce(&Db) -> R<T>) -> R<T> {
        self.conn.execute_batch("SAVEPOINT fl").map_err(e)?;
        match f(self) {
            Ok(v) => {
                self.conn.execute_batch("RELEASE fl").map_err(e)?;
                Ok(v)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK TO fl; RELEASE fl");
                Err(err)
            }
        }
    }

    pub fn is_empty(&self) -> R<bool> {
        Ok(self.scalar("SELECT COUNT(*) FROM accounts", p![])? == 0)
    }

    // ---------------------------------------------------------- settings

    pub fn get_settings(&self) -> R<Obj> {
        let mut o = Obj::new();
        for r in self.rows("SELECT key,value FROM settings", p![])? {
            o.insert(
                str_of(&r, "key"),
                r.get("value").cloned().unwrap_or(Value::Null),
            );
        }
        Ok(o)
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten()
    }

    pub fn save_settings(&self, d: &Obj) -> R<()> {
        self.atomic(|db| {
            for (k, v) in d {
                db.exec(
                    "INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)",
                    p![k.clone(), py_str(v)],
                )?;
            }
            Ok(())
        })
    }

    // ---------------------------------------------------------- contas

    pub fn account(&self, aid: &Value) -> R<Option<Obj>> {
        self.one("SELECT * FROM accounts WHERE id=?", vec![sql(aid)])
    }

    /// Saldo de cada conta = saldo inicial + lançamentos pagos.
    pub fn balances(&self) -> R<HashMap<i64, i64>> {
        let mut out = HashMap::new();
        for a in self.rows(
            "SELECT id, initial_cents FROM accounts WHERE kind='conta'",
            p![],
        )? {
            let id = int_of(&a, "id");
            let s = self.scalar(
                "SELECT COALESCE(SUM(amount_cents),0) s FROM transactions WHERE account_id=? AND paid=1",
                p![id],
            )?;
            out.insert(id, int_of(&a, "initial_cents") + s);
        }
        Ok(out)
    }

    pub fn list_accounts(&self, include_archived: bool) -> R<Vec<Obj>> {
        let sql_ = format!(
            "SELECT * FROM accounts {}ORDER BY kind, sort, name",
            if include_archived {
                ""
            } else {
                "WHERE archived=0 "
            }
        );
        let mut accs = self.rows(&sql_, p![])?;
        let bal = self.balances()?;
        let t = self.today();
        let month_end = iso(last_of_month(t));
        for a in accs.iter_mut() {
            let id = int_of(a, "id");
            if str_of(a, "kind") == "conta" {
                let b = *bal.get(&id).unwrap_or(&0);
                a.insert("balance_cents".into(), json!(b));
                let pend = self.scalar(
                    "SELECT COALESCE(SUM(amount_cents),0) s FROM transactions WHERE account_id=? AND paid=0 AND date<=?",
                    p![id, month_end.clone()],
                )?;
                a.insert("forecast_cents".into(), json!(b + pend));
            } else {
                let inv = self.list_invoices_for(a)?;
                let unpaid: Vec<String> = inv
                    .iter()
                    .filter(|i| !truthy(i.get("paid")))
                    .map(|i| str_of(i, "due"))
                    .collect();
                // limite em uso: faturas não pagas, sem contar contas fixas que ainda não aconteceram
                let marks = if unpaid.is_empty() {
                    "''".to_string()
                } else {
                    vec!["?"; unpaid.len()].join(",")
                };
                let mut args: Vec<SqlV> = vec![SqlV::Integer(id)];
                args.extend(unpaid.into_iter().map(SqlV::Text));
                args.push(SqlV::Text(iso(t)));
                let open = self.scalar(
                    &format!(
                        "SELECT COALESCE(SUM(amount_cents),0) s FROM transactions WHERE account_id=?
                         AND invoice_due IN ({marks})
                         AND NOT (recurrence_id IS NOT NULL AND date>?)"
                    ),
                    args,
                )?;
                a.insert("open_cents".into(), json!(open));
                a.insert(
                    "available_cents".into(),
                    json!(as_int(a.get("limit_cents")).unwrap_or(0) + open),
                );
                let cur = iso(self.current_invoice(a, None));
                let cur_total = inv
                    .iter()
                    .find(|i| str_of(i, "due") == cur)
                    .map(|i| int_of(i, "total_cents"))
                    .unwrap_or(0);
                a.insert("current_due".into(), json!(cur));
                a.insert("current_cents".into(), json!(cur_total));
            }
        }
        Ok(accs)
    }

    pub fn save_account(&self, d: &Obj) -> R<Value> {
        const FIELDS: &[&str] = &[
            "name",
            "kind",
            "initial_cents",
            "closing_day",
            "due_day",
            "limit_cents",
            "payment_account_id",
            "color",
            "archived",
            "sort",
            "network",
            "tier",
        ];
        let mut vals = Obj::new();
        for k in FIELDS {
            if let Some(v) = d.get(*k) {
                vals.insert(k.to_string(), v.clone());
            }
        }
        if vals.get("kind").and_then(|v| v.as_str()) == Some("conta") {
            vals.insert("closing_day".into(), Value::Null);
            vals.insert("due_day".into(), Value::Null);
            vals.insert("limit_cents".into(), Value::Null);
        }
        if truthy(d.get("id")) {
            let id = d["id"].clone();
            if !vals.is_empty() {
                let sets = vals
                    .keys()
                    .map(|k| format!("{k}=?"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut args: Vec<SqlV> = vals.values().map(sql).collect();
                args.push(sql(&id));
                self.exec(&format!("UPDATE accounts SET {sets} WHERE id=?"), args)?;
            }
            return Ok(id);
        }
        if !vals.contains_key("initial_cents") {
            vals.insert("initial_cents".into(), json!(0));
        }
        let cols = vals.keys().cloned().collect::<Vec<_>>().join(", ");
        let marks = vec!["?"; vals.len()].join(",");
        let id = self.exec(
            &format!("INSERT INTO accounts({cols}) VALUES({marks})"),
            vals.values().map(sql).collect(),
        )?;
        Ok(json!(id))
    }

    /// Ajusta o saldo inicial para a conta mostrar o saldo informado hoje.
    pub fn set_balance(&self, aid: &Value, target_cents: i64) -> R<()> {
        let id = as_int(Some(aid)).unwrap_or(0);
        let cur = *self.balances()?.get(&id).unwrap_or(&0);
        let a = self.account(aid)?.ok_or("conta não encontrada")?;
        self.exec(
            "UPDATE accounts SET initial_cents=? WHERE id=?",
            p![int_of(&a, "initial_cents") + target_cents - cur, id],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, aid: i64) -> R<&'static str> {
        let n = self.scalar(
            "SELECT COUNT(*) n FROM transactions WHERE account_id=?",
            p![aid],
        )?;
        if n > 0 {
            self.exec("UPDATE accounts SET archived=1 WHERE id=?", p![aid])?;
            return Ok("arquivada");
        }
        self.exec("DELETE FROM accounts WHERE id=?", p![aid])?;
        Ok("excluida")
    }

    // ---------------------------------------------------------- categorias

    pub fn list_categories(&self) -> R<Vec<Obj>> {
        self.rows(
            "SELECT * FROM categories WHERE archived=0 ORDER BY kind, COALESCE(parent_id,id), parent_id IS NOT NULL, name",
            p![],
        )
    }

    pub fn save_category(&self, d: &Obj) -> R<Value> {
        let name = need(d, "name")?.clone();
        let kind = need(d, "kind")?.clone();
        let parent = d.get("parent_id").cloned().unwrap_or(Value::Null);
        let color = d.get("color").cloned().unwrap_or(Value::Null);
        if truthy(d.get("id")) {
            self.exec(
                "UPDATE categories SET name=?, kind=?, parent_id=?, color=? WHERE id=?",
                vec![sql(&name), sql(&kind), sql(&parent), sql(&color), sql(&d["id"])],
            )?;
            if let Some(icon) = d.get("icon") {
                self.exec("UPDATE categories SET icon=? WHERE id=?", vec![sql(icon), sql(&d["id"])])?;
            }
            return Ok(d["id"].clone());
        }
        let id = self.exec(
            "INSERT INTO categories(name,kind,parent_id,color,icon) VALUES(?,?,?,?,?)",
            vec![sql(&name), sql(&kind), sql(&parent), sql(&color), sql(d.get("icon").unwrap_or(&Value::Null))],
        )?;
        Ok(json!(id))
    }

    pub fn delete_category(&self, cid: i64) -> R<&'static str> {
        let n = self.scalar(
            "SELECT COUNT(*) n FROM transactions WHERE category_id=?",
            p![cid],
        )?;
        if n > 0 {
            self.exec("UPDATE categories SET archived=1 WHERE id=?", p![cid])?;
            return Ok("arquivada");
        }
        self.atomic(|db| {
            db.exec(
                "UPDATE categories SET parent_id=NULL WHERE parent_id=?",
                p![cid],
            )?;
            db.exec("DELETE FROM categories WHERE id=?", p![cid])
        })?;
        Ok("excluida")
    }

    // ---------------------------------------------------------- faturas

    pub fn current_invoice(&self, card: &Obj, reference: Option<NaiveDate>) -> NaiveDate {
        invoice_due_for(
            reference.unwrap_or_else(|| self.today()),
            int_of(card, "closing_day"),
            int_of(card, "due_day"),
        )
    }

    pub fn list_invoices(&self, card_id: i64) -> R<Vec<Obj>> {
        let card = self
            .account(&json!(card_id))?
            .ok_or("cartão não encontrado")?;
        self.list_invoices_for(&card)
    }

    fn list_invoices_for(&self, card: &Obj) -> R<Vec<Obj>> {
        let card_id = int_of(card, "id");
        let (cl, du) = (int_of(card, "closing_day"), int_of(card, "due_day"));
        let mut totals: HashMap<String, i64> = HashMap::new();
        for r in self.rows(
            "SELECT invoice_due, SUM(amount_cents) s FROM transactions WHERE account_id=? GROUP BY invoice_due",
            p![card_id],
        )? {
            if let Some(due) = as_str(r.get("invoice_due")) {
                totals.insert(due, int_of(&r, "s"));
            }
        }
        let mut status: HashMap<String, Obj> = HashMap::new();
        for r in self.rows("SELECT * FROM invoices WHERE card_id=?", p![card_id])? {
            status.insert(str_of(&r, "due"), r);
        }
        let current = self.current_invoice(card, None);
        let mut dues: BTreeSet<String> = totals.keys().cloned().collect();
        dues.extend(status.keys().cloned());
        dues.insert(iso(current));
        let t = self.today();
        let mut out = Vec::new();
        for due in dues.into_iter().filter(|d| !d.is_empty()) {
            let d = parse(&due)?;
            let empty = Obj::new();
            let st = status.get(&due).unwrap_or(&empty);
            let close = closing_date_for(d, cl, du);
            let total = *totals.get(&due).unwrap_or(&0);
            let paid = truthy(st.get("paid")) || total >= 0;
            let state = if paid {
                "paga"
            } else if d < t {
                "atrasada"
            } else if close < t {
                "fechada"
            } else if close >= t && d == current {
                "aberta"
            } else {
                "futura"
            };
            let mut o = Obj::new();
            o.insert("card_id".into(), json!(card_id));
            o.insert("due".into(), json!(due));
            o.insert("closing".into(), json!(iso(close)));
            o.insert("total_cents".into(), json!(total));
            o.insert("paid".into(), json!(paid));
            o.insert("state".into(), json!(state));
            o.insert(
                "paid_at".into(),
                st.get("paid_at").cloned().unwrap_or(Value::Null),
            );
            o.insert(
                "payment_tx_id".into(),
                st.get("payment_tx_id").cloned().unwrap_or(Value::Null),
            );
            add_grace(&mut o, d);
            out.push(o);
        }
        Ok(out)
    }

    pub fn invoice_items(&self, card_id: i64, due: &str) -> R<Vec<Obj>> {
        self.rows(
            "SELECT t.*, c.name category FROM transactions t LEFT JOIN categories c ON c.id=t.category_id
             WHERE t.account_id=? AND t.invoice_due=? ORDER BY t.date, t.id",
            p![card_id, due.to_string()],
        )
    }

    pub fn pay_invoice(
        &self,
        card_id: i64,
        due: &str,
        account_id: &Value,
        date: &Value,
        amount_cents: Option<i64>,
    ) -> R<i64> {
        let card = self
            .account(&json!(card_id))?
            .ok_or("cartão não encontrado")?;
        let total: i64 = self
            .invoice_items(card_id, due)?
            .iter()
            .map(|i| int_of(i, "amount_cents"))
            .sum();
        let amount = amount_cents.unwrap_or(total);
        let desc = format!(
            "Pagamento fatura {} ({})",
            str_of(&card, "name"),
            parse(due)?.format("%d/%m/%Y")
        );
        self.atomic(|db| {
            let txid = db.exec(
                "INSERT INTO transactions(description,amount_cents,date,account_id,paid,kind,invoice_due)
                 VALUES(?,?,?,?,1,'pagamento_fatura',?)",
                vec![SqlV::Text(desc), SqlV::Integer(-amount.abs()), sql(date), sql(account_id), SqlV::Text(due.into())],
            )?;
            db.exec(
                "INSERT OR REPLACE INTO invoices(card_id,due,paid,paid_at,payment_tx_id) VALUES(?,?,1,?,?)",
                vec![SqlV::Integer(card_id), SqlV::Text(due.into()), sql(date), SqlV::Integer(txid)],
            )?;
            Ok(txid)
        })
    }

    pub fn unpay_invoice(&self, card_id: i64, due: &str) -> R<()> {
        let st = self.one(
            "SELECT * FROM invoices WHERE card_id=? AND due=?",
            p![card_id, due.to_string()],
        )?;
        self.atomic(|db| {
            if let Some(st) = &st {
                if truthy(st.get("payment_tx_id")) {
                    db.exec("DELETE FROM transactions WHERE id=?", vec![sql(&st["payment_tx_id"])])?;
                }
            }
            db.exec(
                "INSERT OR REPLACE INTO invoices(card_id,due,paid,paid_at,payment_tx_id) VALUES(?,?,0,NULL,NULL)",
                p![card_id, due.to_string()],
            )?;
            Ok(())
        })
    }

    // ---------------------------------------------------------- lançamentos

    fn fill_invoice(&self, d: &mut Obj) -> R<()> {
        let acc = match d.get("account_id") {
            Some(v) => self.account(v)?,
            None => None,
        };
        if let Some(acc) = acc.filter(|a| str_of(a, "kind") == "cartao") {
            if !truthy(d.get("invoice_due")) {
                let date = parse(&need_str(d, "date")?)?;
                let due =
                    invoice_due_for(date, int_of(&acc, "closing_day"), int_of(&acc, "due_day"));
                d.insert("invoice_due".into(), json!(iso(due)));
            }
            d.insert("paid".into(), json!(1));
        } else {
            let kind = match d.get("kind") {
                None => Some("normal".to_string()),
                Some(v) => as_str(Some(v)),
            };
            if kind.as_deref() == Some("normal") {
                d.insert("invoice_due".into(), Value::Null);
            }
        }
        Ok(())
    }

    fn insert_tx(&self, d: &Obj) -> R<i64> {
        let mut d = d.clone();
        self.fill_invoice(&mut d)?;
        let cols: Vec<&str> = TX_FIELDS
            .iter()
            .copied()
            .filter(|k| d.contains_key(*k))
            .collect();
        let marks = vec!["?"; cols.len()].join(",");
        let args = cols.iter().map(|k| sql(&d[*k])).collect();
        self.exec(
            &format!(
                "INSERT INTO transactions({}) VALUES({marks})",
                cols.join(",")
            ),
            args,
        )
    }

    /// Cria um lançamento. Com parcelas > 1, divide o valor total em N parcelas mensais.
    /// Com fixa=true, cria uma conta fixa mensal e gera as ocorrências.
    pub fn create_transaction(&self, body: &Obj) -> R<i64> {
        let mut d = body.clone();
        let n = d
            .remove("parcelas")
            .map(|v| {
                if truthy(Some(&v)) {
                    as_int(Some(&v)).unwrap_or(1)
                } else {
                    1
                }
            })
            .unwrap_or(1);
        let fixa = truthy(d.remove("fixa").as_ref());
        if !d.contains_key("paid") {
            d.insert("paid".into(), json!(1));
        }
        if fixa {
            let date = need_str(&d, "date")?;
            let day = parse(&date)?.day() as i64;
            let first = self.atomic(|db| {
                let rid = db.exec(
                    "INSERT INTO recurrences(description,amount_cents,account_id,category_id,day,start_date)
                     VALUES(?,?,?,?,?,?)",
                    vec![
                        sql(need(&d, "description")?),
                        sql(need(&d, "amount_cents")?),
                        sql(need(&d, "account_id")?),
                        sql(d.get("category_id").unwrap_or(&Value::Null)),
                        SqlV::Integer(day),
                        SqlV::Text(date.clone()),
                    ],
                )?;
                d.insert("recurrence_id".into(), json!(rid));
                let first = db.insert_tx(&d)?;
                db.exec("UPDATE recurrences SET generated_until=? WHERE id=?", p![date.clone(), rid])?;
                Ok(first)
            })?;
            self.ensure_recurrences(62)?;
            return Ok(first);
        }
        if n <= 1 {
            return self.atomic(|db| db.insert_tx(&d));
        }
        let total = need_int(&d, "amount_cents")?;
        let base = total / n; // int(total / n): trunca em direção a zero, como no Python
        let rest = total - base * n;
        let sid = uuid::Uuid::new_v4().simple().to_string();
        let start = parse(&need_str(&d, "date")?)?;
        let acc = self
            .account(need(&d, "account_id")?)?
            .ok_or("conta não encontrada")?;
        let first_due = if str_of(&acc, "kind") == "cartao" {
            Some(match d.get("invoice_due").filter(|v| truthy(Some(v))) {
                Some(v) => parse(&as_str(Some(v)).unwrap_or_default())?,
                None => {
                    invoice_due_for(start, int_of(&acc, "closing_day"), int_of(&acc, "due_day"))
                }
            })
        } else {
            None
        };
        self.atomic(|db| {
            let mut first = None;
            for i in 0..n {
                let mut p = d.clone();
                p.insert(
                    "amount_cents".into(),
                    json!(base + if i == 0 { rest } else { 0 }),
                );
                p.insert(
                    "date".into(),
                    json!(iso(add_months(start, i as i32, Some(start.day())))),
                );
                p.insert("series_id".into(), json!(sid));
                p.insert("inst_index".into(), json!(i + 1));
                p.insert("inst_total".into(), json!(n));
                if let Some(fd) = first_due {
                    p.insert(
                        "invoice_due".into(),
                        json!(iso(add_months(fd, i as i32, Some(fd.day())))),
                    );
                    p.insert("paid".into(), json!(1));
                } else if i > 0 {
                    p.insert("paid".into(), json!(0));
                }
                let tid = db.insert_tx(&p)?;
                first.get_or_insert(tid);
            }
            Ok(first.unwrap_or(0))
        })
    }

    pub fn update_transaction(&self, tid: i64, body: &Obj, scope: &str) -> R<()> {
        let Some(old) = self.one("SELECT * FROM transactions WHERE id=?", p![tid])? else {
            return Ok(());
        };
        let mut d: Obj = body
            .iter()
            .filter(|(k, _)| TX_FIELDS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if truthy(old.get("transfer_id")) {
            // transferência: atualiza os dois lados
            let tr = old["transfer_id"].clone();
            return self.atomic(|db| {
                for k in ["date", "description", "paid"] {
                    if let Some(v) = d.get(k) {
                        db.exec(&format!("UPDATE transactions SET {k}=? WHERE transfer_id=?"), vec![sql(v), sql(&tr)])?;
                    }
                }
                if d.contains_key("amount_cents") {
                    let v = need_int(&d, "amount_cents")?.abs();
                    db.exec(
                        "UPDATE transactions SET amount_cents=CASE WHEN amount_cents<0 THEN ? ELSE ? END WHERE transfer_id=?",
                        vec![SqlV::Integer(-v), SqlV::Integer(v), sql(&tr)],
                    )?;
                }
                Ok(())
            });
        }

        if d.contains_key("date") || d.contains_key("account_id") {
            let nd = d.get("date").unwrap_or(&old["date"]);
            let na = d.get("account_id").unwrap_or(&old["account_id"]);
            if (nd != &old["date"] || na != &old["account_id"]) && !d.contains_key("invoice_due") {
                d.insert("invoice_due".into(), Value::Null);
            }
            let mut merged = old.clone();
            for (k, v) in &d {
                merged.insert(k.clone(), v.clone());
            }
            self.fill_invoice(&mut merged)?;
            d.insert(
                "invoice_due".into(),
                merged.get("invoice_due").cloned().unwrap_or(Value::Null),
            );
        }

        self.atomic(|db| {
            let mut targets = vec![tid];
            let series = truthy(old.get("series_id"));
            let rec = truthy(old.get("recurrence_id"));
            if scope == "future" && (series || rec) {
                let col = if series { "series_id" } else { "recurrence_id" };
                targets = db
                    .rows(
                        &format!("SELECT id FROM transactions WHERE {col}=? AND date>=?"),
                        vec![sql(&old[col]), sql(&old["date"])],
                    )?
                    .iter()
                    .map(|r| int_of(r, "id"))
                    .collect();
                if rec {
                    let rd: Vec<(&str, &Value)> =
                        ["description", "amount_cents", "account_id", "category_id"]
                            .iter()
                            .filter_map(|k| d.get(*k).map(|v| (*k, v)))
                            .collect();
                    if !rd.is_empty() {
                        let sets = rd
                            .iter()
                            .map(|(k, _)| format!("{k}=?"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let mut args: Vec<SqlV> = rd.iter().map(|(_, v)| sql(v)).collect();
                        args.push(sql(&old["recurrence_id"]));
                        db.exec(&format!("UPDATE recurrences SET {sets} WHERE id=?"), args)?;
                    }
                }
            }
            for t in targets {
                let mut dd = d.clone();
                if t != tid {
                    dd.remove("date");
                    dd.remove("paid");
                    dd.remove("invoice_due");
                }
                if dd.is_empty() {
                    continue;
                }
                let sets = dd
                    .keys()
                    .map(|k| format!("{k}=?"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut args: Vec<SqlV> = dd.values().map(sql).collect();
                args.push(SqlV::Integer(t));
                db.exec(&format!("UPDATE transactions SET {sets} WHERE id=?"), args)?;
            }
            Ok(())
        })
    }

    pub fn delete_transaction(&self, tid: i64, scope: &str) -> R<()> {
        let Some(old) = self.one("SELECT * FROM transactions WHERE id=?", p![tid])? else {
            return Ok(());
        };
        self.atomic(|db| {
            if truthy(old.get("transfer_id")) {
                db.exec("DELETE FROM transactions WHERE transfer_id=?", vec![sql(&old["transfer_id"])])?;
            } else if str_of(&old, "kind") == "pagamento_fatura" {
                db.exec("DELETE FROM transactions WHERE id=?", p![tid])?;
                db.exec("UPDATE invoices SET paid=0, paid_at=NULL, payment_tx_id=NULL WHERE payment_tx_id=?", p![tid])?;
            } else if scope == "future" && truthy(old.get("series_id")) {
                db.exec("DELETE FROM transactions WHERE series_id=? AND date>=?", vec![sql(&old["series_id"]), sql(&old["date"])])?;
            } else if scope == "future" && truthy(old.get("recurrence_id")) {
                db.exec(
                    "DELETE FROM transactions WHERE recurrence_id=? AND date>=?",
                    vec![sql(&old["recurrence_id"]), sql(&old["date"])],
                )?;
                let end = parse(&str_of(&old, "date"))? - Duration::days(1);
                db.exec("UPDATE recurrences SET active=0, end_date=? WHERE id=?", vec![SqlV::Text(iso(end)), sql(&old["recurrence_id"])])?;
            } else {
                db.exec("DELETE FROM transactions WHERE id=?", p![tid])?;
            }
            Ok(())
        })
    }

    pub fn create_transfer(
        &self,
        from_id: &Value,
        to_id: &Value,
        amount_cents: i64,
        date: &Value,
        description: &str,
        paid: &Value,
    ) -> R<String> {
        let tid = uuid::Uuid::new_v4().simple().to_string();
        let amt = amount_cents.abs();
        self.atomic(|db| {
            for (acc, val) in [(from_id, -amt), (to_id, amt)] {
                db.exec(
                    "INSERT INTO transactions(description,amount_cents,date,account_id,paid,kind,transfer_id)
                     VALUES(?,?,?,?,?,'transferencia',?)",
                    vec![SqlV::Text(description.into()), SqlV::Integer(val), sql(date), sql(acc), sql(paid), SqlV::Text(tid.clone())],
                )?;
            }
            Ok(())
        })?;
        Ok(tid)
    }

    pub fn list_transactions(
        &self,
        start: &str,
        end: &str,
        account_id: Option<i64>,
        category_id: Option<i64>,
        q: Option<&str>,
    ) -> R<Obj> {
        let mut sql_ = String::from(
            "SELECT t.*, c.name category, c.color category_color, c.parent_id category_parent,
                    a.name account, a.kind account_kind,
                    (SELECT a2.name FROM transactions t2 JOIN accounts a2 ON a2.id=t2.account_id
                      WHERE t2.transfer_id=t.transfer_id AND t2.id<>t.id) other_account
             FROM transactions t LEFT JOIN categories c ON c.id=t.category_id
             JOIN accounts a ON a.id=t.account_id
             WHERE t.date BETWEEN ? AND ?",
        );
        let mut args = p![start.to_string(), end.to_string()];
        if let Some(a) = account_id.filter(|v| *v != 0) {
            sql_.push_str(" AND t.account_id=?");
            args.push(SqlV::Integer(a));
        }
        if let Some(c) = category_id.filter(|v| *v != 0) {
            sql_.push_str(" AND (t.category_id=? OR c.parent_id=?)");
            args.push(SqlV::Integer(c));
            args.push(SqlV::Integer(c));
        }
        if let Some(q) = q.filter(|s| !s.is_empty()) {
            sql_.push_str(" AND (t.description LIKE ? OR t.notes LIKE ? OR t.tags LIKE ?)");
            for _ in 0..3 {
                args.push(SqlV::Text(format!("%{q}%")));
            }
        }
        sql_.push_str(" ORDER BY t.date DESC, t.id DESC");
        let items = self.rows(&sql_, args)?;
        let normal = |t: &&Obj| str_of(t, "kind") == "normal";
        let inc: i64 = items
            .iter()
            .filter(normal)
            .map(|t| int_of(t, "amount_cents"))
            .filter(|v| *v > 0)
            .sum();
        let exp: i64 = items
            .iter()
            .filter(normal)
            .map(|t| int_of(t, "amount_cents"))
            .filter(|v| *v < 0)
            .sum();
        let mut o = Obj::new();
        o.insert(
            "items".into(),
            Value::Array(items.into_iter().map(Value::Object).collect()),
        );
        o.insert("income_cents".into(), json!(inc));
        o.insert("expense_cents".into(), json!(exp));
        o.insert("result_cents".into(), json!(inc + exp));
        Ok(o)
    }

    // ---------------------------------------------------------- contas fixas

    /// Gera as ocorrências das contas fixas até o horizonte (padrão: ~2 meses à frente).
    pub fn ensure_recurrences(&self, horizon_days: i64) -> R<()> {
        let horizon = self.today() + Duration::days(horizon_days);
        self.atomic(|db| {
            for r in db.rows("SELECT * FROM recurrences WHERE active=1", p![])? {
                let day = int_of(&r, "day").max(1) as u32;
                let last = match as_str(r.get("generated_until")).filter(|s| !s.is_empty()) {
                    Some(s) => Some(parse(&s)?),
                    None => None,
                };
                let mut nxt = match last {
                    Some(l) => add_months(l, 1, Some(day)),
                    None => parse(&str_of(&r, "start_date"))?,
                };
                let end = match as_str(r.get("end_date")).filter(|s| !s.is_empty()) {
                    Some(s) => Some(parse(&s)?),
                    None => None,
                };
                let mut made = None;
                while nxt <= horizon && end.map_or(true, |e| nxt <= e) {
                    let mut t = Obj::new();
                    t.insert("description".into(), r["description"].clone());
                    t.insert("amount_cents".into(), r["amount_cents"].clone());
                    t.insert("date".into(), json!(iso(nxt)));
                    t.insert("account_id".into(), r["account_id"].clone());
                    t.insert("category_id".into(), r["category_id"].clone());
                    t.insert("paid".into(), json!(0));
                    t.insert("recurrence_id".into(), r["id"].clone());
                    db.insert_tx(&t)?;
                    made = Some(nxt);
                    nxt = add_months(nxt, 1, Some(day));
                }
                if let Some(m) = made {
                    db.exec(
                        "UPDATE recurrences SET generated_until=? WHERE id=?",
                        vec![SqlV::Text(iso(m)), sql(&r["id"])],
                    )?;
                }
            }
            Ok(())
        })
    }

    pub fn list_recurrences(&self) -> R<Vec<Obj>> {
        let mut out = self.rows(
            "SELECT r.*, a.name account, c.name category FROM recurrences r
             JOIN accounts a ON a.id=r.account_id LEFT JOIN categories c ON c.id=r.category_id
             WHERE r.active=1 ORDER BY r.day",
            p![],
        )?;
        let t = iso(self.today());
        for r in out.iter_mut() {
            let n = self.one(
                "SELECT id, date, paid FROM transactions WHERE recurrence_id=? AND date>=? ORDER BY date LIMIT 1",
                vec![sql(&r["id"]), SqlV::Text(t.clone())],
            )?;
            r.insert("next".into(), n.map(Value::Object).unwrap_or(Value::Null));
        }
        Ok(out)
    }

    pub fn save_recurrence(&self, d: &Obj) -> R<Value> {
        let cat = d.get("category_id").cloned().unwrap_or(Value::Null);
        if truthy(d.get("id")) {
            let id = d["id"].clone();
            self.atomic(|db| {
                db.exec(
                    "UPDATE recurrences SET description=?, amount_cents=?, account_id=?, category_id=?, day=? WHERE id=?",
                    vec![
                        sql(need(d, "description")?), sql(need(d, "amount_cents")?), sql(need(d, "account_id")?),
                        sql(&cat), sql(need(d, "day")?), sql(&id),
                    ],
                )?;
                db.exec(
                    "UPDATE transactions SET description=?, amount_cents=?, account_id=?, category_id=?
                     WHERE recurrence_id=? AND paid=0 AND date>=?",
                    vec![
                        sql(&d["description"]), sql(&d["amount_cents"]), sql(&d["account_id"]), sql(&cat), sql(&id),
                        SqlV::Text(iso(self.today())),
                    ],
                )?;
                Ok(())
            })?;
            return Ok(id);
        }
        let day = need_int(d, "day")?;
        let mut start = match d.get("start_date").filter(|v| truthy(Some(v))) {
            Some(v) => parse(&as_str(Some(v)).unwrap_or_default())?,
            None => self.today(),
        };
        if start.day() as i64 > day {
            start = add_months(start, 1, Some(day as u32));
        } else {
            let last = days_in_month(start.year(), start.month());
            start = start.with_day((day.max(1) as u32).min(last)).unwrap();
        }
        let rid = self.exec(
            "INSERT INTO recurrences(description,amount_cents,account_id,category_id,day,start_date) VALUES(?,?,?,?,?,?)",
            vec![
                sql(need(d, "description")?), sql(need(d, "amount_cents")?), sql(need(d, "account_id")?), sql(&cat),
                SqlV::Integer(day), SqlV::Text(iso(start)),
            ],
        )?;
        self.ensure_recurrences(62)?;
        Ok(json!(rid))
    }

    pub fn stop_recurrence(&self, rid: i64, delete_future: bool) -> R<()> {
        let t = iso(self.today());
        self.atomic(|db| {
            if delete_future {
                db.exec(
                    "DELETE FROM transactions WHERE recurrence_id=? AND paid=0 AND date>=?",
                    p![rid, t.clone()],
                )?;
            }
            db.exec(
                "UPDATE recurrences SET active=0, end_date=? WHERE id=?",
                p![t.clone(), rid],
            )?;
            Ok(())
        })
    }

    // ---------------------------------------------------------- visão geral

    /// Contas a pagar/receber até `days` dias à frente, incluindo atrasadas.
    pub fn upcoming(&self, days: i64) -> R<Vec<Obj>> {
        let t = self.today();
        let ts = iso(t);
        let lim = iso(t + Duration::days(days));
        let mut items = Vec::new();
        for mut r in self.rows(
            "SELECT t.id, t.description, t.amount_cents, t.date, a.name account, c.name category, t.category_id
             FROM transactions t JOIN accounts a ON a.id=t.account_id
             LEFT JOIN categories c ON c.id=t.category_id
             WHERE a.kind='conta' AND t.paid=0 AND t.date<=? ORDER BY t.date",
            p![lim.clone()],
        )? {
            let overdue = str_of(&r, "date") < ts;
            r.insert("type".into(), json!("lancamento"));
            r.insert("overdue".into(), json!(overdue));
            let d = parse(&str_of(&r, "date"))?;
            add_grace(&mut r, d);
            items.push(r);
        }
        for card in self.rows(
            "SELECT * FROM accounts WHERE kind='cartao' AND archived=0",
            p![],
        )? {
            for inv in self.list_invoices_for(&card)? {
                let due = str_of(&inv, "due");
                let total = int_of(&inv, "total_cents");
                if !truthy(inv.get("paid")) && due <= lim && total < 0 {
                    let mut o = Obj::new();
                    o.insert("type".into(), json!("fatura"));
                    o.insert("card_id".into(), card["id"].clone());
                    o.insert(
                        "description".into(),
                        json!(format!("Fatura {}", str_of(&card, "name"))),
                    );
                    o.insert("amount_cents".into(), json!(total));
                    o.insert("date".into(), json!(due));
                    o.insert("account".into(), card["name"].clone());
                    o.insert("state".into(), inv["state"].clone());
                    o.insert("overdue".into(), json!(due < ts));
                    o.insert(
                        "color".into(),
                        card.get("color").cloned().unwrap_or(Value::Null),
                    );
                    add_grace(&mut o, parse(&due)?);
                    items.push(o);
                }
            }
        }
        items.sort_by(|a, b| str_of(a, "date").cmp(&str_of(b, "date"))); // estável, como o sort do Python
        Ok(items)
    }

    pub fn month_by_category(&self, start: &str, end: &str) -> R<Vec<Obj>> {
        self.rows(
            "SELECT COALESCE(p.id, c.id) id, COALESCE(p.name, c.name) name, COALESCE(p.color, c.color) color,
                    SUM(t.amount_cents) total
             FROM transactions t JOIN categories c ON c.id=t.category_id
             LEFT JOIN categories p ON p.id=c.parent_id
             WHERE t.kind='normal' AND t.amount_cents<0 AND t.date BETWEEN ? AND ?
             GROUP BY 1 ORDER BY total",
            p![start.to_string(), end.to_string()],
        )
    }

    pub fn launched_today(&self) -> R<i64> {
        self.scalar(
            "SELECT COUNT(*) n FROM transactions WHERE date(created_at)=? AND recurrence_id IS NULL",
            p![iso(self.today())],
        )
    }

    pub fn summary(&self) -> R<Obj> {
        self.ensure_recurrences(62)?;
        let t = self.today();
        let start = iso(first_of_month(t));
        let end = iso(last_of_month(t));
        let accs = self.list_accounts(false)?;
        let (contas, cards): (Vec<Obj>, Vec<Obj>) =
            accs.into_iter().partition(|a| str_of(a, "kind") == "conta");
        let up = self.upcoming(15)?;
        let month = self.list_transactions(&start, &end, None, None, None)?;
        let mut to_pay = Vec::new();
        for c in &cards {
            for inv in self.list_invoices_for(c)? {
                let st = str_of(&inv, "state");
                if !truthy(inv.get("paid"))
                    && int_of(&inv, "total_cents") < 0
                    && ["fechada", "atrasada", "aberta"].contains(&st.as_str())
                {
                    let mut o = inv.clone();
                    o.insert("card".into(), c["name"].clone());
                    to_pay.push(o);
                }
            }
        }
        let balance: i64 = contas.iter().map(|a| int_of(a, "balance_cents")).sum();
        let forecast: i64 = contas.iter().map(|a| int_of(a, "forecast_cents")).sum();
        let inv_total: i64 = to_pay.iter().map(|i| int_of(i, "total_cents")).sum();
        let mut m = Obj::new();
        m.insert("start".into(), json!(start));
        m.insert("end".into(), json!(end));
        m.insert("income_cents".into(), month["income_cents"].clone());
        m.insert("expense_cents".into(), month["expense_cents"].clone());
        m.insert("result_cents".into(), month["result_cents"].clone());
        m.insert(
            "by_category".into(),
            objs(self.month_by_category(&start, &end)?),
        );
        let mut o = Obj::new();
        o.insert("today".into(), json!(iso(t)));
        o.insert("balance_cents".into(), json!(balance));
        o.insert("forecast_cents".into(), json!(forecast));
        o.insert("invoices_cents".into(), json!(inv_total));
        o.insert("invoices".into(), objs(to_pay));
        o.insert("accounts".into(), objs(contas));
        o.insert("cards".into(), objs(cards.clone()));
        o.insert("upcoming".into(), objs(up));
        o.insert("month".into(), Value::Object(m));
        o.insert("launched_today".into(), json!(self.launched_today()?));
        // saúde financeira (v2): números para os indicadores da visão geral
        let income = month["income_cents"].as_i64().unwrap_or(0);
        let result = month["result_cents"].as_i64().unwrap_or(0);
        let fixed_exp = -self.scalar("SELECT COALESCE(SUM(amount_cents),0) FROM recurrences WHERE active=1 AND amount_cents<0", p![])?;
        let fixed_inc = self.scalar("SELECT COALESCE(SUM(amount_cents),0) FROM recurrences WHERE active=1 AND amount_cents>0", p![])?;
        let limit: i64 = cards.iter().map(|c| int_of(c, "limit_cents")).sum();
        let used: i64 = -cards.iter().map(|c| int_of(c, "open_cents")).sum::<i64>();
        let ratio = |a: i64, b: i64| if b > 0 { json!(a as f64 / b as f64) } else { Value::Null };
        o.insert(
            "health".into(),
            json!({
                "savings_cents": result, "income_cents": income, "savings_ratio": ratio(result, income),
                "fixed_expense_cents": fixed_exp, "fixed_income_cents": fixed_inc, "fixed_ratio": ratio(fixed_exp, fixed_inc),
                "cards_used_cents": used, "cards_limit_cents": limit, "cards_ratio": ratio(used, limit),
                "after_invoices_cents": forecast + inv_total,
            }),
        );
        self.add_snowball(&mut o, t, fixed_inc)?;
        Ok(o)
    }

    /// Indicadores de "bola de neve" do cartão: faturas do próximo mês contra a renda fixa,
    /// evolução das faturas nos últimos meses e quanto dos gastos foi no cartão.
    fn add_snowball(&self, o: &mut Obj, t: NaiveDate, fixed_inc: i64) -> R<()> {
        // mês das próximas faturas: o do vencimento mais próximo ainda não pago (a partir de hoje)
        let ts = iso(t);
        let mut next_due: Option<String> = None;
        for card in self.rows("SELECT * FROM accounts WHERE kind='cartao' AND archived=0", p![])? {
            for inv in self.list_invoices_for(&card)? {
                let due = str_of(&inv, "due");
                if !truthy(inv.get("paid")) && int_of(&inv, "total_cents") < 0 && due >= ts && next_due.as_ref().map_or(true, |n| due < *n) {
                    next_due = Some(due);
                }
            }
        }
        let month = match next_due {
            Some(d) => d[..7].to_string(),
            None => iso(add_months(first_of_month(t), 1, None))[..7].to_string(),
        };
        let m0 = parse(&format!("{month}-01"))?;
        let mut series = Vec::new();
        for k in (0..6).rev() {
            let m = add_months(m0, -k, None);
            let key = iso(m)[..7].to_string();
            let total = -self.scalar(
                "SELECT COALESCE(SUM(t.amount_cents),0) FROM transactions t JOIN accounts a ON a.id=t.account_id
                 WHERE a.kind='cartao' AND substr(t.invoice_due,1,7)=?",
                p![key.clone()],
            )?;
            series.push((key, total));
        }
        let next = series.last().map(|x| x.1).unwrap_or(0);
        let mut streak = 0;
        for w in series.windows(2).rev() {
            if w[1].1 > w[0].1 && w[0].1 > 0 {
                streak += 1;
            } else {
                break;
            }
        }
        // cartão x débito nos gastos do mês (lançamentos normais), mês atual e os dois anteriores
        let mut share = Vec::new();
        for k in (0..3).rev() {
            let m = add_months(first_of_month(t), -k, None);
            let (a, b) = (iso(m), iso(last_of_month(m)));
            let card = -self.scalar(
                "SELECT COALESCE(SUM(t.amount_cents),0) FROM transactions t JOIN accounts a ON a.id=t.account_id
                 WHERE t.kind='normal' AND t.amount_cents<0 AND a.kind='cartao' AND t.date BETWEEN ? AND ?",
                p![a.clone(), b.clone()],
            )?;
            let debit = -self.scalar(
                "SELECT COALESCE(SUM(t.amount_cents),0) FROM transactions t JOIN accounts a ON a.id=t.account_id
                 WHERE t.kind='normal' AND t.amount_cents<0 AND a.kind='conta' AND t.date BETWEEN ? AND ?",
                p![a, b],
            )?;
            share.push(json!({"month": iso(m)[..7], "card_cents": card, "debit_cents": debit,
                "card_ratio": if card + debit > 0 { json!(card as f64 / (card + debit) as f64) } else { Value::Null }}));
        }
        if let Some(Value::Object(h)) = o.get_mut("health") {
            h.insert("next_invoices_month".into(), json!(month));
            h.insert("next_invoices_cents".into(), json!(next));
            h.insert("next_invoices_ratio".into(), if fixed_inc > 0 { json!(next as f64 / fixed_inc as f64) } else { Value::Null });
            h.insert("invoice_months".into(), Value::Array(series.into_iter().map(|(m, v)| json!({"month": m, "total_cents": v})).collect()));
            h.insert("invoice_growth_streak".into(), json!(streak));
            h.insert("card_share".into(), Value::Array(share));
        }
        Ok(())
    }

    // ---------------------------------------------------------- backup

    pub fn export_all(&self) -> R<Obj> {
        let mut o = Obj::new();
        o.insert("app".into(), json!("financas-local"));
        o.insert("version".into(), json!(1));
        o.insert(
            "exported_at".into(),
            json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
        );
        for t in TABLES {
            o.insert(
                t.to_string(),
                objs(self.rows(&format!("SELECT * FROM {t}"), p![])?),
            );
        }
        Ok(o)
    }

    fn table_columns(&self, table: &str) -> R<Vec<String>> {
        Ok(self
            .rows(&format!("PRAGMA table_info({table})"), p![])?
            .iter()
            .map(|r| str_of(r, "name"))
            .collect())
    }

    pub fn import_all(&self, data: &Obj) -> R<()> {
        if data.get("app").and_then(|v| v.as_str()) != Some("financas-local") {
            return Err("Arquivo não é um backup do Finanças Local".into());
        }
        self.atomic(|db| {
            for t in TABLES.iter().filter(|t| **t != "settings") {
                db.exec(&format!("DELETE FROM {t}"), p![])?;
            }
            for t in TABLES {
                let valid = db.table_columns(t)?;
                let Some(Value::Array(list)) = data.get(*t) else {
                    continue;
                };
                for r in list {
                    let Some(r) = r.as_object() else { continue };
                    if *t == "settings" {
                        db.exec(
                            "INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)",
                            vec![
                                sql(need(r, "key")?),
                                sql(r.get("value").unwrap_or(&Value::Null)),
                            ],
                        )?;
                        continue;
                    }
                    let cols: Vec<&String> = r.keys().collect();
                    if let Some(bad) = cols.iter().find(|c| !valid.contains(c)) {
                        return Err(format!("coluna desconhecida em {t}: {bad}"));
                    }
                    let marks = vec!["?"; cols.len()].join(",");
                    let names = cols
                        .iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    db.exec(
                        &format!("INSERT INTO {t}({names}) VALUES({marks})"),
                        r.values().map(sql).collect(),
                    )?;
                }
            }
            Ok(())
        })?;
        self.ensure_recurrences(62)
    }

    /// Cópia diária do banco em JSON na pasta de dados (mantém as últimas `keep`).
    pub fn auto_backup(&self, keep: usize) -> R<PathBuf> {
        let folder = self.data_dir.join("backups");
        std::fs::create_dir_all(&folder).map_err(|x| x.to_string())?;
        let name = folder.join(format!("backup-{}.json", iso(self.today())));
        if !name.exists() && !self.is_empty()? {
            let txt = serde_json::to_string(&Value::Object(self.export_all()?))
                .map_err(|x| x.to_string())?;
            std::fs::write(&name, txt).map_err(|x| x.to_string())?;
        }
        let mut files: Vec<String> = std::fs::read_dir(&folder)
            .map_err(|x| x.to_string())?
            .filter_map(|f| f.ok().map(|f| f.file_name().to_string_lossy().into_owned()))
            .filter(|n| n.starts_with("backup-"))
            .collect();
        files.sort();
        if files.len() > keep {
            for old in &files[..files.len() - keep] {
                let _ = std::fs::remove_file(folder.join(old));
            }
        }
        Ok(folder)
    }

    /// Apaga todos os dados (contas, cartões, categorias, contas fixas, lançamentos e faturas),
    /// mantendo as configurações. Antes, salva um backup completo em `backups/`.
    pub fn reset_all(&self) -> R<PathBuf> {
        let folder = self.data_dir.join("backups");
        std::fs::create_dir_all(&folder).map_err(|x| x.to_string())?;
        let name = folder.join(format!("antes-de-apagar-{}.json", Local::now().format("%Y-%m-%d-%H%M%S")));
        let txt = serde_json::to_string(&Value::Object(self.export_all()?)).map_err(|x| x.to_string())?;
        std::fs::write(&name, txt).map_err(|x| x.to_string())?;
        self.atomic(|db| {
            for t in TABLES.iter().filter(|t| **t != "settings") {
                db.exec(&format!("DELETE FROM {t}"), p![])?;
            }
            db.exec("DELETE FROM settings WHERE key='seed_version'", p![])?;
            Ok(())
        })?;
        Ok(name)
    }

    /// Importa um conjunto de dados inicial se o banco estiver vazio (usado nos testes e na
    /// comparação com a versão Python). Se vier uma versão mais nova dos dados
    /// e ainda não houver lançamentos feitos à mão, atualiza sozinho (com backup antes).
    pub fn first_run_import(&self, seed: &Obj) -> R<bool> {
        let version = match seed.get("seed_version") {
            Some(Value::String(s)) => s.clone(),
            Some(v @ Value::Number(_)) => v.to_string(),
            _ => "1".into(),
        };
        let mut vs = Obj::new();
        vs.insert("seed_version".into(), json!(version));
        if self.is_empty()? {
            self.import_all(seed)?;
            self.save_settings(&vs)?;
            return Ok(true);
        }
        let current = self.setting("seed_version").unwrap_or_else(|| "1".into());
        if current != version {
            let own = self.scalar(
                "SELECT COUNT(*) n FROM transactions WHERE created_at <> '2026-11-01 00:00:00' AND recurrence_id IS NULL",
                p![],
            )?;
            if own == 0 {
                self.auto_backup(15)?;
                self.import_all(seed)?;
                self.save_settings(&vs)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Vencimento em fim de semana ou feriado: o pagamento sem juros vale até o próximo dia útil.
/// Acrescenta `pay_until` (data) e `grace_days` (dias de folga, 0 se já é dia útil).
fn add_grace(o: &mut Obj, due: NaiveDate) {
    let pay = next_business_day(due);
    o.insert("pay_until".into(), json!(iso(pay)));
    o.insert("grace_days".into(), json!((pay - due).num_days()));
}

pub fn objs(v: Vec<Obj>) -> Value {
    Value::Array(v.into_iter().map(Value::Object).collect())
}

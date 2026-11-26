//! Confere os valores de 25/11/2026 (seção 2 do briefing) a partir de dados_iniciais.json.
use financas_core::{api, dates::parse, Db};
use serde_json::{json, Value};

fn db_hoje() -> (Db, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "financas-teste-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Db::open(&dir.join("financas.db"), &dir).unwrap();
    db.set_today(Some(parse("2026-11-25").unwrap()));
    assert!(db.first_run_import(&financas_core::seed()).unwrap());
    (db, dir)
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn by_name<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == name)
        .unwrap_or_else(|| panic!("sem {name}"))
}

#[test]
fn saldos_das_contas() {
    let (db, _) = db_hoje();
    let meta = api::route(&db, "GET", "meta", None).unwrap();
    let accs = &meta["accounts"];
    assert_eq!(by_name(accs, "Itaú - Principal")["balance_cents"], 3460);
    assert_eq!(by_name(accs, "Ifood - Vale")["balance_cents"], 130);
    assert_eq!(by_name(accs, "Contas de Casa")["balance_cents"], 0);
    let mp = by_name(accs, "Mercado Pago - Rendimento");
    assert_eq!(mp["archived"], 1);
    assert_eq!(mp["balance_cents"], 1600);
}

#[test]
fn disponivel_nos_cartoes() {
    let (db, _) = db_hoje();
    let s = api::route(&db, "GET", "summary", None).unwrap();
    assert_eq!(by_name(&s["cards"], "Nubank")["available_cents"], 1422862);
    assert_eq!(
        by_name(&s["cards"], "Magazine - Itáu")["available_cents"],
        653041
    );
    assert_eq!(
        by_name(&s["cards"], "Itaú Platium - Visa")["available_cents"],
        346529
    );
}

#[test]
fn faturas_a_pagar() {
    let (db, _) = db_hoje();
    let s = api::route(&db, "GET", "summary", None).unwrap();
    assert_eq!(s["invoices_cents"], -943772);
    let platinum_fechada = s["invoices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["card"] == "Itaú Platium - Visa" && i["state"] == "fechada")
        .expect("fatura fechada do Platinum");
    assert_eq!(platinum_fechada["total_cents"], -338492);
    assert_eq!(platinum_fechada["due"], "2026-12-01");
}

#[test]
fn historico_completo() {
    let (db, _) = db_hoje();
    let n: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM transactions WHERE created_at='2026-11-01 00:00:00'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1448);
    // a importação não pode ser repetida por cima dos dados
    assert!(!db.first_run_import(&financas_core::seed()).unwrap());
}

#[test]
fn compra_depois_do_fechamento_vai_para_a_proxima_fatura() {
    let (db, _) = db_hoje();
    // Nubank fecha dia 25 e vence dia 2
    let r = api::route(&db, "POST", "transactions", Some(json!({"description": "x", "amount_cents": -100, "date": "2026-11-26", "account_id": 4}))).unwrap();
    let t = db
        .one(
            "SELECT invoice_due, paid FROM transactions WHERE id=?",
            vec![r["id"].as_i64().unwrap().into()],
        )
        .unwrap()
        .unwrap();
    assert_eq!(t["invoice_due"], "2027-01-02");
    assert_eq!(t["paid"], 1);
}

#[test]
fn backup_ida_e_volta() {
    let (db, _) = db_hoje();
    let antes = api::route(&db, "GET", "summary", None).unwrap();
    let exp = api::route(&db, "GET", "backup", None).unwrap();
    api::route(&db, "POST", "backup", Some(exp)).unwrap();
    let depois = api::route(&db, "GET", "summary", None).unwrap();
    assert_eq!(antes, depois);
}

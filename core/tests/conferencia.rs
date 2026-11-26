//! Confere os valores de 25/11/2026 (seção 2 do briefing) a partir de tests/dados/dados_iniciais.json.
//! Esse arquivo tem dados pessoais e não vai no instalador; sem ele, os testes de conferência são pulados.
use financas_core::{api, dates::parse, Db};
use serde_json::{json, Map, Value};

const SEED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/dados/dados_iniciais.json");

fn seed() -> Option<Map<String, Value>> {
    std::fs::read_to_string(SEED).ok().map(|s| serde_json::from_str(&s).unwrap())
}

fn db_vazio() -> Db {
    let dir = std::env::temp_dir().join(format!("financas-teste-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Db::open(&dir.join("financas.db"), &dir).unwrap();
    db.set_today(Some(parse("2026-11-25").unwrap()));
    db
}

/// Banco com o histórico do Organizze, ou None (teste pulado) se o arquivo não estiver no repositório.
fn db_hoje() -> Option<(Db, ())> {
    let Some(seed) = seed() else {
        eprintln!("pulado: {SEED} não encontrado");
        return None;
    };
    let db = db_vazio();
    assert!(db.first_run_import(&seed).unwrap());
    Some((db, ()))
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
    let Some((db, _)) = db_hoje() else { return };
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
    let Some((db, _)) = db_hoje() else { return };
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
    let Some((db, _)) = db_hoje() else { return };
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
    let Some((db, _)) = db_hoje() else { return };
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
    assert!(!db.first_run_import(&seed().unwrap()).unwrap());
}

#[test]
fn compra_depois_do_fechamento_vai_para_a_proxima_fatura() {
    let Some((db, _)) = db_hoje() else { return };
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
    let Some((db, _)) = db_hoje() else { return };
    let antes = api::route(&db, "GET", "summary", None).unwrap();
    let exp = api::route(&db, "GET", "backup", None).unwrap();
    api::route(&db, "POST", "backup", Some(exp)).unwrap();
    let depois = api::route(&db, "GET", "summary", None).unwrap();
    assert_eq!(antes, depois);
}

#[test]
fn instalacao_nova_comeca_vazia() {
    let db = db_vazio();
    let s = api::route(&db, "GET", "summary", None).unwrap();
    assert_eq!(s["balance_cents"], 0);
    assert_eq!(s["accounts"].as_array().unwrap().len(), 0);
    let meta = api::route(&db, "GET", "meta", None).unwrap();
    assert_eq!(meta["settings"]["lembrete_diario"], "21:00");
}

#[test]
fn arquivar_preserva_o_historico() {
    let db = db_vazio();
    let a = api::route(&db, "POST", "accounts", Some(json!({"name": "Banco X", "kind": "conta", "balance_cents": 10000}))).unwrap()["id"].clone();
    let b = api::route(&db, "POST", "accounts", Some(json!({"name": "Banco Y", "kind": "conta", "balance_cents": 500}))).unwrap()["id"].clone();
    api::route(&db, "POST", "transactions", Some(json!({"description": "Mercado", "amount_cents": -2500, "date": "2026-11-20", "account_id": a}))).unwrap();
    assert_eq!(api::route(&db, "GET", "summary", None).unwrap()["balance_cents"], 8000);

    // arquivar: some do saldo, o lançamento continua nos relatórios
    api::route(&db, "PUT", &format!("accounts/{a}"), Some(json!({"archived": 1}))).unwrap();
    let s = api::route(&db, "GET", "summary", None).unwrap();
    assert_eq!(s["balance_cents"], 500);
    let tx = api::route(&db, "GET", "transactions?start=2026-11-01&end=2026-11-30", None).unwrap();
    assert_eq!(tx["items"].as_array().unwrap().len(), 1);
    assert_eq!(tx["expense_cents"], -2500);

    // reativar: volta com o mesmo saldo
    api::route(&db, "PUT", &format!("accounts/{a}"), Some(json!({"archived": 0}))).unwrap();
    assert_eq!(api::route(&db, "GET", "summary", None).unwrap()["balance_cents"], 8000);

    // excluir conta com histórico só arquiva; sem histórico, exclui
    assert_eq!(api::route(&db, "DELETE", &format!("accounts/{a}"), None).unwrap()["resultado"], "arquivada");
    assert_eq!(api::route(&db, "DELETE", &format!("accounts/{b}"), None).unwrap()["resultado"], "excluida");
}

#[test]
fn apagar_tudo_faz_backup_e_mantem_configuracoes() {
    let db = db_vazio();
    api::route(&db, "POST", "accounts", Some(json!({"name": "Banco X", "kind": "conta", "balance_cents": 10000}))).unwrap();
    api::route(&db, "POST", "settings", Some(json!({"ntfy_topico": "meu-topico"}))).unwrap();
    assert!(api::route(&db, "POST", "reset", Some(json!({}))).is_err(), "sem confirmação não apaga");
    let r = api::route(&db, "POST", "reset", Some(json!({"confirm": "APAGAR"}))).unwrap();
    let backup = std::path::PathBuf::from(r["backup"].as_str().unwrap());
    assert!(backup.exists());
    let exp: Value = serde_json::from_str(&std::fs::read_to_string(&backup).unwrap()).unwrap();
    assert_eq!(exp["accounts"].as_array().unwrap().len(), 1);
    let meta = api::route(&db, "GET", "meta", None).unwrap();
    assert_eq!(meta["accounts"].as_array().unwrap().len(), 0);
    assert_eq!(meta["settings"]["ntfy_topico"], "meu-topico");
    // o backup volta tudo
    api::route(&db, "POST", "backup", Some(exp)).unwrap();
    assert_eq!(api::route(&db, "GET", "summary", None).unwrap()["balance_cents"], 10000);
}

#[test]
fn aviso_de_fechamento_das_faturas() {
    let Some((db, _)) = db_hoje() else { return };
    let state = db.data_dir.join("lembretes.json");
    let n = financas_core::reminders::closing_notice(&db, &state, 1, false).unwrap().expect("aviso");
    println!("{}\n{}", n.title, n.message);
    assert!(n.message.contains("Nubank fecha hoje (25/11)"));
    assert!(n.message.contains("Magazine - Itáu fecha amanhã (26/11)"));
    assert!(n.message.contains("Itaú Platium - Visa fechou em R$ 3.384,92. Vence terça, 01/12."));
    // não repete
    assert!(financas_core::reminders::closing_notice(&db, &state, 1, false).unwrap().is_none());
}

#[test]
fn folga_no_vencimento() {
    let db = db_vazio();
    // cartão que vence dia 5: em dezembro/2026 cai num sábado
    api::route(&db, "POST", "accounts", Some(json!({"name": "Conta", "kind": "conta", "balance_cents": 0}))).unwrap();
    let c = api::route(&db, "POST", "accounts", Some(json!({"name": "Cartão", "kind": "cartao", "closing_day": 27, "due_day": 5, "limit_cents": 100000}))).unwrap()["id"].clone();
    api::route(&db, "POST", "transactions", Some(json!({"description": "x", "amount_cents": -1000, "date": "2026-11-20", "account_id": c}))).unwrap();
    let inv = api::route(&db, "GET", &format!("invoices?card={c}"), None).unwrap();
    let i = inv["invoices"].as_array().unwrap().iter().find(|i| i["due"] == "2026-12-05").unwrap();
    assert_eq!(i["pay_until"], "2026-12-07");
    assert_eq!(i["grace_days"], 2);
}

#[test]
fn saude_financeira() {
    let Some((db, _)) = db_hoje() else { return };
    let s = api::route(&db, "GET", "summary", None).unwrap();
    let h = &s["health"];
    assert_eq!(h["savings_cents"], 1023153);
    assert_eq!(h["income_cents"], 1874924);
    assert_eq!(h["fixed_expense_cents"], 295984);
    assert_eq!(h["fixed_income_cents"], 670598);
    assert_eq!(h["cards_limit_cents"], 3372600);
    assert_eq!(h["cards_used_cents"], 950168);
    assert_eq!(h["after_invoices_cents"], 1864310 - 943772);
    assert!((h["savings_ratio"].as_f64().unwrap() - 0.5457).abs() < 0.001);
}

#[test]
fn bola_de_neve_do_cartao() {
    let Some((db, _)) = db_hoje() else { return };
    let h = api::route(&db, "GET", "summary", None).unwrap()["health"].clone();
    assert_eq!(h["next_invoices_month"], "2026-12");
    assert_eq!(h["next_invoices_cents"], 941193);
    assert_eq!(h["invoice_growth_streak"], 4);
    let months: Vec<i64> = h["invoice_months"].as_array().unwrap().iter().map(|m| m["total_cents"].as_i64().unwrap()).collect();
    assert_eq!(&months[2..], &[212367, 244777, 303900, 941193]);
    let nov = &h["card_share"][2];
    assert_eq!(nov["month"], "2026-11");
    assert_eq!(nov["card_cents"], 453553);
    assert_eq!(nov["debit_cents"], 398218);
    assert!((h["next_invoices_ratio"].as_f64().unwrap() - 1.4035).abs() < 0.001);
}

#[test]
fn categoria_com_icone() {
    let db = db_vazio();
    let id = api::route(&db, "POST", "categories", Some(json!({"name": "Mercado", "kind": "despesa", "color": "#F9845D", "icon": "cart"}))).unwrap()["id"].clone();
    api::route(&db, "PUT", &format!("categories/{id}"), Some(json!({"name": "Mercado", "kind": "despesa", "color": "#167A56", "icon": "food"}))).unwrap();
    let meta = api::route(&db, "GET", "meta", None).unwrap();
    let c = &meta["categories"][0];
    assert_eq!(c["icon"], "food");
    assert_eq!(c["color"], "#167A56");
}

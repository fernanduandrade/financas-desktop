//! Lembretes: mesma lógica do `check_reminders` do financas.pyw.
//! Devolve os avisos a mostrar; quem chama decide como mostrar (janela própria, sistema, ntfy).
use crate::dates::{brl, parse};
use crate::db::{Db, R};
use chrono::{Datelike, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Um botão do aviso. `route`: "" abre o app, "novo" abre um lançamento, "snooze" lembra de novo
/// em 60 min, outra rota abre essa tela; `None` só fecha.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Action {
    pub label: String,
    pub route: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Notice {
    pub title: String,
    pub message: String,
    pub kind: String,
    pub actions: Vec<Action>,
    pub push: bool,
}

fn act(label: &str, route: Option<&str>) -> Action {
    Action {
        label: label.into(),
        route: route.map(String::from),
    }
}

impl Notice {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        kind: &str,
        actions: Vec<Action>,
    ) -> Notice {
        Notice {
            title: title.into(),
            message: message.into(),
            kind: kind.into(),
            actions,
            push: true,
        }
    }

    pub fn test() -> Notice {
        Notice::new(
            "Aviso de teste",
            "Está funcionando. É assim que os lembretes vão aparecer.",
            "info",
            vec![act("Abrir app", Some("")), act("Ok", None)],
        )
    }
}

pub fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join("lembretes.json")
}

fn load_state(p: &Path) -> Map<String, Value> {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn mark(p: &Path, key: &str, value: &str) {
    let mut st = load_state(p);
    st.insert(key.into(), Value::String(value.into()));
    let _ = std::fs::write(p, Value::Object(st).to_string());
}

fn hm(s: Option<String>, default: &str) -> NaiveTime {
    let s = s
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| default.into());
    let parse_hm = |x: &str| -> Option<NaiveTime> {
        let mut it = x.split(':');
        let h: u32 = it.next()?.trim().parse().ok()?;
        let m: u32 = it.next()?.trim().parse().ok()?;
        if it.next().is_some() {
            return None;
        }
        NaiveTime::from_hms_opt(h, m, 0)
    };
    parse_hm(&s).or_else(|| parse_hm(default)).unwrap()
}

pub fn bills_message(db: &Db, days: i64) -> R<Option<(String, String)>> {
    let items = db.upcoming(days)?;
    let pay: Vec<_> = items
        .iter()
        .filter(|i| i["amount_cents"].as_i64().unwrap_or(0) < 0)
        .collect();
    if pay.is_empty() {
        return Ok(None);
    }
    let late = pay
        .iter()
        .filter(|i| i["overdue"].as_bool().unwrap_or(false))
        .count();
    let mut lines = Vec::new();
    for i in pay.iter().take(6) {
        let d = parse(i["date"].as_str().unwrap_or(""))?.format("%d/%m");
        lines.push(format!(
            "{}{} {}: {}",
            if i["overdue"].as_bool().unwrap_or(false) {
                "ATRASADA "
            } else {
                ""
            },
            d,
            i["description"].as_str().unwrap_or(""),
            brl(i["amount_cents"].as_i64().unwrap_or(0))
        ));
    }
    if pay.len() > 6 {
        lines.push(format!("+ {} itens", pay.len() - 6));
    }
    let title = if late > 0 {
        format!("{late} conta(s) atrasada(s)")
    } else {
        format!("Contas vencendo em até {days} dias")
    };
    Ok(Some((title, lines.join("\n"))))
}

/// `force`: "contas", "diario" ou "semanal" para disparar já (menu da bandeja / testes).
pub fn check_reminders(db: &Db, now: NaiveDateTime, force: Option<&str>) -> R<Vec<Notice>> {
    let sp = state_path(&db.data_dir);
    let s = |k: &str| db.setting(k);
    let st = load_state(&sp);
    let stv = |k: &str| st.get(k).and_then(|v| v.as_str()).map(String::from);
    let today = now.date().format("%Y-%m-%d").to_string();
    let mut out = Vec::new();

    // 1) contas a vencer (uma vez por dia, a partir do horário escolhido ou ao ligar o PC depois dele)
    if force == Some("contas")
        || (now.time() >= hm(s("aviso_contas_hora"), "09:00")
            && stv("contas").as_deref() != Some(&today))
    {
        let days = s("aviso_contas_dias")
            .and_then(|x| x.trim().parse::<i64>().ok())
            .unwrap_or(2);
        if let Some((t, m)) = bills_message(db, days)? {
            out.push(Notice::new(
                t,
                m,
                "contas",
                vec![act("Ver contas", Some("inicio")), act("Ok", None)],
            ));
        }
        mark(&sp, "contas", &today);
    }

    // 2) lembrete diário de lançar os gastos
    let on = |k: &str| s(k).unwrap_or_else(|| "1".into()) == "1";
    if on("lembrete_diario_ativo")
        && (force == Some("diario")
            || (now.time() >= hm(s("lembrete_diario"), "21:00")
                && stv("diario").as_deref() != Some(&today)))
    {
        let n = db.summary()?["launched_today"].as_i64().unwrap_or(0);
        let acts = vec![
            act("Lançar agora", Some("novo")),
            act("Mais tarde", Some("snooze")),
        ];
        if n == 0 {
            out.push(Notice::new(
                "Já lançou os gastos de hoje?",
                "Nada registrado hoje ainda. Leva 1 minuto.",
                "diario",
                acts,
            ));
        } else {
            out.push(Notice::new(
                "Finanças em dia?",
                format!("Você registrou {n} lançamento(s) hoje. Faltou algum gasto?"),
                "diario",
                acts,
            ));
        }
        mark(&sp, "diario", &today);
    }

    // 3) revisão semanal (domingo)
    let iw = now.date().iso_week();
    let week = format!("{}-{}", iw.year(), iw.week());
    if on("revisao_semanal_ativo")
        && (force == Some("semanal")
            || (now.weekday() == chrono::Weekday::Sun
                && now.time() >= hm(s("revisao_semanal_hora"), "19:00")
                && stv("semanal").as_deref() != Some(&week)))
    {
        let sm = db.summary()?;
        out.push(Notice::new(
            "Revisão semanal",
            format!(
                "Saldo em contas: {}\nFaturas a pagar: {}\nGastos do mês até agora: {}",
                brl(sm["balance_cents"].as_i64().unwrap_or(0)),
                brl(sm["invoices_cents"].as_i64().unwrap_or(0)),
                brl(sm["month"]["expense_cents"].as_i64().unwrap_or(0))
            ),
            "semanal",
            vec![act("Ver relatórios", Some("relatorios")), act("Ok", None)],
        ));
        mark(&sp, "semanal", &week);
    }
    Ok(out)
}

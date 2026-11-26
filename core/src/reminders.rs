//! Lembretes: mesma lógica do `check_reminders` do financas.pyw.
//! Devolve os avisos a mostrar; quem chama decide como mostrar (janela própria, sistema, ntfy).
use crate::dates::{brl, closing_date_for, grace_text, parse, weekday_pt};
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

    let on = |k: &str| s(k).unwrap_or_else(|| "1".into()) == "1";

    // 2) fechamento das faturas: avisa antes de fechar e quando fechou (uma vez por fatura)
    if on("aviso_fechamento_ativo")
        && (force == Some("fechamento") || now.time() >= hm(s("aviso_contas_hora"), "09:00"))
    {
        let days = s("aviso_fechamento_dias").and_then(|x| x.trim().parse::<i64>().ok()).unwrap_or(1);
        if let Some(n) = closing_notice(db, &sp, days, force == Some("fechamento"))? {
            out.push(n);
        }
    }

    // 3) lembrete diário de lançar os gastos
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

    // 4) revisão semanal (domingo)
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

fn dm(d: chrono::NaiveDate) -> String {
    format!("{}, {}", weekday_pt(d), d.format("%d/%m"))
}

/// Aviso de fechamento das faturas dos cartões ativos:
/// - "fecha amanhã": até `days` dias antes do fechamento, com o total até agora e para quando
///   vão as compras feitas depois;
/// - "fechou": no dia seguinte ao fechamento, com o total, o vencimento e a folga se o
///   vencimento cair em fim de semana ou feriado.
/// Cada evento é avisado uma vez por fatura (fica marcado em lembretes.json).
pub fn closing_notice(db: &Db, state: &Path, days: i64, force: bool) -> R<Option<Notice>> {
    let today = db.today();
    let st = load_state(state);
    let done = |k: &str| !force && st.contains_key(k);
    let mut lines = Vec::new();
    let mut keys = Vec::new();
    for card in db.rows("SELECT * FROM accounts WHERE kind='cartao' AND archived=0 ORDER BY sort, name", vec![])? {
        let id = card["id"].as_i64().unwrap_or(0);
        let name = card["name"].as_str().unwrap_or("").to_string();
        let (cl, du) = (card["closing_day"].as_i64().unwrap_or(1), card["due_day"].as_i64().unwrap_or(1));
        let invs = db.list_invoices(id)?;
        let total_of = |due: &str| invs.iter().find(|i| i["due"] == due).and_then(|i| i["total_cents"].as_i64()).unwrap_or(0);

        // fatura aberta que está para fechar
        let cur = db.current_invoice(&card, None);
        let close = closing_date_for(cur, cl, du);
        let left = (close - today).num_days();
        let key = format!("fecha:{id}:{}", crate::dates::iso(cur));
        if (0..=days).contains(&left) && !done(&key) {
            let quando = match left { 0 => "hoje".to_string(), 1 => "amanhã".to_string(), n => format!("em {n} dias") };
            let next_due = crate::dates::invoice_due_for(close + chrono::Duration::days(1), cl, du);
            lines.push(format!(
                "{name} fecha {quando} ({}), com {} até agora. Compras a partir de {} vão para a fatura que vence em {}.",
                close.format("%d/%m"),
                brl(total_of(&crate::dates::iso(cur)).abs()),
                (close + chrono::Duration::days(1)).format("%d/%m"),
                next_due.format("%d/%m")
            ));
            keys.push(key);
        }

        // fatura que acabou de fechar (fechou ontem ou antes, ainda não paga)
        for inv in invs.iter().filter(|i| i["state"] == "fechada") {
            let due = parse(inv["due"].as_str().unwrap_or(""))?;
            let key = format!("fechou:{id}:{}", crate::dates::iso(due));
            if done(&key) {
                continue;
            }
            let mut line = format!(
                "{name} fechou em {}. Vence {}",
                brl(inv["total_cents"].as_i64().unwrap_or(0).abs()),
                dm(due)
            );
            match grace_text(due) {
                Some(g) => line.push_str(&format!("; {g}.")),
                None => line.push('.'),
            }
            lines.push(line);
            keys.push(key);
        }
    }
    if lines.is_empty() {
        return Ok(None);
    }
    for k in &keys {
        mark(state, k, &crate::dates::iso(today));
    }
    let title = if lines.len() == 1 { "Fatura do cartão".to_string() } else { format!("{} avisos de fatura", lines.len()) };
    Ok(Some(Notice::new(title, lines.join("\n"), "contas", vec![act("Ver cartões", Some("cartoes")), act("Ok", None)])))
}

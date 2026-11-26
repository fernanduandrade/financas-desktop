//! Datas e regras de fatura (iguais às do db.py).
use chrono::{Datelike, NaiveDate};

pub fn days_in_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .unwrap()
        .pred_opt()
        .unwrap()
        .day()
}

/// add_months(d, n, day=None) do Python: soma meses e limita o dia ao fim do mês.
pub fn add_months(d: NaiveDate, n: i32, day: Option<u32>) -> NaiveDate {
    let m0 = d.month0() as i32 + n;
    let y = d.year() + m0.div_euclid(12);
    let m = m0.rem_euclid(12) as u32 + 1;
    let want = match day {
        Some(x) if x != 0 => x,
        _ => d.day(),
    };
    NaiveDate::from_ymd_opt(y, m, want.min(days_in_month(y, m))).unwrap()
}

/// Fatura em que cai uma compra. No dia do fechamento ainda entra na fatura que fecha;
/// depois dele, vai para a seguinte. Vence no mês do fechamento se due_day > closing_day.
pub fn invoice_due_for(date: NaiveDate, closing_day: i64, due_day: i64) -> NaiveDate {
    let mut close = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap();
    if date.day() as i64 > closing_day {
        close = add_months(close, 1, None);
    }
    let due_month = if due_day > closing_day {
        close
    } else {
        add_months(close, 1, None)
    };
    let last = days_in_month(due_month.year(), due_month.month());
    NaiveDate::from_ymd_opt(
        due_month.year(),
        due_month.month(),
        (due_day.max(1) as u32).min(last),
    )
    .unwrap()
}

pub fn closing_date_for(due: NaiveDate, closing_day: i64, due_day: i64) -> NaiveDate {
    let cm = if due_day > closing_day {
        due
    } else {
        add_months(due, -1, None)
    };
    let last = days_in_month(cm.year(), cm.month());
    NaiveDate::from_ymd_opt(cm.year(), cm.month(), (closing_day.max(1) as u32).min(last)).unwrap()
}

pub fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.get(..10).unwrap_or(s), "%Y-%m-%d")
        .map_err(|_| format!("data inválida: {s}"))
}

pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}

pub fn last_of_month(d: NaiveDate) -> NaiveDate {
    add_months(first_of_month(d), 1, None).pred_opt().unwrap()
}

/// Mesmo formato do brl() do Python: "R$ 1.234,56" / "-R$ 1.234,56".
pub fn brl(cents: i64) -> String {
    let v = cents.unsigned_abs();
    let int = (v / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!(
        "{}{},{:02}",
        if cents < 0 { "-R$ " } else { "R$ " },
        grouped,
        v % 100
    )
}

// ------------------------------------------------------------------ dias úteis e feriados

/// Domingo de Páscoa (algoritmo de Meeus/Jones/Butcher, calendário gregoriano).
pub fn easter(y: i32) -> NaiveDate {
    let a = y % 19;
    let b = y / 100;
    let c = y % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(y, month as u32, day as u32).unwrap()
}

/// Feriados nacionais em que os bancos não funcionam (inclui Carnaval e Corpus Christi,
/// que são ponto facultativo mas sem expediente bancário).
pub fn bank_holidays(y: i32) -> Vec<(NaiveDate, &'static str)> {
    let fixed = |m: u32, d: u32| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let e = easter(y);
    let mut v = vec![
        (fixed(1, 1), "Confraternização Universal"),
        (e - chrono::Duration::days(48), "Carnaval"),
        (e - chrono::Duration::days(47), "Carnaval"),
        (e - chrono::Duration::days(2), "Sexta-feira Santa"),
        (fixed(4, 21), "Tiradentes"),
        (fixed(5, 1), "Dia do Trabalho"),
        (e + chrono::Duration::days(60), "Corpus Christi"),
        (fixed(9, 7), "Independência"),
        (fixed(10, 12), "Nossa Senhora Aparecida"),
        (fixed(11, 2), "Finados"),
        (fixed(11, 15), "Proclamação da República"),
        (fixed(12, 25), "Natal"),
    ];
    if y >= 2024 {
        v.push((fixed(11, 20), "Consciência Negra"));
    }
    v.sort();
    v
}

pub fn holiday_name(d: NaiveDate) -> Option<&'static str> {
    bank_holidays(d.year()).into_iter().find(|(h, _)| *h == d).map(|(_, n)| n)
}

pub fn is_business_day(d: NaiveDate) -> bool {
    !matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) && holiday_name(d).is_none()
}

/// Quando o vencimento cai em fim de semana ou feriado, o pagamento sem juros vale até o
/// próximo dia útil.
pub fn next_business_day(d: NaiveDate) -> NaiveDate {
    let mut x = d;
    while !is_business_day(x) {
        x = x.succ_opt().unwrap();
    }
    x
}

pub fn weekday_pt(d: NaiveDate) -> &'static str {
    ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"][d.weekday().num_days_from_monday() as usize]
}

/// Por que a data não é dia útil: "sábado", "domingo" ou o nome do feriado.
pub fn non_business_reason(d: NaiveDate) -> Option<String> {
    if let Some(n) = holiday_name(d) {
        return Some(format!("feriado de {n}"));
    }
    match d.weekday() {
        chrono::Weekday::Sat | chrono::Weekday::Sun => Some(weekday_pt(d).to_string()),
        _ => None,
    }
}

/// Texto de folga para um vencimento, ou None se ele já cai em dia útil.
/// Ex.: "cai no sábado: dá para pagar até segunda, 07/12, sem juros (+2 dias)".
pub fn grace_text(due: NaiveDate) -> Option<String> {
    let reason = non_business_reason(due)?;
    let pay = next_business_day(due);
    let days = (pay - due).num_days();
    Some(format!(
        "cai no {reason}: dá para pagar até {}, {}, sem juros (+{days} dia{})",
        weekday_pt(pay),
        pay.format("%d/%m"),
        if days > 1 { "s" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> NaiveDate {
        parse(s).unwrap()
    }

    #[test]
    fn fatura_pela_data() {
        // fecha 25, vence 2 do mês seguinte
        assert_eq!(invoice_due_for(d("2026-11-25"), 25, 2), d("2026-12-02"));
        assert_eq!(invoice_due_for(d("2026-11-26"), 25, 2), d("2027-01-02"));
        // fecha 3, vence 10 no mesmo mês
        assert_eq!(invoice_due_for(d("2026-11-03"), 3, 10), d("2026-11-10"));
        assert_eq!(invoice_due_for(d("2026-11-04"), 3, 10), d("2026-12-10"));
        // vencimento 31 em fevereiro
        assert_eq!(invoice_due_for(d("2027-01-20"), 20, 31), d("2027-01-31"));
        assert_eq!(invoice_due_for(d("2027-01-21"), 20, 31), d("2027-02-28"));
        assert_eq!(closing_date_for(d("2026-12-02"), 25, 2), d("2026-11-25"));
    }

    #[test]
    fn feriados_e_dias_uteis() {
        assert_eq!(easter(2026), d("2026-04-05"));
        assert_eq!(easter(2027), d("2027-03-28"));
        assert_eq!(holiday_name(d("2026-02-16")), Some("Carnaval"));
        assert_eq!(holiday_name(d("2026-04-03")), Some("Sexta-feira Santa"));
        assert_eq!(holiday_name(d("2026-06-04")), Some("Corpus Christi"));
        assert_eq!(next_business_day(d("2026-12-05")), d("2026-12-07")); // sábado → segunda
        assert_eq!(next_business_day(d("2026-12-25")), d("2026-12-28")); // Natal (sexta) → segunda
        assert_eq!(next_business_day(d("2026-11-20")), d("2026-11-23")); // Consciência Negra (sexta)
        assert_eq!(next_business_day(d("2026-12-02")), d("2026-12-02"));
        assert_eq!(grace_text(d("2026-12-02")), None);
        assert_eq!(
            grace_text(d("2026-12-05")).unwrap(),
            "cai no sábado: dá para pagar até segunda, 07/12, sem juros (+2 dias)"
        );
        assert_eq!(
            grace_text(d("2026-12-25")).unwrap(),
            "cai no feriado de Natal: dá para pagar até segunda, 28/12, sem juros (+3 dias)"
        );
    }

    #[test]
    fn meses() {
        assert_eq!(add_months(d("2026-01-31"), 1, None), d("2026-02-28"));
        assert_eq!(add_months(d("2026-01-15"), -2, None), d("2025-11-15"));
        assert_eq!(add_months(d("2026-02-28"), 1, Some(31)), d("2026-03-31"));
        assert_eq!(brl(-123456789), "-R$ 1.234.567,89");
        assert_eq!(brl(5), "R$ 0,05");
    }
}

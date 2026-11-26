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
    fn meses() {
        assert_eq!(add_months(d("2026-01-31"), 1, None), d("2026-02-28"));
        assert_eq!(add_months(d("2026-01-15"), -2, None), d("2025-11-15"));
        assert_eq!(add_months(d("2026-02-28"), 1, Some(31)), d("2026-03-31"));
        assert_eq!(brl(-123456789), "-R$ 1.234.567,89");
        assert_eq!(brl(5), "R$ 0,05");
    }
}

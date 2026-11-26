//! Janela principal, painel da bandeja, menu da bandeja e início com o sistema.
use crate::{log, AppState};
use financas_core::dates::brl;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// Largura do painel em px de CSS (cartão de 320 + 12 de margem para a sombra de cada lado).
const PANEL_W: f64 = 344.0;

static PANEL_ANCHOR: Mutex<Option<(f64, f64)>> = Mutex::new(None);

pub fn create_main(app: &AppHandle, visible: bool) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Finanças")
        .inner_size(1200.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .decorations(false) // barra de título própria, no visual do app
        .center()
        .visible(visible)
        .build()?;
    Ok(w)
}

pub fn create_panel(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, "panel", WebviewUrl::App("panel.html".into()))
        .title("Finanças")
        .inner_size(PANEL_W, 420.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build()
}

/// Mostra o app (e navega para uma tela: "novo", "inicio", "relatorios", "lembretes"…).
pub fn open_app(app: &AppHandle, route: &str) {
    let w = match app.get_webview_window("main") {
        Some(w) => w,
        None => match create_main(app, true) {
            Ok(w) => w,
            Err(e) => return log(format!("abrir app: {e}")),
        },
    };
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    let js = format!(
        "window.__abrir && window.__abrir({})",
        Value::String(route.to_string())
    );
    let _ = w.eval(&js);
}

// ------------------------------------------------------------------ painel

pub fn toggle_panel(app: &AppHandle, tray_rect: Option<tauri::Rect>) {
    let Some(panel) = app.get_webview_window("panel") else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        hide_panel(app);
        return;
    }
    // o clique no ícone que acabou de fechar o painel (por perder o foco) não reabre
    let st = app.state::<AppState>();
    if let Some(t) = *st.panel_hidden_at.lock().unwrap() {
        if t.elapsed() < Duration::from_millis(350) {
            return;
        }
    }
    *PANEL_ANCHOR.lock().unwrap() = tray_rect.map(|r| {
        let p = r.position.to_physical::<f64>(1.0);
        (p.x, p.y)
    });
    // o painel busca o resumo, mede a altura e chama panel_fit, que posiciona e mostra
    let _ = app.emit_to("panel", "panel:show", ());
}

pub fn hide_panel(app: &AppHandle) {
    if let Some(p) = app.get_webview_window("panel") {
        if p.is_visible().unwrap_or(false) {
            let _ = p.hide();
            *app.state::<AppState>().panel_hidden_at.lock().unwrap() = Some(Instant::now());
        }
    }
}

/// Área útil (sem a barra de tarefas) do monitor onde está o ponto, ou do principal.
pub fn work_area(app: &AppHandle, point: Option<(f64, f64)>) -> Option<(i32, i32, i32, i32, f64)> {
    let m = point
        .and_then(|(x, y)| app.monitor_from_point(x, y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let wa = m.work_area();
    Some((
        wa.position.x,
        wa.position.y,
        wa.position.x + wa.size.width as i32,
        wa.position.y + wa.size.height as i32,
        m.scale_factor(),
    ))
}

#[tauri::command]
pub async fn panel_fit(app: AppHandle, height: f64) -> Result<(), String> {
    let p = app.get_webview_window("panel").ok_or("sem painel")?;
    let anchor = *PANEL_ANCHOR.lock().unwrap();
    let (l, t, r, b, sf) = work_area(&app, anchor).ok_or("sem monitor")?;
    let _ = l;
    let (w, h) = ((PANEL_W * sf).round() as i32, (height * sf).round() as i32);
    // Windows: acima do relógio, como os painéis do Windows 11. Ubuntu: abaixo da barra do topo.
    let (x, y) = if cfg!(windows) {
        (r - w, b - h)
    } else {
        (r - w, t)
    };
    p.set_size(PhysicalSize::new(w as u32, h as u32))
        .map_err(|e| e.to_string())?;
    p.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    p.show().map_err(|e| e.to_string())?;
    let _ = p.set_focus();
    Ok(())
}

#[tauri::command]
pub async fn panel_action(app: AppHandle, what: String) {
    hide_panel(&app);
    match what.as_str() {
        "abrir" => open_app(&app, ""),
        "novo" => open_app(&app, "novo"),
        "lembretes" => open_app(&app, "lembretes"),
        "sair" => app.exit(0),
        "fechar" => {}
        other => open_app(&app, other),
    }
}

// ------------------------------------------------------------------ menu da bandeja

pub fn refresh_tray(app: &AppHandle) {
    let st = app.state::<AppState>();
    let lines = match st.db.lock().unwrap().summary() {
        Ok(sm) => {
            let num = |v: &Value| v.as_i64().unwrap_or(0);
            let next = sm["upcoming"]
                .as_array()
                .and_then(|a| a.iter().find(|i| num(&i["amount_cents"]) < 0))
                .map(|i| {
                    let d = i["date"].as_str().unwrap_or("");
                    format!(
                        "Próxima: {} {}/{}",
                        i["description"].as_str().unwrap_or(""),
                        d.get(8..10).unwrap_or(""),
                        d.get(5..7).unwrap_or("")
                    )
                })
                .unwrap_or_else(|| "Nada a pagar em 15 dias".into());
            vec![
                format!("Saldo em contas: {}", brl(num(&sm["balance_cents"]))),
                format!("Faturas a pagar: {}", brl(num(&sm["invoices_cents"]))),
                next,
            ]
        }
        Err(e) => vec![
            "Finanças Local".into(),
            e.chars().take(60).collect(),
            String::new(),
        ],
    };
    for (item, text) in st.tray_lines.lock().unwrap().iter().zip(lines.iter()) {
        let _ = item.set_text(text);
    }
    if let Some(tray) = app.tray_by_id("financas") {
        let _ = tray.set_tooltip(Some(format!("Finanças\n{}", lines[0])));
    }
}

// ------------------------------------------------------------------ início com o sistema

pub fn set_autostart(app: &AppHandle, on: bool) {
    if cfg!(debug_assertions) {
        return; // em desenvolvimento não registra o executável de teste
    }
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let r = if on { al.enable() } else { al.disable() };
    if let Err(e) = r {
        log(format!("início com o sistema: {e}"));
    }
}

/// Liga o início com o sistema na primeira execução (pode ser desligado em Configurações).
pub fn first_run_autostart(app: &AppHandle) {
    let current = {
        let st = app.state::<AppState>();
        let db = st.db.lock().unwrap();
        db.setting("iniciar_com_sistema")
    };
    match current.as_deref() {
        None => {
            let st = app.state::<AppState>();
            let mut o = serde_json::Map::new();
            o.insert("iniciar_com_sistema".into(), Value::String("1".into()));
            let _ = st.db.lock().unwrap().save_settings(&o);
            set_autostart(app, true);
        }
        Some("1") => set_autostart(app, true), // refaz o registro (o caminho muda nas atualizações)
        _ => {}
    }
}

//! Finanças Local — app Tauri 2.
//!
//! Janelas: `main` (o app), `panel` (painel da bandeja), `toast-N` (avisos) e `shake` (tremor da tela).
//! Toda a regra de negócio está em `financas-core`; o frontend fala com ela por um único comando,
//! `api(method, path, body)`, que repete as rotas do antigo servidor Python.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod notify;
mod windows;

use chrono::Local;
use financas_core::{api as core_api, reminders, Db};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow, WindowEvent};

pub struct AppState {
    pub db: Mutex<Db>,
    pub toasts: Mutex<notify::Toasts>,
    pub shot: Mutex<Option<Vec<u8>>>,
    pub panel_hidden_at: Mutex<Option<Instant>>,
    pub tray_lines: Mutex<Vec<MenuItem<tauri::Wry>>>,
}

/// Registro simples de erros (o app não tem console no Windows).
pub fn log(msg: impl AsRef<str>) {
    let dir = financas_core::db::default_data_dir();
    let line = format!(
        "{} {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        msg.as_ref()
    );
    eprint!("{line}");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("financas-log.txt"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}

// ------------------------------------------------------------------ comandos

/// O mesmo `api(method, path, body)` do app.js, agora sem HTTP.
#[tauri::command]
async fn api(
    app: AppHandle,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, String> {
    let route = path
        .split('?')
        .next()
        .unwrap_or("")
        .trim_matches('/')
        .to_string();
    if route == "notify-test" && method.eq_ignore_ascii_case("POST") {
        let a = app.clone();
        std::thread::spawn(move || notify::notify(&a, reminders::Notice::test()));
        return Ok(json!({"ok": true}));
    }
    let st = app.state::<AppState>();
    let res = {
        let db = st.db.lock().map_err(|e| e.to_string())?;
        core_api::route(&db, &method, &path, body.clone())
    };
    if route == "settings" && method.eq_ignore_ascii_case("POST") && res.is_ok() {
        if let Some(v) = body
            .as_ref()
            .and_then(|b| b.get("iniciar_com_sistema"))
            .and_then(|v| v.as_str())
        {
            windows::set_autostart(&app, v == "1");
        }
    }
    if !method.eq_ignore_ascii_case("GET") && res.is_ok() {
        let a = app.clone();
        std::thread::spawn(move || windows::refresh_tray(&a));
    }
    res
}

/// Exportar backup: pergunta onde salvar e grava o JSON.
#[tauri::command]
async fn export_backup(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let data = {
        let st = app.state::<AppState>();
        let db = st.db.lock().map_err(|e| e.to_string())?;
        db.export_all()?
    };
    let today = Local::now().format("%Y-%m-%d");
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Exportar backup")
        .set_file_name(format!("financas-backup-{today}.json"))
        .add_filter("Backup do Finanças", &["json"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let txt = serde_json::to_string_pretty(&Value::Object(data)).map_err(|e| e.to_string())?;
    std::fs::write(&path, txt).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Botões da barra de título própria.
#[tauri::command]
async fn window_control(window: WebviewWindow, action: String) -> Result<(), String> {
    let r = match action.as_str() {
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "close" => window.hide(),
        _ => Ok(()),
    };
    r.map_err(|e| e.to_string())
}

#[tauri::command]
fn app_info() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "wayland": notify::is_wayland(),
    })
}

// ------------------------------------------------------------------ bandeja

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let linux = cfg!(target_os = "linux");
    let lines: Vec<MenuItem<tauri::Wry>> = (0..3)
        .map(|i| MenuItem::with_id(app, format!("info{i}"), "…", false, None::<&str>))
        .collect::<Result<_, _>>()?;
    let painel = MenuItem::with_id(app, "painel", "Abrir painel", true, None::<&str>)?;
    let abrir = MenuItem::with_id(app, "abrir", "Abrir app", true, None::<&str>)?;
    let novo = MenuItem::with_id(app, "novo", "Lançamento rápido", true, None::<&str>)?;
    let contas = MenuItem::with_id(app, "contas", "Ver contas a vencer", true, None::<&str>)?;
    let sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = Vec::new();
    if linux {
        // no Ubuntu o clique no ícone só abre o menu: o painel vira o primeiro item
        items.push(&painel);
    }
    for l in &lines {
        items.push(l);
    }
    items.extend([
        &sep1 as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &abrir,
        &novo,
        &contas,
        &sep2,
        &sair,
    ]);
    let menu = Menu::with_items(app, &items)?;

    TrayIconBuilder::with_id("financas")
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/tray.png"
        ))?)
        .tooltip("Finanças")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "painel" => windows::toggle_panel(app, None),
            "abrir" => windows::open_app(app, ""),
            "novo" => windows::open_app(app, "novo"),
            "contas" => {
                let a = app.clone();
                std::thread::spawn(move || run_reminders(&a, Some("contas")));
            }
            "sair" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = ev
            {
                windows::toggle_panel(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;
    *app.state::<AppState>().tray_lines.lock().unwrap() = lines;
    windows::refresh_tray(app);
    Ok(())
}

// ------------------------------------------------------------------ lembretes

fn run_reminders(app: &AppHandle, force: Option<&str>) {
    let notices = {
        let st = app.state::<AppState>();
        let db = st.db.lock().unwrap();
        reminders::check_reminders(&db, Local::now().naive_local(), force)
    };
    match notices {
        Ok(list) => {
            for n in list {
                notify::notify(app, n);
            }
        }
        Err(e) => log(format!("lembretes: {e}")),
    }
}

fn scheduler(app: AppHandle) {
    let mut last_day = None;
    loop {
        let today = Local::now().date_naive();
        if last_day != Some(today) {
            let st = app.state::<AppState>();
            let db = st.db.lock().unwrap();
            if let Err(e) = db.ensure_recurrences(62) {
                log(format!("contas fixas: {e}"));
            }
            if let Err(e) = db.auto_backup(15) {
                log(format!("backup: {e}"));
            }
            last_day = Some(today);
        }
        run_reminders(&app, None);
        windows::refresh_tray(&app);
        std::thread::sleep(Duration::from_secs(30));
    }
}

/// A versão Python usava um servidor na porta 8765. Se ele ainda estiver rodando,
/// os lembretes chegam em dobro: avisa uma vez.
fn warn_if_old_version_running(app: &AppHandle) {
    let port: u16 = {
        let st = app.state::<AppState>();
        let db = st.db.lock().unwrap();
        db.setting("porta")
            .and_then(|p| p.parse().ok())
            .unwrap_or(8765)
    };
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok() {
        let mut n = reminders::Notice::new(
            "A versão antiga ainda está aberta",
            "Feche o Finanças antigo (ícone R$ → Sair) e desinstale com desinstalar.bat, mantendo os dados. Senão os lembretes chegam em dobro.",
            "info",
            vec![reminders::Action { label: "Ok".into(), route: None }],
        );
        n.push = false;
        notify::notify(app, n);
    }
}

// ------------------------------------------------------------------ main

fn main() {
    #[cfg(target_os = "linux")]
    {
        // Wayland não deixa programas posicionarem janelas: usa XWayland para o painel e os avisos
        // ficarem no canto certo. O render DMA-BUF do WebKitGTK falha em algumas placas de vídeo.
        if notify::is_wayland() && std::env::var_os("GDK_BACKEND").is_none() {
            std::env::set_var("GDK_BACKEND", "x11");
        }
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
    let hidden = std::env::args().any(|a| a == "--oculto");

    let db = Db::open_default().expect("não consegui abrir o banco de dados");
    if let Err(e) = db
        .ensure_recurrences(62)
        .and_then(|_| db.auto_backup(15).map(|_| ()))
    {
        log(format!("início: {e}"));
    }

    let app = tauri::Builder::default()
        // abrir de novo só traz a janela; `--painel` abre o painel (útil como atalho de teclado no Ubuntu)
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--painel") {
                windows::toggle_panel(app, None);
            } else if argv.iter().any(|a| a == "--testar-aviso") {
                let a = app.clone();
                std::thread::spawn(move || notify::notify(&a, reminders::Notice::test()));
            } else if !argv.iter().any(|a| a == "--oculto") {
                windows::open_app(app, "");
            }
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--oculto")
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            db: Mutex::new(db),
            toasts: Mutex::new(notify::Toasts::default()),
            shot: Mutex::new(None),
            panel_hidden_at: Mutex::new(None),
            tray_lines: Mutex::new(Vec::new()),
        })
        .register_uri_scheme_protocol("shot", |ctx, _req| {
            // foto da tela usada pelo tremor (fica só na memória)
            let bytes = ctx
                .app_handle()
                .state::<AppState>()
                .shot
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default();
            #[cfg(debug_assertions)]
            log(format!("tremor: foto servida ({} bytes)", bytes.len()));
            tauri::http::Response::builder()
                .header("Content-Type", "image/jpeg")
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes)
                .unwrap()
        })
        .invoke_handler(tauri::generate_handler![
            api,
            export_backup,
            window_control,
            app_info,
            windows::panel_fit,
            windows::panel_action,
            notify::toast_payload,
            notify::toast_ready,
            notify::toast_action,
            notify::toast_close,
            notify::shake_info,
            notify::shake_done,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            windows::create_main(&handle, !hidden)?;
            windows::create_panel(&handle)?;
            build_tray(&handle)?;
            windows::first_run_autostart(&handle);
            let h = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                warn_if_old_version_running(&h);
                scheduler(h);
            });
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                // fechar a janela só esconde: o app continua na bandeja com os lembretes
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "panel" => {
                windows::hide_panel(window.app_handle())
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Finanças");

    app.run(|_app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit(); // só "Sair" encerra
            }
        }
    });
}

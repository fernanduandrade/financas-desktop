//! Avisos: janela própria (com botões, barra de tempo, som e tremor da tela), aviso do sistema
//! como alternativa e push no celular via ntfy.
use crate::windows::{open_app, work_area};
use crate::{log, AppState};
use financas_core::reminders::Notice;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

/// Largura do aviso em px de CSS (cartão de 368 + 12 de margem para a sombra de cada lado).
const TOAST_W: f64 = 392.0;
const TOAST_H_GUESS: f64 = 190.0;
const SNOOZE_MIN: u64 = 60;

#[derive(Default)]
pub struct Toasts {
    next: u64,
    /// avisos na tela, na ordem de empilhamento: (id, altura em px físicos)
    stack: Vec<(u64, i32)>,
    notices: HashMap<u64, Notice>,
    wiggle: HashSet<u64>,
    /// avisos esperando o tremor terminar
    after_shake: Vec<u64>,
    shake_amp: f64,
    /// conta os tremores, para a garantia de tempo de um não encerrar o seguinte
    shake_gen: u64,
}

pub fn is_wayland() -> bool {
    cfg!(target_os = "linux")
        && std::env::var("XDG_SESSION_TYPE")
            .map(|s| s.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}

struct Prefs {
    own: bool,
    shake: bool,
    intensity: String,
    sound: bool,
}

fn prefs(app: &AppHandle) -> Prefs {
    let st = app.state::<AppState>();
    let db = st.db.lock().unwrap();
    let s = |k: &str, d: &str| db.setting(k).unwrap_or_else(|| d.to_string());
    Prefs {
        own: s("aviso_estilo", "proprio") != "sistema",
        shake: s("tremer_tela", "1") == "1",
        intensity: s("tremor_intensidade", "medio"),
        sound: s("som_aviso", "1") == "1",
    }
}

/// Mostra um aviso conforme as Configurações → Lembretes e avisos.
pub fn notify(app: &AppHandle, notice: Notice) {
    let p = prefs(app);
    if notice.push {
        let (a, t, m) = (app.clone(), notice.title.clone(), notice.message.clone());
        std::thread::spawn(move || ntfy(&a, &t, &m));
    }
    if p.own {
        if p.sound {
            play_sound();
        }
        let id = register(app, notice);
        if p.shake {
            match shake_screen(app, &p.intensity, Some(id)) {
                Ok(()) => return, // o aviso entra quando o tremor acabar
                Err(e) => {
                    log(format!(
                        "tremor da tela indisponível ({e}); tremendo só o aviso"
                    ));
                    app.state::<AppState>()
                        .toasts
                        .lock()
                        .unwrap()
                        .wiggle
                        .insert(id);
                }
            }
        }
        open_toast(app, id);
    } else {
        if p.shake {
            let _ = shake_screen(app, &p.intensity, None);
        }
        use tauri_plugin_notification::NotificationExt;
        if let Err(e) = app
            .notification()
            .builder()
            .title(&notice.title)
            .body(&notice.message)
            .show()
        {
            log(format!("aviso do sistema: {e}"));
        }
    }
}

fn register(app: &AppHandle, notice: Notice) -> u64 {
    let st = app.state::<AppState>();
    let mut t = st.toasts.lock().unwrap();
    t.next += 1;
    let id = t.next;
    t.notices.insert(id, notice);
    id
}

// ------------------------------------------------------------------ janela do aviso

fn open_toast(app: &AppHandle, id: u64) {
    let label = format!("toast-{id}");
    let built = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("toast.html?id={id}").into()),
    )
    .title("Finanças")
    .inner_size(TOAST_W, TOAST_H_GUESS)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false) // não rouba o foco de quem está digitando
    .visible(true)
    .build();
    let w = match built {
        Ok(w) => w,
        Err(e) => return log(format!("aviso: {e}")),
    };
    let sf = w.scale_factor().unwrap_or(1.0);
    let _ = w.set_size(PhysicalSize::new(
        (TOAST_W * sf) as u32,
        (TOAST_H_GUESS * sf) as u32,
    ));
    app.state::<AppState>()
        .toasts
        .lock()
        .unwrap()
        .stack
        .push((id, (TOAST_H_GUESS * sf) as i32));
    restack(app);
}

/// Empilha os avisos: no Windows de baixo para cima (perto do relógio); no Ubuntu de cima para baixo.
fn restack(app: &AppHandle) {
    let Some((_l, t, r, b, sf)) = work_area(app, None) else {
        return;
    };
    let stack = app.state::<AppState>().toasts.lock().unwrap().stack.clone();
    let w = (TOAST_W * sf) as i32;
    let mut y = if cfg!(windows) { b } else { t };
    for (id, h) in stack {
        let Some(win) = app.get_webview_window(&format!("toast-{id}")) else {
            continue;
        };
        let top = if cfg!(windows) {
            y -= h;
            y
        } else {
            let top = y;
            y += h;
            top
        };
        let _ = win.set_position(PhysicalPosition::new(r - w, top));
    }
}

fn close_toast(app: &AppHandle, id: u64) -> Option<Notice> {
    let notice = {
        let st = app.state::<AppState>();
        let mut t = st.toasts.lock().unwrap();
        t.stack.retain(|(i, _)| *i != id);
        t.wiggle.remove(&id);
        t.notices.remove(&id)
    };
    if let Some(w) = app.get_webview_window(&format!("toast-{id}")) {
        let _ = w.close();
    }
    restack(app);
    notice
}

#[tauri::command]
pub async fn toast_payload(app: AppHandle, id: u64) -> Result<Value, String> {
    let st = app.state::<AppState>();
    let t = st.toasts.lock().unwrap();
    let n = t.notices.get(&id).ok_or("aviso não encontrado")?;
    Ok(json!({
        "id": id,
        "title": n.title,
        "message": n.message,
        "kind": n.kind,
        "actions": n.actions.iter().map(|a| a.label.clone()).collect::<Vec<_>>(),
        "wiggle": t.wiggle.contains(&id),
        "seconds": 14,
    }))
}

/// O aviso já desenhou e mediu a própria altura (px de CSS).
#[tauri::command]
pub async fn toast_ready(app: AppHandle, id: u64, height: f64) -> Result<(), String> {
    let w = app
        .get_webview_window(&format!("toast-{id}"))
        .ok_or("sem janela")?;
    let sf = w.scale_factor().unwrap_or(1.0);
    let h = (height * sf).round() as i32;
    w.set_size(PhysicalSize::new((TOAST_W * sf) as u32, h as u32))
        .map_err(|e| e.to_string())?;
    {
        let st = app.state::<AppState>();
        let mut t = st.toasts.lock().unwrap();
        if let Some(slot) = t.stack.iter_mut().find(|(i, _)| *i == id) {
            slot.1 = h;
        }
    }
    restack(&app);
    Ok(())
}

#[tauri::command]
pub async fn toast_close(app: AppHandle, id: u64) {
    close_toast(&app, id);
}

/// Botão do aviso: "" abre o app, "novo" um lançamento, "snooze" lembra de novo em 60 min.
#[tauri::command]
pub async fn toast_action(app: AppHandle, id: u64, index: usize) {
    let Some(notice) = close_toast(&app, id) else {
        return;
    };
    let Some(route) = notice.actions.get(index).and_then(|a| a.route.clone()) else {
        return;
    };
    if route == "snooze" {
        let mut again = notice.clone();
        again.push = false;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(SNOOZE_MIN * 60));
            notify(&app, again);
        });
    } else {
        open_app(&app, &route);
    }
}

// ------------------------------------------------------------------ tremor da tela

/// Tira uma foto da tela, cobre a tela com ela e sacode por ~0,5 s.
/// Falha (e o aviso treme sozinho) se não der para fotografar a tela, como no Wayland.
fn shake_screen(app: &AppHandle, intensity: &str, then: Option<u64>) -> Result<(), String> {
    if is_wayland() {
        return Err("Wayland não permite fotografar a tela".into());
    }
    let st = app.state::<AppState>();
    if app.get_webview_window("shake").is_some() {
        // já está tremendo: este aviso entra junto com o outro
        st.toasts.lock().unwrap().after_shake.extend(then);
        return Ok(());
    }
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let mon = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or("nenhum monitor")?;
    let (x, y) = (
        mon.x().map_err(|e| e.to_string())?,
        mon.y().map_err(|e| e.to_string())?,
    );
    let img = mon.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    let rgb = image::DynamicImage::ImageRgba8(img).into_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    #[cfg(debug_assertions)]
    log(format!("tremor: foto {}x{} pronta", w, h));
    *st.shot.lock().unwrap() = Some(jpeg);
    let gen = {
        let mut t = st.toasts.lock().unwrap();
        t.shake_gen += 1;
        t.after_shake.extend(then);
        t.shake_amp = match intensity {
            "suave" => 7.0,
            "forte" => 24.0,
            _ => 14.0,
        };
        t.shake_gen
    };
    let win = WebviewWindowBuilder::new(app, "shake", WebviewUrl::App("shake.html".into()))
        .title("Finanças")
        .decorations(false)
        .transparent(true) // fica invisível até a foto carregar: nada pisca
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .focusable(false)
        .visible(true)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.set_size(PhysicalSize::new(w, h));
    // garantia: se a página não responder, encerra o tremor e mostra os avisos
    let a = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        if a.state::<AppState>().toasts.lock().unwrap().shake_gen == gen {
            finish_shake(&a);
        }
    });
    Ok(())
}

fn finish_shake(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("shake") {
        let _ = w.close();
    }
    let st = app.state::<AppState>();
    *st.shot.lock().unwrap() = None;
    let pending: Vec<u64> = std::mem::take(&mut st.toasts.lock().unwrap().after_shake);
    for id in pending {
        open_toast(app, id);
    }
}

#[tauri::command]
pub async fn shake_info(app: AppHandle) -> Value {
    let amp = app.state::<AppState>().toasts.lock().unwrap().shake_amp;
    json!({ "amp": amp })
}

#[tauri::command]
pub async fn shake_done(app: AppHandle) {
    #[cfg(debug_assertions)]
    log("tremor: fim (página)");
    finish_shake(&app);
}

// ------------------------------------------------------------------ som e celular

pub fn play_sound() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
        let name: Vec<u16> = "SystemNotification\0".encode_utf16().collect();
        PlaySoundW(
            name.as_ptr(),
            std::ptr::null_mut(),
            SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
        );
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        let tries: [&[&str]; 2] = [
            &["canberra-gtk-play", "-i", "message-new-instant"],
            &[
                "paplay",
                "/usr/share/sounds/freedesktop/stereo/message-new-instant.oga",
            ],
        ];
        for cmd in tries {
            if Command::new(cmd[0])
                .args(&cmd[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return;
            }
        }
    }
}

/// Push no celular: publica no ntfy (formato JSON, que aceita acentos no título).
fn ntfy(app: &AppHandle, title: &str, message: &str) {
    let (topic, server) = {
        let st = app.state::<AppState>();
        let db = st.db.lock().unwrap();
        (
            db.setting("ntfy_topico")
                .unwrap_or_default()
                .trim()
                .to_string(),
            db.setting("ntfy_servidor")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://ntfy.sh".into()),
        )
    };
    if topic.is_empty() {
        return;
    }
    let body = json!({ "topic": topic, "title": title, "message": message, "tags": ["moneybag"], "priority": 3 });
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into();
    let url = format!("{}/", server.trim_end_matches('/'));
    if let Err(e) = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(body.to_string())
    {
        log(format!("ntfy falhou: {e}"));
    }
}

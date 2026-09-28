//! Ectodeck's desktop shell: the windows, the tray, and the calls the
//! screens make, answered by the EVGA Z10 RGB core.
//!
//! Every call that talks to the keyboard runs off the interface thread, since
//! a single exchange with the Z10 waits on the keyboard for up to 150 ms.

use std::path::PathBuf;
use std::sync::Arc;

use ectotype_core::{Conn, Keyboard, Recorder};
use serde_json::{Value, json};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

struct App {
    kb: Arc<Keyboard>,
    rec: Recorder,
}

/// Run keyboard work on a blocking thread and hand back what it produced,
/// with any error as the plain sentence the screens show.
async fn blocking<T: Send + 'static>(what: &'static str, f: impl FnOnce() -> ectotype_core::Result<T> + Send + 'static) -> Result<T, String> {
    let t = std::time::Instant::now();
    let r = tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string());
    match &r {
        Ok(_) => eprintln!("ectodeck: {what} took {} ms", t.elapsed().as_millis()),
        Err(e) => eprintln!("ectodeck: {what} failed: {e}"),
    }
    r
}

#[tauri::command]
async fn state(app: State<'_, App>) -> Result<Value, String> {
    let kb = app.kb.clone();
    blocking("state", move || kb.state()).await
}

#[tauri::command]
async fn save(app: State<'_, App>, n: u8, body: Value) -> Result<Value, String> {
    let kb = app.kb.clone();
    blocking("save", move || kb.save(n, &body)).await
}

#[tauri::command]
async fn write_macro(app: State<'_, App>, index: u8, body: Value) -> Result<Value, String> {
    let kb = app.kb.clone();
    blocking("write_macro", move || kb.write_macro(index, &body)).await
}

#[tauri::command]
async fn activate(app: State<'_, App>, profile: u8) -> Result<Value, String> {
    let kb = app.kb.clone();
    blocking("activate", move || kb.activate(profile).map(|n| json!({ "active": n }))).await
}

#[tauri::command]
async fn preview(app: State<'_, App>, body: Value) -> Result<Value, String> {
    let kb = app.kb.clone();
    blocking("preview", move || kb.preview(&body)).await
}

/// `action` is start, stop, cancel or status.
#[tauri::command]
fn record(app: State<'_, App>, action: String, room: Option<usize>, waits: Option<bool>) -> Result<Value, String> {
    match action.as_str() {
        "start" => app.rec.start(room.unwrap_or(967), waits.unwrap_or(true)).map_err(|e| e.to_string()),
        "stop" => Ok(app.rec.stop(true)),
        "cancel" => Ok(app.rec.stop(false)),
        _ => Ok(app.rec.status()),
    }
}

/// What the keyboard's display is showing. The keyboard will not read its
/// display back, so this is the frame the z10-lcd service last wrote, until
/// the display's stats move into Ectodeck itself.
#[tauri::command]
fn lcd() -> Value {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let f = dir.join("z10-lcd/frame");
    let fresh = std::fs::metadata(&f).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() < 10);
    let rows: Vec<String> = std::fs::read_to_string(&f)
        .map(|s| s.split('\n').take(4).map(|r| format!("{:<20}", r.chars().take(20).collect::<String>())).collect())
        .unwrap_or_default();
    json!({ "live": fresh && !rows.is_empty(), "rows": rows })
}

/// Open the Macro Editor window, or bring it forward, on macro `index` if given.
/// Where both sliders are. Null on firmware that cannot say. Asked for
/// several times a second while a device page is open, so unlike the rest it
/// says nothing in the log unless it fails.
#[tauri::command]
async fn sliders(app: State<'_, App>) -> Result<Value, String> {
    let kb = app.kb.clone();
    tauri::async_runtime::spawn_blocking(move || kb.sliders())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Hold the dimmer at a level, or with none give the slider it back.
#[tauri::command]
async fn set_dimmer(app: State<'_, App>, level: Option<u8>) -> Result<bool, String> {
    let kb = app.kb.clone();
    tauri::async_runtime::spawn_blocking(move || kb.set_dimmer(level))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Ask the keyboard to stop doing a slider's job by itself.
#[tauri::command]
async fn set_modes(app: State<'_, App>, quiet_volume: bool, keep_dimmer: bool) -> Result<bool, String> {
    let kb = app.kb.clone();
    tauri::async_runtime::spawn_blocking(move || kb.set_modes(quiet_volume, keep_dimmer))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Set the computer's output or microphone volume, 0 to 1, through
/// PipeWire's own control. This is how a slider put on either one sets it
/// exactly where the slider is, instead of sending volume keys.
#[tauri::command]
fn set_audio(target: String, level: f32) -> Result<(), String> {
    let node = match target.as_str() {
        "output" => "@DEFAULT_AUDIO_SINK@",
        "microphone" => "@DEFAULT_AUDIO_SOURCE@",
        _ => return Err(format!("no audio target called {target}")),
    };
    let level = level.clamp(0.0, 1.0);
    let ok = std::process::Command::new("wpctl")
        .args(["set-volume", node, &format!("{level:.3}")])
        .status()
        .map_err(|e| format!("could not run wpctl: {e}"))?
        .success();
    if ok { Ok(()) } else { Err("wpctl refused the volume".into()) }
}

/// Retail or custom firmware, and what the keyboard's build can do (see
/// ectotype_core::firmware). The app offers a custom feature only if listed.
#[tauri::command]
fn firmware(version: u16) -> Value {
    ectotype_core::firmware::describe(version)
}

#[tauri::command]
fn open_macro_editor(app: AppHandle, index: Option<u8>) -> Result<(), String> {
    show_macro_editor(&app, index).map_err(|e| e.to_string())
}

/// Build the Macro Editor window out of sight, once the main window has read
/// the keyboard, so it has read the keyboard too by the time it is opened;
/// built only when first opened, it showed nothing for the seconds that takes.
#[tauri::command]
fn prepare_macro_editor(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("macros").is_some() {
        return Ok(());
    }
    macro_window(&app, "macros".into(), false).map(|_| ()).map_err(|e| e.to_string())
}

fn macro_window(app: &AppHandle, hash: String, visible: bool) -> tauri::Result<tauri::WebviewWindow> {
    WebviewWindowBuilder::new(app, "macros", WebviewUrl::App(format!("index.html#{hash}").into()))
        .title("EVGA Z10 RGB / Macro Editor")
        .inner_size(1080.0, 620.0)
        .decorations(false)
        .transparent(true)
        .min_inner_size(900.0, 420.0)
        .visible(visible)
        .build()
}

fn show_macro_editor(app: &AppHandle, index: Option<u8>) -> tauri::Result<()> {
    let hash = match index { Some(i) => format!("macros/{i}"), None => "macros".into() };
    if let Some(w) = app.get_webview_window("macros") {
        w.eval(&format!("location.hash = '{hash}'"))?;
        w.show()?;
        w.unminimize()?;
        return w.set_focus();
    }
    macro_window(app, hash, true)?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The tray belongs to Ectodeck, not to any one device, so it carries nothing
/// that is a single device's own. The Z10's Macro Editor opens from the Z10.
fn tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Ectodeck", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Ectodeck", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;
    TrayIconBuilder::with_id("ectodeck")
        .icon(app.default_window_icon().cloned().expect("the app's icon"))
        .tooltip("Ectodeck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|icon, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(icon.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    // As Ectodeck's MagTran app does on Linux: WebKitGTK's DMA-BUF renderer
    // fails on NVIDIA under Wayland (the window dies with a protocol error),
    // and without compositing mode off the frameless window cannot be
    // see-through at its corners, so it shows square instead of rounded.
    // Either can still be set by the user. Nothing else runs yet, so setting
    // the environment here is safe.
    #[cfg(target_os = "linux")]
    for var in ["WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DISABLE_COMPOSITING_MODE"] {
        if std::env::var_os(var).is_none() {
            unsafe { std::env::set_var(var, "1") };
        }
    }

    tauri::Builder::default()
        .setup(|app| {
            let backups = app.path().app_data_dir()?.join("backups");
            app.manage(App { kb: Arc::new(Keyboard::new(Conn::hid(), Some(backups))), rec: Recorder::new() });
            tray(app.handle())?;
            Ok(())
        })
        // Closing a window keeps Ectodeck running in the tray; Quit in the
        // tray menu ends it.
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![state, save, write_macro, activate, preview, record, lcd, open_macro_editor, prepare_macro_editor, firmware, sliders, set_dimmer, set_audio, set_modes])
        .run(tauri::generate_context!())
        .expect("Ectodeck could not start");
}

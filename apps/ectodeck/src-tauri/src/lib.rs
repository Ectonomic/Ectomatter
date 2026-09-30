//! Ectodeck's desktop shell: the windows, the tray, and the calls the
//! screens make, answered by the EVGA Z10 RGB core.
//!
//! Every call that talks to the keyboard runs off the interface thread, since
//! a single exchange with the Z10 waits on the keyboard for up to 150 ms.

mod devices;
mod screens;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;

use ectotype_core::{Conn, Keyboard, Recorder};
use serde_json::{Value, json};
use tauri::Emitter;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

struct App {
    kb: Arc<Keyboard>,
    rec: Recorder,
    settings: Arc<settings::Settings>,
    /// One weather feed for every device, for the place in Settings.
    weather: ectodeck_weather::Feed,
}

/// The place Settings names for the weather, as saved: `weather.place`
/// holds a place as the search gave it (see ectodeck_weather::Place).
fn weather_place(settings: &settings::Settings) -> Option<ectodeck_weather::At> {
    let p = settings.get("weather.place")?;
    Some(ectodeck_weather::At {
        lat: p["lat"].as_f64()? as f32,
        lon: p["lon"].as_f64()? as f32,
        country: p["country_code"].as_str().unwrap_or("").to_string(),
    })
}

/// The computer's locale, such as "en-US", for the defaults it implies.
fn locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en-US".into())
}

/// What a screen is told when Ectodeck has been set not to control the
/// keyboard, or one part of it. The screens look for the first sentence.
const NOT_CONTROLLED: &str = "Ectodeck is set not to control this keyboard.";

/// Whether Ectodeck may talk to the keyboard at all (Settings › Devices › Control).
fn allowed(app: &App) -> Result<(), String> {
    if !app.settings.on("devices.z10.control") {
        return Err(NOT_CONTROLLED.into());
    }
    Ok(())
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
    allowed(&app)?;
    let kb = app.kb.clone();
    blocking("state", move || kb.state()).await
}

#[tauri::command]
async fn save(app: State<'_, App>, n: u8, body: Value) -> Result<Value, String> {
    allowed(&app)?;
    let kb = app.kb.clone();
    blocking("save", move || kb.save(n, &body)).await
}

#[tauri::command]
async fn write_macro(app: State<'_, App>, index: u8, body: Value) -> Result<Value, String> {
    allowed(&app)?;
    let kb = app.kb.clone();
    blocking("write_macro", move || kb.write_macro(index, &body)).await
}

#[tauri::command]
async fn activate(app: State<'_, App>, profile: u8) -> Result<Value, String> {
    allowed(&app)?;
    let kb = app.kb.clone();
    blocking("activate", move || kb.activate(profile).map(|n| json!({ "active": n }))).await
}

#[tauri::command]
async fn preview(app: State<'_, App>, body: Value) -> Result<Value, String> {
    if !body.get("stop").and_then(Value::as_bool).unwrap_or(false) {
        allowed(&app)?;
    }
    let kb = app.kb.clone();
    blocking("preview", move || kb.preview(&body)).await
}

/// `action` is start, stop, cancel or status.
#[tauri::command]
fn record(app: State<'_, App>, action: String, room: Option<usize>, waits: Option<bool>) -> Result<Value, String> {
    match action.as_str() {
        "start" => allowed(&app).and_then(|_| app.rec.start(room.unwrap_or(967), waits.unwrap_or(true)).map_err(|e| e.to_string())),
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
    allowed(&app)?;
    let kb = app.kb.clone();
    tauri::async_runtime::spawn_blocking(move || kb.sliders())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Hold the dimmer at a level, or with none give the slider it back.
#[tauri::command]
async fn set_dimmer(app: State<'_, App>, level: Option<u8>) -> Result<bool, String> {
    allowed(&app)?;
    let kb = app.kb.clone();
    tauri::async_runtime::spawn_blocking(move || kb.set_dimmer(level))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Ask the keyboard to stop doing a slider's job by itself.
#[tauri::command]
async fn set_modes(app: State<'_, App>, quiet_volume: bool, keep_dimmer: bool) -> Result<bool, String> {
    allowed(&app)?;
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

/// Everything in Ectodeck's own settings.
#[tauri::command]
fn settings_get(app: State<'_, App>) -> Value {
    app.settings.all()
}

/// Set one of Ectodeck's own settings. Turning control of the keyboard, or of
/// its sliders, off first gives the keyboard back what Ectodeck had borrowed,
/// and turning control off altogether closes the keyboard, so nothing of
/// Ectodeck's has it open.
#[tauri::command]
async fn settings_set(app: State<'_, App>, path: String, value: Value) -> Result<Value, String> {
    let off = value == Value::Bool(false);
    let kb = app.kb.clone();
    if off && path == "devices.z10.control" {
        let k = kb.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let _ = k.set_modes(false, false);
            let _ = k.set_dimmer(None);
        }).await;
    }
    if off && path == "devices.z10.control" {
        let k = kb.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || k.preview(&json!({ "stop": true }))).await;
    }
    app.settings.set(&path, value).map_err(|e| format!("could not save the setting: {e}"))?;
    if path.starts_with("weather") {
        app.weather.set_place(weather_place(&app.settings));
    }
    if off && path == "devices.z10.control" {
        kb.release();
    }
    Ok(app.settings.all())
}

/// The weather now for the place in Settings, or no report while there is no
/// place or the first answer has not come.
#[tauri::command]
fn weather(app: State<'_, App>) -> Value {
    // units as chosen in Settings, or as the computer's locale implies
    let d = ectodeck_weather::units::units_for(&locale());
    let units = json!({
        "temperature": app.settings.get("weather.units.temperature").unwrap_or(json!(d.temperature)),
        "wind": app.settings.get("weather.units.wind").unwrap_or(json!(d.wind)),
    });
    let place = app.settings.get("weather.place");
    let country = place.as_ref().and_then(|p| p["country_code"].as_str()).unwrap_or("").to_string();
    json!({
        "place": place,
        "report": app.weather.report(),
        "units": units,
        "locale": locale(),
        // who issues warnings for the place, or null where they are read from the weather
        "warnings": ectodeck_weather::alerts::source(&country).map(|s| s.name()),
    })
}

/// Places matching a name, for choosing where the weather is for.
#[tauri::command]
async fn weather_search(query: String) -> Result<Value, String> {
    let language = ectodeck_weather::units::language(&locale());
    tauri::async_runtime::spawn_blocking(move || ectodeck_weather::search(&query, &language))
        .await
        .map_err(|e| e.to_string())?
        .map(|p| json!(p))
}

/// Each screen: whether it is in HDR, its reference white and its peak.
#[tauri::command]
async fn screens() -> Value {
    json!(tauri::async_runtime::spawn_blocking(screens::screens).await.unwrap_or_default())
}

/// What Ectodeck can see of each device without opening it.
#[tauri::command]
fn devices_status(app: State<'_, App>) -> Value {
    let mut z10 = devices::z10();
    z10["id"] = json!("z10");
    z10["control"] = json!(app.settings.on("devices.z10.control"));
    json!([z10])
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

/// A device in the tray's list, which sits under Ectodeck itself: its page is
/// one click away, and one that is not plugged in is greyed rather than
/// dropped, so the list keeps still.
#[derive(serde::Deserialize)]
pub struct TrayDevice {
    id: String,
    name: String,
    connected: bool,
}

/// The tray belongs to Ectodeck, not to any one device, so it carries the
/// devices themselves and nothing that is a single device's own: the Z10's
/// Macro Editor opens from the Z10. Called again whenever the list or a
/// device's connection changes.
#[tauri::command]
fn set_tray_devices(app: AppHandle, devices: Vec<TrayDevice>) -> Result<(), String> {
    let tray = app.tray_by_id("ectodeck").ok_or("no tray")?;
    tray.set_menu(Some(tray_menu(&app, &devices).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())
}

fn tray_menu(app: &AppHandle, devices: &[TrayDevice]) -> tauri::Result<Menu<tauri::Wry>> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    items.push(Box::new(MenuItem::with_id(app, "open", "Open Ectodeck", true, None::<&str>)?));
    for d in devices {
        let label = if d.connected { d.name.clone() } else { format!("{} (not connected)", d.name) };
        items.push(Box::new(MenuItem::with_id(app, format!("device:{}", d.id), label, d.connected, None::<&str>)?));
    }
    items.push(Box::new(MenuItem::with_id(app, "quit", "Quit Ectodeck", true, None::<&str>)?));
    Menu::with_items(app, &items.iter().map(|i| i.as_ref()).collect::<Vec<_>>())
}

fn tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app, &[])?;
    TrayIconBuilder::with_id("ectodeck")
        .icon(app.default_window_icon().cloned().expect("the app's icon"))
        .tooltip("Ectodeck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            id => {
                if let Some(device) = id.strip_prefix("device:") {
                    show_main(app);
                    let _ = app.emit("device", device.to_string());
                }
            }
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
            let settings = Arc::new(settings::Settings::load(app.path().app_config_dir()?.join("settings.json")));
            let weather = ectodeck_weather::Feed::start();
            weather.set_place(weather_place(&settings));
            app.manage(App { kb: Arc::new(Keyboard::new(Conn::hid(), Some(backups))), rec: Recorder::new(), settings, weather });
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
        .invoke_handler(tauri::generate_handler![state, save, write_macro, activate, preview, record, lcd, open_macro_editor, prepare_macro_editor, firmware, sliders, set_dimmer, set_audio, set_modes, set_tray_devices, settings_get, settings_set, devices_status, weather, weather_search, screens])
        .run(tauri::generate_context!())
        .expect("Ectodeck could not start");
}

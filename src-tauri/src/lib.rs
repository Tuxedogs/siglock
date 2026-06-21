use base64::engine::general_purpose;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{Emitter, LogicalPosition, Manager, PhysicalPosition, Position, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::time::sleep;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EnumWindows, GetClassNameW, GetMessageW,
    GetWindowThreadProcessId, SetLayeredWindowAttributes, SetWindowsHookExW, TranslateMessage,
    LWA_ALPHA, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_MBUTTONDOWN, WM_XBUTTONDOWN,
};

#[cfg(windows)]
static MOUSE_HOOK_APP: OnceLock<tauri::AppHandle> = OnceLock::new();
#[cfg(windows)]
static SCAN_MOUSE_BUTTON: AtomicU32 = AtomicU32::new(0);
#[cfg(windows)]
static AUTO_TOGGLE_MOUSE_BUTTON: AtomicU32 = AtomicU32::new(0);
static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_DUPLICATE_LOGGED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Material signature data (base RS per rock/node)
#[derive(Serialize, Clone, Debug)]
pub struct Material {
    pub name: &'static str,
    pub base: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<&'static str>,
}

/// A single match result for an observed signature
#[derive(Serialize, Clone, Debug)]
pub struct MatchResult {
    pub material: String,
    pub estimated_rocks: u32,
    pub predicted_signature: u32,
    pub diff: i32,
    pub confidence: f32,
}

/// OCR scan result from adapter (mock or real)
#[derive(Serialize, Clone, Debug)]
pub struct OcrScanResult {
    pub raw_text: String,
    pub normalized_value: Option<u32>,
    pub confidence: Option<f32>,
    pub scanned_at: String,
    pub error: Option<String>,
    // Debug fields for tuning
    pub raw_crop_path: Option<String>,
    pub preprocessed_path: Option<String>,
    pub capture_width: Option<u32>,
    pub capture_height: Option<u32>,
}

/// Configuration for preprocessing and Tesseract (dev tunable)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OcrConfig {
    pub upscale: u32, // 1, 2, 3, 4
    pub threshold_enabled: bool,
    pub threshold: u8, // 0-255
    pub grayscale: bool,
    pub invert: bool,
    pub sharpen: bool,
    pub psm: u8,            // 7, 8, 13 etc.
    pub numeric_only: bool, // apply whitelist
}

// ==================== Release-friendly Path & Engine Resolution Helpers ====================
//
// These helpers exist to keep the app future-proof for packaging (portable .exe + installer).
//
// Rules followed:
// - No hard-coded user-specific paths.
// - Tesseract resolution is centralized so it can later check:
//     1. Bundled app-local path (resources/tesseract/)
//     2. User-configured custom path (from store)
//     3. System PATH fallback
// - OCR working files use the system temporary directory and are removed after use.

/// Returns the base app data directory using Tauri's proper resolver.
/// This is the correct way for both development and packaged apps.
fn get_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))
}

fn get_native_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(get_app_data_dir(app)?.join("native-settings.json"))
}

fn log_window_lifecycle(app: &tauri::AppHandle, label: &str, action: &str, context: &str) {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let line = format!(
        "{} label={} action={} context={}\n",
        timestamp, label, action, context
    );
    if let Ok(path) = get_app_data_dir(app).map(|dir| dir.join("window-lifecycle.log")) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
    print!("[WindowLifecycle] {}", line);
}

fn background_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(windows)]
unsafe extern "system" fn suppress_tao_event_target(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let mut process_id = 0;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    if process_id != std::process::id() {
        return 1;
    }

    let mut class_name = [0u16; 64];
    let length = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
    if length > 0
        && String::from_utf16_lossy(&class_name[..length as usize]) == "Tao Thread Event Target"
    {
        SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
        *(lparam as *mut bool) = true;
    }
    1
}

#[cfg(windows)]
fn suppress_tao_event_target_artifact(app: &tauri::AppHandle) {
    let mut found = false;
    unsafe {
        EnumWindows(
            Some(suppress_tao_event_target),
            &mut found as *mut bool as LPARAM,
        );
    }
    if found {
        log_window_lifecycle(app, "tao_event_target", "set_alpha_zero", "startup");
    }
}

#[cfg(not(windows))]
fn suppress_tao_event_target_artifact(_app: &tauri::AppHandle) {}

/// Resolves the Tesseract executable path by trying candidates in priority order
/// and returning the first one that successfully responds to `--version`.
///
/// Search order:
/// 1. Bundled/app-local Tesseract (resources/tesseract/tesseract.exe when packaged)
/// 2. C:\Program Files\Tesseract-OCR\tesseract.exe
/// 3. C:\Program Files (x86)\Tesseract-OCR\tesseract.exe
/// 4. System PATH ("tesseract")
fn resolve_tesseract_executable(app: Option<&tauri::AppHandle>) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. Bundled / app-local path (used after proper packaging)
    if let Some(app_handle) = app {
        if let Ok(resource_dir) = app_handle.path().resource_dir() {
            let bundled = resource_dir.join("tesseract").join("tesseract.exe");
            candidates.push(bundled);
        }
    }

    // 2. Common 64-bit Windows install location
    candidates.push(PathBuf::from(
        r"C:\Program Files\Tesseract-OCR\tesseract.exe",
    ));

    // 3. Common 32-bit Windows install location
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
    ));

    // 4. System PATH fallback
    candidates.push(PathBuf::from("tesseract"));

    for candidate in &candidates {
        // For PATH entry ("tesseract"), we don't check existence — just try to run it.
        // For explicit paths, we can skip if the file doesn't exist to avoid noise.
        if candidate == &PathBuf::from("tesseract") || candidate.exists() {
            if let Ok(output) = background_command(candidate).arg("--version").output() {
                if output.status.success() {
                    println!("[SigLock] Using Tesseract at: {}", candidate.display());
                    return candidate.clone();
                }
            }
        }
    }

    // Final fallback (will likely fail at runtime, which is handled gracefully)
    PathBuf::from("tesseract")
}

/// Result of capturing a screen region for preview/debug
#[derive(Serialize, Clone, Debug)]
pub struct CaptureResult {
    pub success: bool,
    pub width: u32,
    pub height: u32,
    pub image_path: Option<String>,
    pub captured_at: String,
    pub error: Option<String>,
    // Base64 data URL for reliable preview in dev (avoids asset protocol / scope issues)
    pub preview_data_url: Option<String>,
}

/// Hard-coded signature index (26 materials)
static MATERIALS: &[Material] = &[
    Material {
        name: "Quantanium",
        base: 3170,
        category: Some("High value"),
    },
    Material {
        name: "Stileron",
        base: 3185,
        category: None,
    },
    Material {
        name: "Savrilium",
        base: 3200,
        category: None,
    },
    Material {
        name: "Ouratite",
        base: 3370,
        category: None,
    },
    Material {
        name: "Riccite",
        base: 3385,
        category: None,
    },
    Material {
        name: "Lindinium",
        base: 3400,
        category: None,
    },
    Material {
        name: "Beryl",
        base: 3540,
        category: None,
    },
    Material {
        name: "Taranite",
        base: 3555,
        category: None,
    },
    Material {
        name: "Borase",
        base: 3570,
        category: None,
    },
    Material {
        name: "Gold",
        base: 3585,
        category: None,
    },
    Material {
        name: "Bexalite",
        base: 3600,
        category: None,
    },
    Material {
        name: "Laranite",
        base: 3825,
        category: None,
    },
    Material {
        name: "Aslarite",
        base: 3840,
        category: None,
    },
    Material {
        name: "Titanium",
        base: 3855,
        category: None,
    },
    Material {
        name: "Tungsten",
        base: 3870,
        category: None,
    },
    Material {
        name: "Agricium",
        base: 3885,
        category: None,
    },
    Material {
        name: "Torite",
        base: 3900,
        category: None,
    },
    Material {
        name: "Hephestanite",
        base: 4180,
        category: None,
    },
    Material {
        name: "Tin",
        base: 4195,
        category: None,
    },
    Material {
        name: "Quartz",
        base: 4210,
        category: None,
    },
    Material {
        name: "Corundum",
        base: 4225,
        category: None,
    },
    Material {
        name: "Copper",
        base: 4240,
        category: None,
    },
    Material {
        name: "Silicon",
        base: 4255,
        category: None,
    },
    Material {
        name: "Iron",
        base: 4270,
        category: None,
    },
    Material {
        name: "Aluminium",
        base: 4285,
        category: None,
    },
    Material {
        name: "Ice",
        base: 4300,
        category: Some("Common"),
    },
];

/// Core matching logic (exact first, then ± tolerance)
#[tauri::command]
fn match_signature(observed: u32, tolerance: Option<i32>) -> Vec<MatchResult> {
    let tolerance = tolerance.unwrap_or(25);
    let mut results: Vec<MatchResult> = Vec::new();

    for mat in MATERIALS {
        if mat.base == 0 {
            continue;
        }

        let rocks_f = observed as f64 / mat.base as f64;
        let estimated_rocks = rocks_f.round().clamp(1.0, 20.0) as u32;
        let predicted = mat.base * estimated_rocks;
        let diff = observed as i32 - predicted as i32;
        let abs_diff = diff.abs();

        if abs_diff == 0 || abs_diff <= tolerance {
            let confidence = if abs_diff == 0 {
                1.0
            } else {
                let rel = abs_diff as f32 / (mat.base as f32 * 0.5);
                (1.0 - rel).max(0.1)
            };

            results.push(MatchResult {
                material: mat.name.to_string(),
                estimated_rocks,
                predicted_signature: predicted,
                diff,
                confidence: (confidence * 100.0).round() / 100.0,
            });
        }
    }

    results.sort_by_key(|r| (r.diff != 0, r.diff.abs()));
    results
}

// ==================== App State ====================

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ScanRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
struct AppStateInner {
    /// Whether Active Scan Mode is enabled by the user (persisted preference)
    active_scan_enabled: bool,
    /// Current visibility of the dedicated overlay window
    overlay_visible: bool,
    overlay_setup_mode: bool,
    /// Current OCR region (None = not set)
    region: Option<ScanRegion>,
    /// Last observed signature and source
    last_value: Option<u32>,
    last_source: Option<String>,
    /// Current hotkey configuration
    hotkey_show_hide: String,
    hotkey_single_scan: String,
    hotkey_toggle_active: String,
    /// Active scan interval in milliseconds (product range 1000-4000)
    scan_interval_ms: u64,
}

type AppState = Arc<Mutex<AppStateInner>>;

/// Controller for the single active scan timer task
struct ActiveScanController {
    stop_flag: Arc<AtomicBool>,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl Default for ActiveScanController {
    fn default() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

type ActiveScanControllerState = Arc<Mutex<ActiveScanController>>;

fn default_state() -> AppStateInner {
    AppStateInner {
        active_scan_enabled: false, // SAFETY: always start OFF
        overlay_visible: true,
        overlay_setup_mode: false,
        region: None,
        last_value: None,
        last_source: None,
        hotkey_show_hide: "Ctrl+Shift+M".to_string(),
        hotkey_single_scan: "Ctrl+Alt+F9".to_string(),
        hotkey_toggle_active: "Ctrl+Shift+S".to_string(),
        scan_interval_ms: 3000,
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
struct NativeSettings {
    region: Option<ScanRegion>,
    overlay_position: Option<(i32, i32)>,
}

fn load_native_settings(app: &tauri::AppHandle) -> NativeSettings {
    let Ok(path) = get_native_settings_path(app) else {
        return NativeSettings::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn save_native_settings(app: &tauri::AppHandle, settings: &NativeSettings) -> Result<(), String> {
    let path = get_native_settings_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let value = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, value).map_err(|e| e.to_string())
}

fn save_region(app: &tauri::AppHandle, region: Option<ScanRegion>) -> Result<(), String> {
    let mut settings = load_native_settings(app);
    settings.region = region;
    save_native_settings(app, &settings)
}

fn save_overlay_position(
    app: &tauri::AppHandle,
    position: PhysicalPosition<i32>,
) -> Result<(), String> {
    let mut settings = load_native_settings(app);
    settings.overlay_position = Some((position.x, position.y));
    save_native_settings(app, &settings)
}

fn safe_overlay_position(
    window: &tauri::WebviewWindow,
    saved: Option<(i32, i32)>,
) -> PhysicalPosition<i32> {
    let fallback = PhysicalPosition::new(80, 120);
    let Some((x, y)) = saved else {
        return fallback;
    };

    let is_visible = window.available_monitors().ok().is_some_and(|monitors| {
        monitors.iter().any(|monitor| {
            let origin = monitor.position();
            let size = monitor.size();
            x < origin.x + size.width as i32
                && x + 340 > origin.x
                && y < origin.y + size.height as i32
                && y + 240 > origin.y
        })
    });

    if is_visible {
        PhysicalPosition::new(x, y)
    } else {
        fallback
    }
}

fn clamp_scan_interval(interval_ms: u64) -> u64 {
    interval_ms.clamp(1000, 4000)
}

fn scan_mouse_button(binding: Option<&str>) -> Result<u32, String> {
    match binding {
        None => Ok(0),
        Some("Middle Mouse") => Ok(1),
        Some("Mouse4") => Ok(4),
        Some("Mouse5") => Ok(5),
        Some(_) => Err("Only Middle Mouse, Mouse4, and Mouse5 are supported.".into()),
    }
}

fn request_shutdown_once(app: &tauri::AppHandle, source: &str) {
    if SHUTDOWN_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        if !SHUTDOWN_DUPLICATE_LOGGED.swap(true, Ordering::SeqCst) {
            log_window_lifecycle(app, "app", "shutdown_already_started", source);
        }
        return;
    }

    log_window_lifecycle(app, "app", "shutdown_start", source);
    if let Ok(mut state) = app.state::<AppState>().lock() {
        state.active_scan_enabled = false;
        state.overlay_setup_mode = false;
    }
    log_window_lifecycle(app, "active_scan", "stop", "shutdown");

    let controller: State<'_, ActiveScanControllerState> = app.state();
    stop_active_scan_timer(controller.inner().clone());
    log_window_lifecycle(app, "shortcuts", "unregister", "shutdown");

    let _ = app.global_shortcut().unregister_all();
    for (label, window) in app.webview_windows() {
        if label != "main" {
            log_window_lifecycle(app, &label, "close", "shutdown");
            let _ = window.close();
        }
    }

    log_window_lifecycle(app, "app", "exit", "shutdown");
    app.exit(0);
}

#[cfg(windows)]
fn mouse_button_from_message(message: u32, mouse_data: u32) -> u32 {
    match message {
        WM_MBUTTONDOWN => 1,
        WM_XBUTTONDOWN if (mouse_data >> 16) == 1 => 4,
        WM_XBUTTONDOWN => 5,
        _ => 0,
    }
}

// ==================== Commands ====================

#[tauri::command]
async fn toggle_overlay_visibility(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("overlay") {
        let currently_visible = window.is_visible().unwrap_or(false);

        if currently_visible {
            window.hide().map_err(|e| e.to_string())?;
            log_window_lifecycle(&app, "overlay", "hide", "overlay_update");
        } else {
            window.set_always_on_top(true).map_err(|e| e.to_string())?;
            window.show().map_err(|e| e.to_string())?;
            log_window_lifecycle(&app, "overlay", "show", "overlay_update");
        }
        let visible = !currently_visible;
        state.lock().unwrap().overlay_visible = visible;
        let _ = app.emit("overlay-visibility-changed", visible);
        Ok(visible)
    } else {
        Err("Overlay window not found".into())
    }
}

#[tauri::command]
fn minimize_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn request_app_shutdown(app: tauri::AppHandle) {
    request_shutdown_once(&app, "frontend-close");
}

#[tauri::command]
fn get_overlay_setup_mode(state: State<'_, AppState>) -> bool {
    state.lock().unwrap().overlay_setup_mode
}

#[tauri::command]
fn set_overlay_setup_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or_else(|| "Overlay window not found".to_string())?;
    if enabled {
        window.set_always_on_top(true).map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        log_window_lifecycle(&app, "overlay", "show", "overlay_update");
    }
    window.set_resizable(enabled).map_err(|e| e.to_string())?;
    window
        .set_ignore_cursor_events(!enabled)
        .map_err(|e| e.to_string())?;
    state.lock().unwrap().overlay_setup_mode = enabled;
    let _ = app.emit("overlay-setup-mode-changed", enabled);
    Ok(enabled)
}

#[tauri::command]
fn reset_overlay_position(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or_else(|| "Overlay window not found".to_string())?;
    window
        .set_position(Position::Logical(LogicalPosition::new(80.0, 120.0)))
        .map_err(|e| e.to_string())?;
    log_window_lifecycle(&app, "overlay", "set_position", "overlay_update");
    save_overlay_position(&app, window.outer_position().map_err(|e| e.to_string())?)
}

#[tauri::command]
fn set_scan_now_mouse_binding(binding: Option<String>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let button = scan_mouse_button(binding.as_deref())?;
        SCAN_MOUSE_BUTTON.store(button, Ordering::SeqCst);
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = binding;
        Err("Global mouse Scan Now bindings are not supported on this platform.".into())
    }
}

#[tauri::command]
fn set_auto_toggle_mouse_binding(binding: Option<String>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let button = scan_mouse_button(binding.as_deref())?;
        AUTO_TOGGLE_MOUSE_BUTTON.store(button, Ordering::SeqCst);
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = binding;
        Err("Global mouse Auto Scan bindings are not supported on this platform.".into())
    }
}

#[tauri::command]
async fn trigger_single_scan(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let has_region = {
        let s = state.lock().unwrap();
        s.region.is_some()
    };

    if has_region {
        let _ = app.emit("request-ocr-scan", ());
    } else {
        // Per spec: show + prompt to set region
        let _ = app.emit("no-region-prompt", ());
    }

    Ok(())
}

#[tauri::command]
async fn toggle_active_scan(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let (enabled, had_region, interval) = {
        let mut s = state.lock().unwrap();
        s.active_scan_enabled = !s.active_scan_enabled;
        (
            s.active_scan_enabled,
            s.region.is_some(),
            s.scan_interval_ms,
        )
    };

    let _ = app.emit("active-scan-toggled", enabled);

    if enabled {
        if had_region {
            start_active_scan_timer(app.clone(), state.inner().clone(), interval);
            println!(
                "[SigLock] Active Scan ENABLED → timer started ({}ms interval)",
                interval
            );
        } else {
            println!("[SigLock] Active Scan ENABLED but no region set — timer not started");
        }
    } else {
        let controller: State<'_, ActiveScanControllerState> = app.state();
        stop_active_scan_timer(controller.inner().clone());
        println!("[SigLock] Active Scan DISABLED → timer stopped");
    }

    Ok(enabled)
}

#[tauri::command]
async fn set_scan_interval(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    interval_ms: u64,
) -> Result<u64, String> {
    let safe_interval = clamp_scan_interval(interval_ms);
    let should_restart = {
        let mut s = state.lock().unwrap();
        s.scan_interval_ms = safe_interval;
        s.active_scan_enabled && s.region.is_some()
    };

    if should_restart {
        start_active_scan_timer(app, state.inner().clone(), safe_interval);
    }

    Ok(safe_interval)
}

#[tauri::command]
fn get_app_state(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let s = state.lock().unwrap();
    Ok(serde_json::json!({
        "active_scan_enabled": s.active_scan_enabled,
        "overlay_visible": s.overlay_visible,
        "overlay_setup_mode": s.overlay_setup_mode,
        "has_region": s.region.is_some(),
        "scan_interval_ms": s.scan_interval_ms,
        "hotkeys": {
            "show_hide": s.hotkey_show_hide,
            "single_scan": s.hotkey_single_scan,
            "toggle_active": s.hotkey_toggle_active,
        }
    }))
}

// ==================== Region Management (persisted) ====================

#[tauri::command]
async fn set_crop_region(
    region: ScanRegion,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.region = Some(region.clone());
    save_region(&app, Some(region))
}

#[tauri::command]
fn get_crop_region(state: State<'_, AppState>) -> Result<Option<ScanRegion>, String> {
    let s = state.lock().unwrap();
    Ok(s.region.clone())
}

#[tauri::command]
async fn clear_crop_region(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.region = None;
    save_region(&app, None)
}

#[tauri::command]
async fn check_tesseract(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let tesseract_path = resolve_tesseract_executable(Some(&app));

    let output = background_command(&tesseract_path)
        .arg("--version")
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}{}", stdout, stderr);

            let version = combined
                .lines()
                .find(|l| l.to_lowercase().contains("tesseract"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            Ok(serde_json::json!({
                "available": true,
                "version": version,
                "path": tesseract_path.to_string_lossy(),
            }))
        }
        Err(_e) => Ok(serde_json::json!({
            "available": false,
            "path_checked": tesseract_path.to_string_lossy(),
            "error": format!(
                "Tesseract not found at any known location (tried bundled, Program Files, and PATH). Last tried: {}",
                tesseract_path.display()
            )
        })),
    }
}

#[tauri::command]
async fn open_region_picker(app: tauri::AppHandle) -> Result<(), String> {
    if app.get_webview_window("region_picker").is_some() {
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "region_picker",
        tauri::WebviewUrl::App("/region-picker".into()),
    )
    .title("Select Scan Region")
    .inner_size(1920.0, 1080.0)
    .position(0.0, 0.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(true)
    .build()
    .map_err(|e| e.to_string())?;
    log_window_lifecycle(&app, "region_picker", "create", "set_region");
    log_window_lifecycle(&app, "region_picker", "show", "set_region");

    Ok(())
}

// ==================== OCR Adapter (Mock for now) ====================

#[tauri::command]
async fn scan_selected_region(
    state: State<'_, AppState>,
    config: Option<OcrConfig>,
    trigger: Option<String>,
    app: tauri::AppHandle,
) -> Result<OcrScanResult, String> {
    let context = if trigger.as_deref() == Some("Active") {
        "auto_scan"
    } else {
        "manual_scan"
    };
    log_window_lifecycle(&app, "none", "audit_no_window_action", context);
    let region = {
        let s = state.lock().unwrap();
        s.region.clone()
    };

    if region.is_none() {
        return Ok(OcrScanResult {
            raw_text: "".to_string(),
            normalized_value: None,
            confidence: None,
            scanned_at: chrono::Utc::now().to_rfc3339(),
            error: Some("No region selected. Click 'Set Region' first.".to_string()),
            raw_crop_path: None,
            preprocessed_path: None,
            capture_width: None,
            capture_height: None,
        });
    }

    let cfg = config.unwrap_or_default();
    perform_real_ocr_scan(region.unwrap(), cfg, &app)
}

fn mock_ocr_scan(test_value: u32) -> Result<OcrScanResult, String> {
    let raw = test_value.to_string();
    Ok(OcrScanResult {
        raw_text: raw.clone(),
        normalized_value: Some(test_value),
        confidence: Some(0.95),
        scanned_at: chrono::Utc::now().to_rfc3339(),
        error: None,
        raw_crop_path: None,
        preprocessed_path: None,
        capture_width: None,
        capture_height: None,
    })
}

/// Preprocess the captured crop for better OCR on HUD text.
/// Now configurable for tuning.
fn preprocess_for_ocr(img: image::DynamicImage, config: &OcrConfig) -> image::DynamicImage {
    let mut working = img;

    // 1. Upscale
    let scale = config.upscale.max(1).min(4) as u32;
    if scale > 1 {
        working = working.resize(
            working.width() * scale,
            working.height() * scale,
            image::imageops::FilterType::Lanczos3,
        );
    }

    // 2. Optional grayscale
    let mut gray = if config.grayscale {
        working.to_luma8()
    } else {
        // If not grayscale, convert to luma anyway for threshold consistency, or keep color?
        // For simplicity and Tesseract, we stay in luma/gray world for thresholding.
        working.to_luma8()
    };

    // 3. Optional invert
    if config.invert {
        image::imageops::invert(&mut gray);
    }

    // 4. Optional threshold
    let final_luma = if config.threshold_enabled {
        let threshold = config.threshold;
        image::ImageBuffer::from_fn(gray.width(), gray.height(), |x, y| {
            let pixel = gray.get_pixel(x, y)[0];
            if pixel > threshold {
                image::Luma([255u8])
            } else {
                image::Luma([0u8])
            }
        })
    } else {
        gray
    };

    // 5. Optional simple sharpen (using unsharp mask approximation via resize trick or just skip for now if complex)
    // For simplicity in this pass, we skip advanced sharpen. Can be added later.
    let result = image::DynamicImage::ImageLuma8(final_luma);

    // Very basic contrast boost if no threshold (optional future)
    if !config.threshold_enabled {
        // Could apply contrast here, but keep simple.
    }

    result
}

/// Run OCR on the preprocessed crop using external tesseract (swappable backend).
fn run_tesseract_ocr(
    preprocessed: &image::DynamicImage,
    config: &OcrConfig,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let ocr_dir = std::env::temp_dir().join("siglock").join("ocr");
    std::fs::create_dir_all(&ocr_dir).map_err(|e| e.to_string())?;

    let input_path = ocr_dir.join("ocr_input.png");
    preprocessed.save(&input_path).map_err(|e| e.to_string())?;

    let tesseract_path = resolve_tesseract_executable(Some(app));

    let mut cmd = background_command(tesseract_path);
    cmd.arg(&input_path).arg("stdout");

    // PSM
    let psm = if [7, 8, 13].contains(&config.psm) {
        config.psm
    } else {
        7
    };
    cmd.arg("--psm").arg(psm.to_string());

    // Numeric whitelist if requested
    if config.numeric_only {
        cmd.arg("-c").arg("tessedit_char_whitelist=0123456789");
    }

    let output_result = cmd.output();
    let _ = std::fs::remove_file(&input_path);
    let output = output_result.map_err(|e| {
        format!(
            "Failed to execute tesseract (is it installed and in PATH?): {}",
            e
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Tesseract failed: {}", stderr));
    }

    let raw_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(raw_text)
}

/// Main real OCR pipeline for the selected region.
/// This is the implementation behind the adapter.
/// Now accepts config for tuning and populates debug fields.
fn perform_real_ocr_scan(
    region: ScanRegion,
    config: OcrConfig,
    app: &tauri::AppHandle,
) -> Result<OcrScanResult, String> {
    let scanned_at = chrono::Utc::now().to_rfc3339();

    let screens = screenshots::Screen::all().map_err(|e| e.to_string())?;

    let mut best_screen = &screens[0];
    let mut best_overlap = 0i64;

    for screen in &screens {
        let di = &screen.display_info;
        let overlap = calculate_overlap(
            region.x,
            region.y,
            region.width as i32,
            region.height as i32,
            di,
        );
        if overlap > best_overlap {
            best_overlap = overlap;
            best_screen = screen;
        }
    }

    // Capture directly into memory. Persist only the temporary preprocessed image
    // required by the external Tesseract process.
    let captured_image = best_screen
        .capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| format!("Screen capture failed: {}", e))?;
    let capture_width = captured_image.width();
    let capture_height = captured_image.height();
    let rgba = image::RgbaImage::from_raw(capture_width, capture_height, captured_image.into_raw())
        .ok_or_else(|| "Failed to prepare captured crop for OCR.".to_string())?;
    let dynamic = image::DynamicImage::ImageRgba8(rgba);

    // Preprocess using config
    let preprocessed = preprocess_for_ocr(dynamic, &config);

    // Run OCR with config
    let raw_text_result = run_tesseract_ocr(&preprocessed, &config, app);

    let raw_text = match raw_text_result {
        Ok(text) => text,
        Err(e) => {
            // Special handling for missing Tesseract - return graceful error instead of failing the command
            if e.to_lowercase().contains("tesseract")
                && (e.to_lowercase().contains("not found")
                    || e.to_lowercase().contains("program not found"))
            {
                return Ok(OcrScanResult {
                    raw_text: String::new(),
                    normalized_value: None,
                    confidence: None,
                    scanned_at,
                    error: Some("Tesseract not found. Please install Tesseract and ensure it is in your PATH (or in the common Program Files location).".to_string()),
                    raw_crop_path: None,
                    preprocessed_path: None,
                    capture_width: Some(capture_width),
                    capture_height: Some(capture_height),
                });
            }
            return Err(e);
        }
    };

    // Normalize
    let normalized: String = raw_text.chars().filter(|c| c.is_ascii_digit()).collect();
    let normalized_value = if normalized.is_empty() {
        None
    } else {
        normalized.parse::<u32>().ok()
    };

    let error = if normalized_value.is_none() {
        Some(format!("OCR returned no valid number. Raw: '{}'", raw_text))
    } else {
        None
    };

    Ok(OcrScanResult {
        raw_text,
        normalized_value,
        confidence: None,
        scanned_at,
        error,
        raw_crop_path: None,
        preprocessed_path: None,
        capture_width: Some(capture_width),
        capture_height: Some(capture_height),
    })
}

fn calculate_overlap(
    reg_x: i32,
    reg_y: i32,
    reg_w: i32,
    reg_h: i32,
    di: &screenshots::display_info::DisplayInfo,
) -> i64 {
    let screen_left = di.x;
    let screen_top = di.y;
    let screen_right = screen_left + di.width as i32;
    let screen_bottom = screen_top + di.height as i32;

    let reg_right = reg_x + reg_w;
    let reg_bottom = reg_y + reg_h;

    let overlap_left = reg_x.max(screen_left);
    let overlap_top = reg_y.max(screen_top);
    let overlap_right = reg_right.min(screen_right);
    let overlap_bottom = reg_bottom.min(screen_bottom);

    let w = (overlap_right - overlap_left).max(0) as i64;
    let h = (overlap_bottom - overlap_top).max(0) as i64;
    w * h
}

// ==================== Real Region Capture (using screenshots crate) ====================

#[tauri::command]
async fn capture_region_preview(state: State<'_, AppState>) -> Result<CaptureResult, String> {
    let region = {
        let s = state.lock().unwrap();
        s.region
            .clone()
            .ok_or_else(|| "No region selected. Click 'Set Region' first.".to_string())?
    };

    // Validate region size
    if region.width < 20 || region.height < 10 {
        return Ok(CaptureResult {
            success: false,
            width: region.width,
            height: region.height,
            image_path: None,
            captured_at: chrono::Utc::now().to_rfc3339(),
            error: Some("Region too small for capture.".to_string()),
            preview_data_url: None,
        });
    }

    let screens =
        screenshots::Screen::all().map_err(|e| format!("Failed to enumerate screens: {}", e))?;

    // Find the screen that contains (or is closest to) the region top-left
    let mut best_screen = &screens[0];
    let mut best_overlap = 0i64;

    for screen in &screens {
        let di = &screen.display_info;
        let screen_left = di.x;
        let screen_top = di.y;
        let screen_right = screen_left + di.width as i32;
        let screen_bottom = screen_top + di.height as i32;

        let reg_left = region.x;
        let reg_top = region.y;
        let reg_right = reg_left + region.width as i32;
        let reg_bottom = reg_top + region.height as i32;

        let overlap_left = reg_left.max(screen_left);
        let overlap_top = reg_top.max(screen_top);
        let overlap_right = reg_right.min(screen_right);
        let overlap_bottom = reg_bottom.min(screen_bottom);

        let overlap_w = (overlap_right - overlap_left).max(0) as i64;
        let overlap_h = (overlap_bottom - overlap_top).max(0) as i64;
        let overlap = overlap_w * overlap_h;

        if overlap > best_overlap {
            best_overlap = overlap;
            best_screen = screen;
        }
    }

    // Attempt capture on the best screen
    let capture_result = best_screen.capture_area(region.x, region.y, region.width, region.height);

    match capture_result {
        Ok(captured_image) => {
            // Encode the preview in memory; previews never create debug capture files.
            let width = captured_image.width();
            let height = captured_image.height();
            let rgba = image::RgbaImage::from_raw(width, height, captured_image.into_raw())
                .ok_or_else(|| "Failed to prepare capture preview.".to_string())?;
            let dynamic = image::DynamicImage::ImageRgba8(rgba);
            let mut encoded = Cursor::new(Vec::new());
            dynamic
                .write_to(&mut encoded, image::ImageFormat::Png)
                .map_err(|e| format!("Failed to encode capture preview: {}", e))?;

            let preview_data_url = Some(format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode(encoded.into_inner())
            ));

            Ok(CaptureResult {
                success: true,
                width,
                height,
                image_path: None,
                captured_at: chrono::Utc::now().to_rfc3339(),
                error: None,
                preview_data_url,
            })
        }
        Err(e) => Ok(CaptureResult {
            success: false,
            width: region.width,
            height: region.height,
            image_path: None,
            captured_at: chrono::Utc::now().to_rfc3339(),
            error: Some(format!(
                "Capture failed: {}. Check region coordinates and scaling.",
                e
            )),
            preview_data_url: None,
        }),
    }
}

// ==================== Active Scan Timer (single controlled task) ====================

/// Starts the single active scan timer task if not already running.
/// The task respects: active_scan_enabled + has region + user interval.
fn start_active_scan_timer(app: tauri::AppHandle, app_state: AppState, interval_ms: u64) {
    let safe_interval = clamp_scan_interval(interval_ms);

    let controller_state: State<'_, ActiveScanControllerState> = app.state();
    let mut controller = controller_state.lock().unwrap();

    // Stop previous timer cleanly
    controller.stop_flag.store(true, Ordering::SeqCst);
    if let Some(handle) = controller.handle.take() {
        handle.abort();
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    controller.stop_flag = stop_flag.clone();

    let app_handle = app.clone();
    let app_state_for_task = app_state.clone();
    let stop_flag_for_task = stop_flag.clone();

    let handle = tauri::async_runtime::spawn(async move {
        loop {
            sleep(Duration::from_millis(safe_interval)).await;

            if stop_flag_for_task.load(Ordering::SeqCst) {
                break;
            }

            // Re-check all gates every tick (per user spec)
            let (should_scan, current_interval) = {
                let s = app_state_for_task.lock().unwrap();
                let has_region = s.region.is_some();
                let conditions_met = s.active_scan_enabled && has_region;
                (conditions_met, s.scan_interval_ms)
            };

            if !should_scan {
                continue;
            }

            // Timer tick — in real use this will trigger capture + OCR.
            // For now we emit an event (frontend or future mock can respond).
            let _ = app_handle.emit(
                "timer-scan-tick",
                serde_json::json!({
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "interval_ms": current_interval
                }),
            );
        }
    });

    controller.handle = Some(handle);
}

/// Stops the active scan timer immediately (called on toggle off or hide).
fn stop_active_scan_timer(controller_state: ActiveScanControllerState) {
    let mut controller = controller_state.lock().unwrap();
    controller.stop_flag.store(true, Ordering::SeqCst);
    if let Some(handle) = controller.handle.take() {
        handle.abort();
    }
}

// ==================== Hotkey Registration ====================

fn register_default_hotkeys(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_hide_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyM);

    app.global_shortcut()
        .on_shortcut(show_hide_shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                if let Some(window) = app.get_webview_window("overlay") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                        log_window_lifecycle(app, "overlay", "hide", "overlay_update");
                    } else {
                        let _ = window.set_always_on_top(true);
                        let _ = window.show();
                        log_window_lifecycle(app, "overlay", "show", "overlay_update");
                    }
                    let visible = window.is_visible().unwrap_or(false);
                    if let Ok(mut state) = app.state::<AppState>().lock() {
                        state.overlay_visible = visible;
                    }
                    let _ = app.emit("overlay-visibility-changed", visible);
                }
            }
        })?;

    println!("[SigLock] Default hotkeys registered successfully");
    Ok(())
}

#[cfg(windows)]
unsafe extern "system" fn scan_mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let selected = SCAN_MOUSE_BUTTON.load(Ordering::SeqCst);
        let selected_auto = AUTO_TOGGLE_MOUSE_BUTTON.load(Ordering::SeqCst);
        let data = (*(lparam as *const MSLLHOOKSTRUCT)).mouseData;
        let pressed = mouse_button_from_message(wparam as u32, data);
        if selected != 0 && pressed == selected {
            if let Some(app) = MOUSE_HOOK_APP.get() {
                let _ = app.emit("scan-now-input", ());
            }
        }
        if selected_auto != 0 && pressed == selected_auto {
            if let Some(app) = MOUSE_HOOK_APP.get() {
                let _ = app.emit("hotkey-toggle-active", ());
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

#[cfg(windows)]
fn start_scan_mouse_hook(app: tauri::AppHandle) {
    let _ = MOUSE_HOOK_APP.set(app);
    std::thread::spawn(|| unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(scan_mouse_hook), std::ptr::null_mut(), 0);
        if hook.is_null() {
            eprintln!("[SigLock] Failed to install global mouse hook");
            return;
        }
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
}

#[cfg(not(windows))]
fn start_scan_mouse_hook(_app: tauri::AppHandle) {}

#[cfg(test)]
mod tests {
    use super::{clamp_scan_interval, scan_mouse_button};
    #[cfg(windows)]
    use super::{mouse_button_from_message, WM_MBUTTONDOWN, WM_XBUTTONDOWN};

    #[test]
    fn scan_interval_is_clamped_to_product_range() {
        assert_eq!(clamp_scan_interval(0), 1000);
        assert_eq!(clamp_scan_interval(1000), 1000);
        assert_eq!(clamp_scan_interval(3000), 3000);
        assert_eq!(clamp_scan_interval(4000), 4000);
        assert_eq!(clamp_scan_interval(30000), 4000);
    }

    #[test]
    fn supported_mouse_bindings_map_to_global_hook_buttons() {
        assert_eq!(scan_mouse_button(None).unwrap(), 0);
        assert_eq!(scan_mouse_button(Some("Middle Mouse")).unwrap(), 1);
        assert_eq!(scan_mouse_button(Some("Mouse4")).unwrap(), 4);
        assert_eq!(scan_mouse_button(Some("Mouse5")).unwrap(), 5);
        assert!(scan_mouse_button(Some("Left Click")).is_err());
        assert!(scan_mouse_button(Some("Right Click")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn global_mouse_messages_trigger_middle_mouse4_and_mouse5() {
        assert_eq!(mouse_button_from_message(WM_MBUTTONDOWN, 0), 1);
        assert_eq!(mouse_button_from_message(WM_XBUTTONDOWN, 1 << 16), 4);
        assert_eq!(mouse_button_from_message(WM_XBUTTONDOWN, 2 << 16), 5);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: AppState = Arc::new(Mutex::new(default_state()));
    let timer_controller: ActiveScanControllerState =
        Arc::new(Mutex::new(ActiveScanController::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state.clone())
        .manage(timer_controller.clone())
        .invoke_handler(tauri::generate_handler![
            match_signature,
            minimize_main_window,
            request_app_shutdown,
            toggle_overlay_visibility,
            get_overlay_setup_mode,
            set_overlay_setup_mode,
            reset_overlay_position,
            set_scan_now_mouse_binding,
            set_auto_toggle_mouse_binding,
            trigger_single_scan,
            toggle_active_scan,
            set_scan_interval,
            get_app_state,
            set_crop_region,
            get_crop_region,
            clear_crop_region,
            scan_selected_region,
            open_region_picker,
            capture_region_preview,
            check_tesseract
        ])
        .setup(move |app| {
            let native_settings = load_native_settings(&app.handle());
            log_window_lifecycle(&app.handle(), "main", "create", "startup");
            log_window_lifecycle(&app.handle(), "main", "show", "startup");
            if let Ok(mut current_state) = state.lock() {
                current_state.region = native_settings.region.clone();
            }
            if let Some(overlay) = app.get_webview_window("overlay") {
                log_window_lifecycle(&app.handle(), "overlay", "create", "startup");
                log_window_lifecycle(&app.handle(), "overlay", "show", "startup");
                let _ = overlay.set_always_on_top(true);
                let _ = overlay.set_ignore_cursor_events(true);
                let position = safe_overlay_position(&overlay, native_settings.overlay_position);
                let _ = overlay.set_position(Position::Physical(position));
                log_window_lifecycle(&app.handle(), "overlay", "set_position", "startup");
            }
            suppress_tao_event_target_artifact(&app.handle());
            start_scan_mouse_hook(app.handle().clone());

            if let Err(e) = register_default_hotkeys(&app.handle()) {
                eprintln!("[SigLock] Failed to register hotkeys: {}", e);
            } else {
                println!("[SigLock] Hotkeys registered: Ctrl+Shift+M (show/hide)");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let app = window.app_handle();
                    request_shutdown_once(&app, "window-close");
                }
            }
            if window.label() == "overlay" {
                if let WindowEvent::Moved(position) = event {
                    let _ = save_overlay_position(&window.app_handle(), *position);
                }
            }
            if window.label() == "region_picker" {
                if let WindowEvent::Destroyed = event {
                    log_window_lifecycle(
                        &window.app_handle(),
                        "region_picker",
                        "close",
                        "set_region",
                    );
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

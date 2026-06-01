use base64::engine::general_purpose;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::time::sleep;

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
    pub upscale: u32,           // 1, 2, 3, 4
    pub threshold_enabled: bool,
    pub threshold: u8,          // 0-255
    pub grayscale: bool,
    pub invert: bool,
    pub sharpen: bool,
    pub psm: u8,                // 7, 8, 13 etc.
    pub numeric_only: bool,     // apply whitelist
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
// - All dev/debug artifacts go through isolated subdirectories.
// - Only "latest" debug files are kept (no endless accumulation).

/// Returns the base app data directory using Tauri's proper resolver.
/// This is the correct way for both development and packaged apps.
fn get_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))
}

/// Returns the directory used for debug captures and OCR working files.
/// This is isolated and will be easy to clean or change in packaged builds.
fn get_debug_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = get_app_data_dir(app)?;
    Ok(base.join("siglock").join("debug"))
}

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
            let bundled = resource_dir
                .join("tesseract")
                .join("tesseract.exe");
            candidates.push(bundled);
        }
    }

    // 2. Common 64-bit Windows install location
    candidates.push(PathBuf::from(r"C:\Program Files\Tesseract-OCR\tesseract.exe"));

    // 3. Common 32-bit Windows install location
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe"));

    // 4. System PATH fallback
    candidates.push(PathBuf::from("tesseract"));

    for candidate in &candidates {
        // For PATH entry ("tesseract"), we don't check existence — just try to run it.
        // For explicit paths, we can skip if the file doesn't exist to avoid noise.
        if candidate == &PathBuf::from("tesseract") || candidate.exists() {
            if let Ok(output) = std::process::Command::new(candidate)
                .arg("--version")
                .output()
            {
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
    Material { name: "Quantainium", base: 3170, category: Some("High value") },
    Material { name: "Stileron", base: 3185, category: None },
    Material { name: "Savrilium", base: 3200, category: None },
    Material { name: "Ouratite", base: 3370, category: None },
    Material { name: "Riccite", base: 3385, category: None },
    Material { name: "Lindinium", base: 3400, category: None },
    Material { name: "Beryl", base: 3540, category: None },
    Material { name: "Taranite", base: 3555, category: None },
    Material { name: "Borase", base: 3570, category: None },
    Material { name: "Gold", base: 3585, category: None },
    Material { name: "Bexalite", base: 3600, category: None },
    Material { name: "Laranite", base: 3825, category: None },
    Material { name: "Aslarite", base: 3840, category: None },
    Material { name: "Titanium", base: 3855, category: None },
    Material { name: "Tungsten", base: 3870, category: None },
    Material { name: "Agricium", base: 3885, category: None },
    Material { name: "Torite", base: 3900, category: None },
    Material { name: "Hephestanite", base: 4180, category: None },
    Material { name: "Tin", base: 4195, category: None },
    Material { name: "Quartz", base: 4210, category: None },
    Material { name: "Corundum", base: 4225, category: None },
    Material { name: "Copper", base: 4240, category: None },
    Material { name: "Silicon", base: 4255, category: None },
    Material { name: "Iron", base: 4270, category: None },
    Material { name: "Aluminium", base: 4285, category: None },
    Material { name: "Ice", base: 4300, category: Some("Common") },
];

/// Core matching logic (exact first, then ± tolerance)
#[tauri::command]
fn match_signature(observed: u32, tolerance: Option<i32>) -> Vec<MatchResult> {
    let tolerance = tolerance.unwrap_or(25);
    let mut results: Vec<MatchResult> = Vec::new();

    for mat in MATERIALS {
        if mat.base == 0 { continue; }

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
    /// Current visibility of the main overlay window (updated on our hide/show paths)
    is_visible: bool,
    /// Current OCR region (None = not set)
    region: Option<ScanRegion>,
    /// Last observed signature and source
    last_value: Option<u32>,
    last_source: Option<String>,
    /// Current hotkey configuration
    hotkey_show_hide: String,
    hotkey_single_scan: String,
    hotkey_toggle_active: String,
    /// Active scan interval in milliseconds (hard range 2000-5000)
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
        is_visible: true,           // Window starts visible
        region: None,
        last_value: None,
        last_source: None,
        hotkey_show_hide: "Ctrl+Shift+M".to_string(),
        hotkey_single_scan: "Ctrl+Shift+S".to_string(),
        hotkey_toggle_active: "Ctrl+Shift+O".to_string(),
        scan_interval_ms: 3000,
    }
}

// ==================== Commands ====================

#[tauri::command]
async fn toggle_overlay_visibility(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        let currently_visible = window.is_visible().unwrap_or(false);

        if currently_visible {
            let _ = window.hide();
            {
                let mut s = state.lock().unwrap();
                s.is_visible = false;
            }

            // Per spec: hiding the overlay must pause Active Scan
            let controller: State<'_, ActiveScanControllerState> = app.state();
            stop_active_scan_timer(controller.inner().clone());

            Ok(false)
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit("focus-manual-input", ());
            {
                let mut s = state.lock().unwrap();
                s.is_visible = true;

                // Per spec: on show, resume Active Scan only if it was enabled AND we have a region
                if s.active_scan_enabled && s.region.is_some() {
                    let interval = s.scan_interval_ms;
                    drop(s); // release lock before starting timer
                    start_active_scan_timer(app.clone(), state.inner().clone(), interval);
                }
            }
            Ok(true)
        }
    } else {
        Err("Main window not found".into())
    }
}

#[tauri::command]
async fn trigger_single_scan(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Per spec: if hidden, show the overlay first
    if let Some(window) = app.get_webview_window("main") {
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit("focus-manual-input", ());
            let mut s = state.lock().unwrap();
            s.is_visible = true;
        }
    }

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
async fn toggle_active_scan(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let (enabled, had_region, interval) = {
        let mut s = state.lock().unwrap();
        s.active_scan_enabled = !s.active_scan_enabled;
        (s.active_scan_enabled, s.region.is_some(), s.scan_interval_ms)
    };

    let _ = app.emit("active-scan-toggled", enabled);

    if enabled {
        if had_region {
            start_active_scan_timer(app.clone(), state.inner().clone(), interval);
            println!("[SigLock] Active Scan ENABLED → timer started ({}ms interval)", interval);
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
fn get_app_state(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let s = state.lock().unwrap();
    Ok(serde_json::json!({
        "active_scan_enabled": s.active_scan_enabled,
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
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.region = Some(region);
    Ok(())
}

#[tauri::command]
fn get_crop_region(state: State<'_, AppState>) -> Result<Option<ScanRegion>, String> {
    let s = state.lock().unwrap();
    Ok(s.region.clone())
}

#[tauri::command]
async fn clear_crop_region(state: State<'_, AppState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.region = None;
    Ok(())
}

#[tauri::command]
async fn check_tesseract(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let tesseract_path = resolve_tesseract_executable(Some(&app));

    let output = std::process::Command::new(&tesseract_path)
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
        Err(e) => {
            Ok(serde_json::json!({
                "available": false,
                "path_checked": tesseract_path.to_string_lossy(),
                "error": format!(
                    "Tesseract not found at any known location (tried bundled, Program Files, and PATH). Last tried: {}",
                    tesseract_path.display()
                )
            }))
        }
    }
}

#[tauri::command]
async fn open_region_picker(app: tauri::AppHandle) -> Result<(), String> {
    println!("[SigLock] open_region_picker command called from frontend");

    // Always close any existing picker first to avoid stale giant overlay
    if let Some(existing) = app.get_webview_window("region-picker") {
        println!("[SigLock] Closing stale region-picker window before opening new one");
        let _ = existing.close();
        // Give it a moment to actually close
        tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
    }

    println!("[SigLock] Creating new region-picker window...");

    // Create picker window with explicit flags
    let picker = tauri::WebviewWindowBuilder::new(
        &app,
        "region-picker",
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

    println!("[SigLock] region-picker window created successfully");
    println!("[SigLock] transparent flag applied via builder + config");
    println!("[SigLock] always-on-top flag applied via builder + runtime");

    // Apply runtime flags for reliability (especially important for borderless/windowed games)
    let _ = picker.set_decorations(false);
    println!("[SigLock] decorations set to false");

    let _ = picker.set_resizable(false);

    let _ = picker.set_always_on_top(true);
    println!("[SigLock] always-on-top applied");

    let _ = picker.set_skip_taskbar(true);

    // Ensure it is shown and focused on top
    let _ = picker.show();
    let _ = picker.set_focus();
    println!("[SigLock] picker shown and focused");

    Ok(())
}

// ==================== OCR Adapter (Mock for now) ====================

#[tauri::command]
async fn scan_selected_region(
    state: State<'_, AppState>,
    config: Option<OcrConfig>,
    app: tauri::AppHandle,
) -> Result<OcrScanResult, String> {
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
    let mut result = image::DynamicImage::ImageLuma8(final_luma);

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
    let debug_dir = get_debug_dir(app)?;
    let ocr_dir = debug_dir.join("ocr");
    std::fs::create_dir_all(&ocr_dir).map_err(|e| e.to_string())?;

    let input_path = ocr_dir.join("ocr_input.png");
    preprocessed.save(&input_path).map_err(|e| e.to_string())?;

    let tesseract_path = resolve_tesseract_executable(Some(app));

    let mut cmd = std::process::Command::new(tesseract_path);
    cmd.arg(&input_path).arg("stdout");

    // PSM
    let psm = if [7, 8, 13].contains(&config.psm) { config.psm } else { 7 };
    cmd.arg("--psm").arg(psm.to_string());

    // Numeric whitelist if requested
    if config.numeric_only {
        cmd.arg("-c").arg("tessedit_char_whitelist=0123456789");
    }

    let output = cmd.output()
        .map_err(|e| format!("Failed to execute tesseract (is it installed and in PATH?): {}", e))?;

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
        let overlap = calculate_overlap(region.x, region.y, region.width as i32, region.height as i32, di);
        if overlap > best_overlap {
            best_overlap = overlap;
            best_screen = screen;
        }
    }

    // Capture and save raw crop (for debug + Capture Test button)
    let captured_image = best_screen
        .capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| format!("Screen capture failed: {}", e))?;

    let debug_dir = get_debug_dir(app)?;
    let captures_dir = debug_dir.join("captures");
    std::fs::create_dir_all(&captures_dir).ok();

    let raw_path = captures_dir.join("last_capture.png");
    captured_image.save(&raw_path).map_err(|e| e.to_string())?;

    // Load for preprocessing
    let dynamic = image::open(&raw_path)
        .map_err(|e| format!("Failed to load captured crop for OCR: {}", e))?;

    // Preprocess using config
    let preprocessed = preprocess_for_ocr(dynamic, &config);

    // Save preprocessed debug image (latest only)
    let preprocessed_path = captures_dir.join("last_preprocessed.png");
    preprocessed.save(&preprocessed_path).ok(); // best effort

    // Run OCR with config
    let raw_text_result = run_tesseract_ocr(&preprocessed, &config, app);

    let raw_text = match raw_text_result {
        Ok(text) => text,
        Err(e) => {
            // Special handling for missing Tesseract - return graceful error instead of failing the command
            if e.to_lowercase().contains("tesseract") && 
               (e.to_lowercase().contains("not found") || e.to_lowercase().contains("program not found")) 
            {
                return Ok(OcrScanResult {
                    raw_text: String::new(),
                    normalized_value: None,
                    confidence: None,
                    scanned_at,
                    error: Some("Tesseract not found. Please install Tesseract and ensure it is in your PATH (or in the common Program Files location).".to_string()),
                    raw_crop_path: Some(raw_path.to_string_lossy().to_string()),
                    preprocessed_path: Some(preprocessed_path.to_string_lossy().to_string()),
                    capture_width: Some(region.width),
                    capture_height: Some(region.height),
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
        raw_crop_path: Some(raw_path.to_string_lossy().to_string()),
        preprocessed_path: Some(preprocessed_path.to_string_lossy().to_string()),
        capture_width: Some(region.width),
        capture_height: Some(region.height),
    })
}

fn calculate_overlap(reg_x: i32, reg_y: i32, reg_w: i32, reg_h: i32, di: &screenshots::display_info::DisplayInfo) -> i64 {
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
async fn capture_region_preview(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<CaptureResult, String> {
    let region = {
        let s = state.lock().unwrap();
        s.region.clone().ok_or_else(|| "No region selected. Click 'Set Region' first.".to_string())?
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

    let screens = screenshots::Screen::all().map_err(|e| format!("Failed to enumerate screens: {}", e))?;

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
        Ok(image) => {
            // Save using the centralized debug directory (release-friendly)
            let captures_dir = get_debug_dir(&app)?.join("captures");
            std::fs::create_dir_all(&captures_dir).map_err(|e| e.to_string())?;

            let file_path = captures_dir.join("last_capture.png");
            image.save(&file_path).map_err(|e| format!("Failed to save capture: {}", e))?;

            println!("[Capture] Saved raw crop to: {}", file_path.display());

            // Generate base64 data URL for reliable preview (preferred for debug)
            // Read the file we just saved — most reliable across image crate versions
            let preview_data_url = match std::fs::read(&file_path) {
                Ok(bytes) => Some(format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&bytes))),
                Err(_) => None,
            };

            Ok(CaptureResult {
                success: true,
                width: image.width(),
                height: image.height(),
                image_path: Some(file_path.to_string_lossy().to_string()),
                captured_at: chrono::Utc::now().to_rfc3339(),
                error: None,
                preview_data_url,
            })
        }
        Err(e) => {
            Ok(CaptureResult {
                success: false,
                width: region.width,
                height: region.height,
                image_path: None,
                captured_at: chrono::Utc::now().to_rfc3339(),
                error: Some(format!("Capture failed: {}. Check region coordinates and scaling.", e)),
                preview_data_url: None,
            })
        }
    }
}

// ==================== Active Scan Timer (single controlled task) ====================

/// Starts the single active scan timer task if not already running.
/// The task respects: active_scan_enabled + has region + is_visible + hard 2s minimum.
fn start_active_scan_timer(
    app: tauri::AppHandle,
    app_state: AppState,
    interval_ms: u64,
) {
    // Enforce hard minimum of 2000ms (and max 5000 as per spec)
    let safe_interval = interval_ms.max(2000).min(5000);

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
                let conditions_met = s.active_scan_enabled && has_region && s.is_visible;
                (conditions_met, s.scan_interval_ms)
            };

            if !should_scan {
                continue;
            }

            // Timer tick — in real use this will trigger capture + OCR.
            // For now we emit an event (frontend or future mock can respond).
            let _ = app_handle.emit("timer-scan-tick", serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "interval_ms": current_interval
            }));
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
    let single_scan_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);
    let toggle_active_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyO);

    let app_handle = app.clone();
    app.global_shortcut().on_shortcut(show_hide_shortcut, move |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            let _ = app.emit("hotkey-show-hide", ());
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = app.emit("focus-manual-input", ());
                }
            }
        }
    })?;

    let app_handle2 = app_handle.clone();
    app.global_shortcut().on_shortcut(single_scan_shortcut, move |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            let _ = app.emit("hotkey-single-scan", ());
        }
    })?;

    app.global_shortcut().on_shortcut(toggle_active_shortcut, move |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            let _ = app.emit("hotkey-toggle-active", ());
        }
    })?;

    println!("[SigLock] Default hotkeys registered successfully");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: AppState = Arc::new(Mutex::new(default_state()));
    let timer_controller: ActiveScanControllerState = Arc::new(Mutex::new(ActiveScanController::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state.clone())
        .manage(timer_controller.clone())
        .invoke_handler(tauri::generate_handler![
            match_signature,
            toggle_overlay_visibility,
            trigger_single_scan,
            toggle_active_scan,
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
            if let Err(e) = register_default_hotkeys(&app.handle()) {
                eprintln!("[SigLock] Failed to register hotkeys: {}", e);
            } else {
                println!("[SigLock] Hotkeys registered: Ctrl+Shift+M (show/hide), S (single scan), O (toggle active)");
            }

            // Active Scan is forced OFF on every launch (already default)
            // Region persistence load can be added here once store trait is in scope on the handle.

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
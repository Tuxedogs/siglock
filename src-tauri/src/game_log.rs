use serde::Serialize;
use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::Emitter;

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameLogStatus {
    pub path: Option<PathBuf>,
    pub channel: Option<String>,
    pub health: String,
    pub current_location: Option<String>,
    pub location_source: Option<String>,
    pub location_confidence: Option<String>,
    pub detected_ship: Option<String>,
    pub ship_source: Option<String>,
    pub player_handle: Option<String>,
    pub last_line_at: Option<String>,
}

pub type GameLogStatusState = Arc<Mutex<GameLogStatus>>;

pub struct GameLogController {
    stop: Arc<AtomicBool>,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl Default for GameLogController {
    fn default() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

impl GameLogController {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

pub type GameLogControllerState = Arc<Mutex<GameLogController>>;

#[derive(Clone, Debug)]
pub struct DiscoveryResult {
    pub selected: Option<PathBuf>,
    pub roots: Vec<PathBuf>,
}

fn valid_game_log(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Game.log"))
}

fn launcher_log_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("rsilauncher").join("logs").join("log.log"))
}

fn roots_from_launcher_log(path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let marker = "Launching Star Citizen ";
            let start = line.find(marker)? + marker.len();
            let rest = &line[start..];
            let open = rest.find('(')? + 1;
            let close = rest[open..].find(')')? + open;
            let channel_dir = PathBuf::from(&rest[open..close]);
            channel_dir.parent().map(Path::to_path_buf)
        })
        .collect()
}

fn conventional_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(variable) {
            roots.push(
                PathBuf::from(base)
                    .join("Roberts Space Industries")
                    .join("StarCitizen"),
            );
        }
    }
    roots
}

pub fn candidate_logs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut logs = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let log = path.join("Game.log");
                if valid_game_log(&log) {
                    logs.push(log);
                }
            }
        }
    }
    logs
}

fn modified_at(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

pub fn discover_game_log(persisted: Option<&Path>) -> DiscoveryResult {
    if let Some(path) = persisted.filter(|path| valid_game_log(path)) {
        return DiscoveryResult {
            selected: Some(path.to_path_buf()),
            roots: path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .into_iter()
                .collect(),
        };
    }

    let mut roots = BTreeSet::new();
    if let Some(path) = launcher_log_path() {
        roots.extend(roots_from_launcher_log(&path));
    }
    roots.extend(conventional_roots());
    let roots = roots.into_iter().collect::<Vec<_>>();
    let selected = candidate_logs(&roots)
        .into_iter()
        .max_by_key(|path| modified_at(path));
    DiscoveryResult { selected, roots }
}

pub fn channel_name(path: &Path) -> Option<String> {
    path.parent()
        .and_then(Path::file_name)
        .map(|value| value.to_string_lossy().into_owned())
}

fn ooc_token(line: &str) -> Option<&str> {
    let start = line.find("OOC_")?;
    let tail = &line[start..];
    let len = tail
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(tail.len());
    Some(&tail[..len])
}

fn quoted_actor(line: &str) -> Option<&str> {
    let value = &line[line.find("Actor '")? + "Actor '".len()..];
    Some(&value[..value.find('\'')?])
}

pub fn normalize_location(identifier: &str) -> Option<String> {
    let lower = identifier.to_ascii_lowercase();
    let known = [
        ("stanton_2b_daymar", "daymar"),
        ("stanton_2c_yela", "yela"),
        ("stanton_2a_cellin", "cellin"),
        ("stanton_1b_aberdeen", "aberdeen"),
        ("stanton_3a_lyria", "lyria"),
        ("stanton_3b_wala", "wala"),
        ("stanton_4a_calliope", "calliope"),
        ("stanton_4b_clio", "clio"),
        ("stanton_4c_euterpe", "euterpe"),
        ("stanton_1a_ariel", "ariel"),
        ("stanton_1c_magda", "magda"),
        ("stanton_1d_ita", "ita"),
        ("stanton_1_hurston", "hurston"),
        ("stanton_2_crusader", "crusader"),
        ("stanton_3_arccorp", "arccorp"),
        ("stanton_4_microtech", "microtech"),
        ("stanton2_l1", "cru-l1"),
    ];
    known
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, location)| (*location).to_string())
}

#[derive(Default)]
pub struct LogClassifier {
    status: GameLogStatus,
}

impl LogClassifier {
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            status: GameLogStatus {
                channel: channel_name(&path),
                path: Some(path),
                health: "monitoring".to_string(),
                ..GameLogStatus::default()
            },
        }
    }

    pub fn status(&self) -> &GameLogStatus {
        &self.status
    }

    pub fn process_line(&mut self, line: &str) -> bool {
        let before = self.status.clone();
        self.capture_player_handle(line);
        self.capture_location(line);
        self.capture_ship_evidence(line);
        if self.status != before {
            self.status.last_line_at = Some(chrono::Utc::now().to_rfc3339());
            true
        } else {
            false
        }
    }

    fn capture_player_handle(&mut self, line: &str) {
        if !line.contains("Login") {
            return;
        }
        if let Some(start) = line.find("Handle[") {
            let value = &line[start + 7..];
            if let Some(end) = value.find(']') {
                if end > 0 {
                    self.status.player_handle = Some(value[..end].to_string());
                }
            }
        }
    }

    fn capture_location(&mut self, line: &str) {
        // These are periodic/global listings and scenario setup, not player position.
        if line.contains("planet cells:")
            || line.contains("ScenarioZoneHost:")
            || line.contains("Data/objectcontainers/")
        {
            return;
        }
        let source = if line.contains("<[ActorState]")
            && line.contains("Actor '")
            && line.contains("to zone 'OOC_")
            && self
                .status
                .player_handle
                .as_deref()
                .is_some_and(|player| quoted_actor(line) == Some(player))
        {
            Some("player actor zone transition")
        } else if line.contains("<FatalCollision>")
            && line.contains("PlayerPilot: 1")
            && line.contains("Zone: OOC_")
        {
            Some("current vehicle player-pilot zone")
        } else if line.contains("<Update Inventory Location>")
            && line.contains("Player [")
            && line.contains("OOC_")
        {
            Some("player inventory location transition")
        } else {
            None
        };
        let Some(source) = source else { return };
        let Some(location) = ooc_token(line).and_then(normalize_location) else {
            return;
        };
        self.status.current_location = Some(location);
        self.status.location_source = Some(source.to_string());
        self.status.location_confidence = Some("high".to_string());
    }

    fn capture_ship_evidence(&mut self, line: &str) {
        if !line.contains("You have joined channel '") {
            return;
        }
        let Some(player) = self.status.player_handle.as_deref() else {
            return;
        };
        let Some(start) = line.find("You have joined channel '") else {
            return;
        };
        let value = &line[start + "You have joined channel '".len()..];
        let Some(end) = value.find("'.") else { return };
        let channel = &value[..end];
        let Some((vehicle, occupant)) = channel.rsplit_once(" : ") else {
            return;
        };
        if occupant != player {
            return;
        }
        let vehicle_lower = vehicle.to_ascii_lowercase();
        let ship = if vehicle_lower.contains("golem") {
            Some("golem")
        } else if vehicle_lower.contains("prospector") {
            Some("prospector")
        } else if vehicle_lower.contains("mole") {
            Some("mole")
        } else {
            None
        };
        if let Some(ship) = ship {
            self.status.detected_ship = Some(ship.to_string());
            self.status.ship_source = Some("player vehicle chat channel joined".to_string());
        }
    }
}

#[derive(Default)]
struct TailCursor {
    offset: u64,
    created: Option<SystemTime>,
    pending: String,
}

impl TailCursor {
    fn reset_if_replaced(&mut self, metadata: &Metadata) {
        let created = metadata.created().ok();
        if metadata.len() < self.offset || (self.created.is_some() && created != self.created) {
            self.offset = 0;
            self.pending.clear();
        }
        self.created = created;
    }

    fn read_appended(&mut self, path: &Path) -> Result<Vec<String>, String> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        self.reset_if_replaced(&metadata);
        file.seek(SeekFrom::Start(self.offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        self.offset += bytes.len() as u64;
        self.pending.push_str(&String::from_utf8_lossy(&bytes));
        let mut parts = self
            .pending
            .split('\n')
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.pending = parts.pop().unwrap_or_default();
        Ok(parts
            .into_iter()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect())
    }
}

fn publish_status(app: &tauri::AppHandle, state: &GameLogStatusState, status: &GameLogStatus) {
    *state.lock().unwrap() = status.clone();
    let _ = app.emit("game-log-status-updated", status);
}

fn read_context_lines(path: &Path) -> Vec<String> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(metadata) = file.metadata() else {
        return Vec::new();
    };
    let mut chunks = Vec::new();
    let mut head = Vec::new();
    if file
        .by_ref()
        .take(128 * 1024)
        .read_to_end(&mut head)
        .is_ok()
    {
        chunks.push(String::from_utf8_lossy(&head).into_owned());
    }
    if metadata.len() > 128 * 1024 {
        let _ = file.seek(SeekFrom::Start(metadata.len().saturating_sub(512 * 1024)));
        let mut tail = Vec::new();
        if file.read_to_end(&mut tail).is_ok() {
            chunks.push(String::from_utf8_lossy(&tail).into_owned());
        }
    }
    chunks
        .into_iter()
        .flat_map(|chunk| chunk.lines().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

pub fn start_monitor(
    app: tauri::AppHandle,
    controller_state: GameLogControllerState,
    status_state: GameLogStatusState,
    discovery: DiscoveryResult,
) {
    let mut controller = controller_state.lock().unwrap();
    controller.stop();
    let stop = Arc::new(AtomicBool::new(false));
    controller.stop = stop.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let Some(mut path) = discovery.selected else {
            publish_status(
                &app,
                &status_state,
                &GameLogStatus {
                    health: "not-found".to_string(),
                    ..GameLogStatus::default()
                },
            );
            return;
        };
        let mut classifier = LogClassifier::with_path(path.clone());
        let mut cursor = TailCursor::default();
        // Establish login and recent location context from bounded slices, then
        // hold an offset and read appended bytes only.
        for line in read_context_lines(&path) {
            classifier.process_line(&line);
        }
        if let Ok(metadata) = path.metadata() {
            cursor.offset = metadata.len();
            cursor.created = metadata.created().ok();
        }
        publish_status(&app, &status_state, classifier.status());
        let mut idle_ticks = 0u32;
        while !stop.load(Ordering::SeqCst) {
            let mut changed = false;
            match cursor.read_appended(&path) {
                Ok(lines) => {
                    if lines.is_empty() {
                        idle_ticks += 1;
                    } else {
                        idle_ticks = 0;
                    }
                    for line in lines {
                        changed |= classifier.process_line(&line);
                    }
                }
                Err(_) => {
                    classifier.status.health = "waiting".to_string();
                    changed = true;
                    idle_ticks += 1;
                }
            }

            // A channel switch leaves the old log in place. Compare only the known
            // direct channel folders after sustained idle time; never scan a drive.
            if idle_ticks >= 30 {
                if let Some(recent) = candidate_logs(&discovery.roots)
                    .into_iter()
                    .max_by_key(|candidate| modified_at(candidate))
                {
                    if recent != path && modified_at(&recent) > modified_at(&path) {
                        path = recent;
                        classifier = LogClassifier::with_path(path.clone());
                        cursor = TailCursor::default();
                        changed = true;
                    }
                }
                idle_ticks = 0;
            }
            if changed {
                publish_status(&app, &status_state, classifier.status());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    controller.handle = Some(handle);
}

pub fn stop_monitor(controller: &GameLogControllerState) {
    controller.lock().unwrap().stop();
}

#[cfg(test)]
mod tests {
    use super::{normalize_location, LogClassifier, TailCursor};
    use std::io::Write;

    #[test]
    fn normalizes_supported_object_container_identifiers() {
        assert_eq!(
            normalize_location("OOC_Stanton_2b_Daymar").as_deref(),
            Some("daymar")
        );
        assert_eq!(
            normalize_location("OOC_Stanton_4c_Euterpe").as_deref(),
            Some("euterpe")
        );
    }

    #[test]
    fn generic_celestial_dump_does_not_change_location() {
        let mut classifier = LogClassifier::default();
        classifier.process_line("User Login Success - Handle[Tux-Actual] - Time[1]");
        classifier.process_line("<[ActorState] Dead> Actor 'Tux-Actual' [42] ejected from zone 'Ship' [1] to zone 'OOC_Stanton_2b_Daymar' [2]");
        classifier.process_line("planet cells: 0 meshes: 0 name: OOC_Stanton_2b_Daymar");
        classifier.process_line("planet cells: 0 meshes: 0 name: OOC_Stanton_3a_Lyria");
        assert_eq!(
            classifier.status().current_location.as_deref(),
            Some("daymar")
        );
    }

    #[test]
    fn player_actor_transition_is_high_confidence() {
        let mut classifier = LogClassifier::default();
        classifier.process_line("User Login Success - Handle[Tux-Actual] - Time[1]");
        classifier.process_line("<[ActorState] Dead> Actor 'Tux-Actual' [42] ejected from zone 'Ship' [1] to zone 'OOC_Stanton_2a_Cellin' [2]");
        assert_eq!(
            classifier.status().current_location.as_deref(),
            Some("cellin")
        );
        assert_eq!(
            classifier.status().location_confidence.as_deref(),
            Some("high")
        );
    }

    #[test]
    fn unrelated_actor_transition_does_not_change_player_location() {
        let mut classifier = LogClassifier::default();
        classifier.process_line("User Login Success - Handle[Tux-Actual] - Time[1]");
        classifier.process_line("<[ActorState] Dead> Actor 'Pirate-NPC' [99] ejected from zone 'Ship' [1] to zone 'OOC_Stanton_3a_Lyria' [2]");
        assert_eq!(classifier.status().current_location, None);
    }

    #[test]
    fn current_player_ship_channel_is_evidence_but_other_players_are_ignored() {
        let mut classifier = LogClassifier::default();
        classifier.process_line("User Login Success - Handle[Tux-Actual] - Time[1]");
        classifier
            .process_line("You have joined channel 'Argo MOLE Teach's Special : Stud-Muffin'.");
        assert_eq!(classifier.status().detected_ship, None);
        classifier.process_line("You have joined channel 'Drake Golem : Tux-Actual'.");
        assert_eq!(classifier.status().detected_ship.as_deref(), Some("golem"));
    }

    #[test]
    fn tail_cursor_reads_only_appends_and_recovers_from_truncation() {
        let path = std::env::temp_dir().join(format!("siglock-tail-{}.log", std::process::id()));
        std::fs::write(&path, "first\n").unwrap();
        let mut cursor = TailCursor::default();
        assert_eq!(cursor.read_appended(&path).unwrap(), vec!["first"]);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"second\n").unwrap();
        assert_eq!(cursor.read_appended(&path).unwrap(), vec!["second"]);
        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(cursor.read_appended(&path).unwrap(), vec!["new"]);
        let _ = std::fs::remove_file(path);
    }
}

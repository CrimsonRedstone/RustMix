//! Settings live in ~/.config/RustMix. Nothing is written until the
//! person changes a control. A screen choice is the only reason the KWin
//! rule file is touched, and other rules in that file stay.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const RULE_ID: &str = crate::PROCESS_NAME;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locks {
    pub all: bool,
    pub monitor: bool,
    pub phones: [bool; 2],
    pub trims: [bool; 10],
    pub cells: Vec<(u16, u16)>,
    pub buses: Vec<u16>,
}

impl Default for Locks {
    fn default() -> Self {
        Self {
            all: false,
            monitor: false,
            phones: [false; 2],
            trims: [false; 10],
            cells: Vec::new(),
            buses: Vec::new(),
        }
    }
}

impl Locks {
    pub fn blocks_monitor(&self) -> bool {
        self.all || self.monitor
    }

    pub fn blocks_phones(&self, pair: u8) -> bool {
        self.all || self.phones.get(pair as usize).copied().unwrap_or(false)
    }

    pub fn blocks_trim(&self, which: u8) -> bool {
        self.all || self.trims.get(which as usize).copied().unwrap_or(false)
    }

    pub fn blocks_cell(&self, bus: u16, input: u16) -> bool {
        self.all || self.cells.iter().any(|cell| *cell == (bus, input))
    }

    pub fn blocks_bus(&self, bus: u16) -> bool {
        self.all || self.buses.contains(&bus)
    }

    pub fn set_cell(&mut self, bus: u16, input: u16, locked: bool) {
        self.cells.retain(|cell| *cell != (bus, input));
        if locked {
            self.cells.push((bus, input));
        }
    }

    pub fn set_bus(&mut self, bus: u16, locked: bool) {
        self.buses.retain(|have| *have != bus);
        if locked {
            self.buses.push(bus);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub close_quits: bool,
    /// KWin screen index last resolved from `screen_key`. The key is the identity.
    pub screen: Option<i32>,
    /// kscreen output id. This stays the same when KWin renumbers screens.
    pub screen_key: Option<String>,
    /// Window position and size relative to that screen's top left: x, y, w, h.
    pub place: Option<[f32; 4]>,
    pub locks: Locks,
    /// Assistant access. Off until Control turns it on. A missing key stays off.
    pub mcp_enabled: bool,
    /// IP address the assistant connects to. `localhost` is stored as 127.0.0.1.
    pub mcp_host: String,
    pub mcp_port: u16,
    /// Routes a program keeps. The key is the program name and playback or capture.
    pub program_locks: Vec<ProgramLock>,
    /// Mixes on this PC. Empty stays out of the file.
    pub dummies: Vec<DummyHost>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_minimized: false,
            close_quits: true,
            screen: None,
            screen_key: None,
            place: None,
            locks: Locks::default(),
            mcp_enabled: false,
            mcp_host: default_mcp_host(),
            mcp_port: default_mcp_port(),
            program_locks: Vec::new(),
            dummies: Vec::new(),
        }
    }
}

/// A mix created on this PC. Dummy in takes apps and inputs. Dummy out feeds jacks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DummyHost {
    pub name: String,
    pub slug: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outs: Vec<DummyOut>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captures: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub from_dummies: Vec<String>,
}

impl DummyHost {
    pub fn new(name: impl Into<String>, slug: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            slug: slug.into(),
            outs: Vec::new(),
            captures: Vec::new(),
            from_dummies: Vec::new(),
        }
    }
}

/// One jack Dummy out is feeding, and the source that was there before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DummyOut {
    pub slot: u16,
    pub previous: u8,
}

/// A program route the desk puts back when the stream returns or something else moves it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramLock {
    pub label: String,
    pub role: String,
    pub target: String,
}

pub const MCP_PORT: u16 = 4771;

fn default_mcp_host() -> String {
    "127.0.0.1".into()
}

fn default_mcp_port() -> u16 {
    MCP_PORT
}

/// Accept an IP address or the word localhost. Anything else is refused.
pub fn normalize_mcp_host(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type an IP address, or localhost.".into());
    }
    if text.eq_ignore_ascii_case("localhost") {
        return Ok(default_mcp_host());
    }
    let bare = text.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).unwrap_or(text);
    match bare.parse::<std::net::IpAddr>() {
        Ok(addr) => Ok(addr.to_string()),
        Err(_) => Err("Type an IP address, or localhost.".into()),
    }
}

pub fn mcp_is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1")
}

#[derive(Serialize, Deserialize)]
struct FileSettings {
    launch_at_login: bool,
    start_minimized: bool,
    close_quits: bool,
    screen: Option<i32>,
    #[serde(default)]
    screen_key: Option<String>,
    #[serde(default)]
    place: Option<[f32; 4]>,
    all_locked: bool,
    monitor_locked: bool,
    phones_locked: [bool; 2],
    trims_locked: [bool; 10],
    cell_locks: Vec<[u16; 2]>,
    bus_locks: Vec<u16>,
    #[serde(default)]
    mcp_enabled: bool,
    #[serde(default = "default_mcp_host")]
    mcp_host: String,
    #[serde(default = "default_mcp_port")]
    mcp_port: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    program_locks: Vec<ProgramLock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dummies: Vec<DummyHost>,
}

impl Settings {
    pub fn config_path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> (Self, Option<String>) {
        let path = Self::config_path();
        if !path.exists() {
            return (Self::default(), None);
        }
        match Self::load_from(&path) {
            Ok(settings) => (settings, None),
            Err(_) => (
                Self::default(),
                Some(format!("Settings could not be read, so {} is using the defaults.", crate::APP_NAME)),
            ),
        }
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let file: FileSettings = serde_json::from_str(&text).map_err(|err| err.to_string())?;
        Ok(Self {
            launch_at_login: file.launch_at_login,
            start_minimized: file.start_minimized,
            close_quits: file.close_quits,
            screen: file.screen.filter(|index| *index >= 0),
            screen_key: file.screen_key.filter(|key| !key.is_empty()),
            place: file.place.filter(|place| {
                place.iter().all(|value| value.is_finite()) && place[2] > 0.0 && place[3] > 0.0
            }),
            locks: Locks {
                all: file.all_locked,
                monitor: file.monitor_locked,
                phones: file.phones_locked,
                trims: file.trims_locked,
                cells: file.cell_locks.into_iter().map(|pair| (pair[0], pair[1])).collect(),
                buses: file.bus_locks,
            },
            mcp_enabled: file.mcp_enabled,
            mcp_host: normalize_mcp_host(&file.mcp_host).unwrap_or_else(|_| default_mcp_host()),
            mcp_port: if file.mcp_port == 0 { default_mcp_port() } else { file.mcp_port },
            program_locks: file.program_locks,
            dummies: file.dummies,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let file = FileSettings {
            launch_at_login: self.launch_at_login,
            start_minimized: self.start_minimized,
            close_quits: self.close_quits,
            screen: self.screen,
            screen_key: self.screen_key.clone(),
            place: self.place,
            all_locked: self.locks.all,
            monitor_locked: self.locks.monitor,
            phones_locked: self.locks.phones,
            trims_locked: self.locks.trims,
            cell_locks: self.locks.cells.iter().map(|(bus, input)| [*bus, *input]).collect(),
            bus_locks: self.locks.buses.clone(),
            mcp_enabled: self.mcp_enabled,
            mcp_host: self.mcp_host.clone(),
            mcp_port: self.mcp_port,
            program_locks: self.program_locks.clone(),
            dummies: self.dummies.clone(),
        };
        let body = serde_json::to_vec_pretty(&file).map_err(|err| err.to_string())?;
        write_atomic(path, &body)
    }
}

fn config_root() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config")
}

pub fn config_dir() -> PathBuf {
    config_root().join(crate::PROCESS_NAME)
}

pub fn autostart_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("autostart");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("autostart")
}

pub fn autostart_text(exec: &str, icon: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=RustMix Crimson Edition\nExec={exec}\nIcon={icon}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

/// Removes only our autostart file. A missing file is a successful disable.
pub fn set_autostart(dir: &Path, enabled: bool, text: &str) -> Result<(), String> {
    let file = dir.join(format!("{}.desktop", crate::PROCESS_NAME));
    if !enabled {
        match fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    } else {
        write_atomic(&file, text.as_bytes())
    }
}

pub fn kwin_rules_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("kwinrulesrc");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("kwinrulesrc")
}

/// `screen` None drops only the RustMix rule. A file that never mentioned
/// us is returned unchanged, so a missing choice does not create the file.
/// A rule that is already present keeps its place in `Order` and its other
/// lines. Only `screen` and `screenrule` change. screenrule 2 is KWin's Force:
/// the window stays on that screen.
pub fn set_screen_rule(text: &str, screen: Option<i32>) -> String {
    let pin = screen.filter(|index| *index >= 0);
    if pin.is_none() && !text.contains(RULE_ID) {
        return text.to_string();
    }
    let mut groups = parse_ini(text);
    if !groups.iter().any(|group| group.name == "General") {
        let insert_at = usize::from(!groups.is_empty());
        groups.insert(
            insert_at.min(groups.len()),
            Group { name: "General".into(), lines: Vec::new() },
        );
    }
    if let Some(screen) = pin {
        if let Some(general) = groups.iter_mut().find(|group| group.name == "General") {
            ensure_order_id(general, RULE_ID);
        }
        if let Some(group) = groups.iter_mut().find(|group| group.name == RULE_ID) {
            set_ini_value(group, "screen", &screen.to_string());
            set_ini_value(group, "screenrule", "2");
        } else {
            groups.push(Group {
                name: RULE_ID.into(),
                lines: vec![
                    format!("Description={RULE_ID}"),
                    format!("screen={screen}"),
                    "screenrule=2".into(),
                    "types=1".into(),
                    format!("wmclass={RULE_ID}"),
                    "wmclasscomplete=false".into(),
                    "wmclassmatch=2".into(),
                ],
            });
        }
    } else {
        if let Some(general) = groups.iter_mut().find(|group| group.name == "General") {
            drop_order_id(general, RULE_ID);
        }
        groups.retain(|group| group.name != RULE_ID);
    }
    render_ini(&groups)
}

fn ini_key(line: &str) -> Option<&str> {
    line.split_once('=').map(|(key, _)| key)
}

fn set_ini_value(group: &mut Group, key: &str, value: &str) {
    if let Some(line) = group.lines.iter_mut().find(|line| ini_key(line) == Some(key)) {
        *line = format!("{key}={value}");
    } else {
        group.lines.push(format!("{key}={value}"));
    }
}

fn order_ids(line: &str) -> Vec<String> {
    line["Order=".len()..]
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn ensure_order_id(general: &mut Group, id: &str) {
    if let Some(line) = general.lines.iter_mut().find(|line| line.starts_with("Order=")) {
        let ids = order_ids(line);
        if !ids.iter().any(|item| item == id) {
            let mut ids = ids;
            ids.push(id.to_string());
            *line = format!("Order={}", ids.join(","));
        }
    } else {
        general.lines.push(format!("Order={id}"));
    }
}

fn drop_order_id(general: &mut Group, id: &str) {
    if let Some(line) = general.lines.iter_mut().find(|line| line.starts_with("Order=")) {
        let ids: Vec<String> = order_ids(line).into_iter().filter(|item| item != id).collect();
        *line = format!("Order={}", ids.join(","));
    }
}

/// True when the file changed. A matching rule is left on disk.
pub fn write_screen_rule(screen: Option<i32>) -> Result<bool, String> {
    let path = kwin_rules_path();
    let current = fs::read_to_string(&path).unwrap_or_default();
    let next = set_screen_rule(&current, screen);
    if next == current {
        return Ok(false);
    }
    write_atomic(&path, next.as_bytes())?;
    Ok(true)
}

pub fn reconfigure_kwin() {
    let _ = std::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/KWin", "reconfigure"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn run_kwin_script(name: &str, body: &str) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("{name}.js"));
    fs::write(&path, body).map_err(|err| err.to_string())?;
    let script = path.display().to_string();
    let _ = std::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", name])
        .output();
    let load = std::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", &script, name])
        .output()
        .map_err(|err| err.to_string());
    let loaded = load.as_ref().map(|out| out.status.success()).unwrap_or(false);
    if !loaded {
        let _ = fs::remove_file(&path);
        return Err(load.err().map(|err| err).unwrap_or_else(|| "KWin did not load the window script.".into()));
    }
    let start = std::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
        .output()
        .map_err(|err| err.to_string())?;
    if !start.status.success() {
        let _ = fs::remove_file(&path);
        return Err("KWin did not run the window script.".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    let _ = std::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", name])
        .output();
    let _ = fs::remove_file(&path);
    Ok(())
}

/// KWin's frame for this window. Wayland does not tell the toolkit where the
/// window is, so the compositor's own geometry is the position we remember.
pub fn desk_frame() -> Option<Geo> {
    let pid = std::process::id();
    let since = command_text("date", &["+%Y-%m-%d %H:%M:%S"]);
    let since = since.trim();
    if since.is_empty() {
        return None;
    }
    let class = crate::PROCESS_NAME;
    let body = format!(
        "var wins = workspace.windowList();\n\
         for (var i = 0; i < wins.length; i++) {{\n\
           var w = wins[i];\n\
           if (String(w.resourceClass || \"\") === \"{class}\") {{\n\
             var g = w.frameGeometry;\n\
             print(\"CRIMSONFRAME {pid} \" + g.x + \" \" + g.y + \" \" + g.width + \" \" + g.height);\n\
           }}\n\
         }}\n"
    );
    run_kwin_script("rustmixframe", &body).ok()?;
    let log = command_text("journalctl", &["--user", "--since", since, "-t", "kwin_wayland", "--no-pager"]);
    let token = format!("CRIMSONFRAME {pid} ");
    let mut found = None;
    for line in log.lines() {
        let Some(rest) = line.split(&token).nth(1) else { continue };
        let mut nums = rest.split_whitespace();
        let x: f32 = nums.next()?.parse().ok()?;
        let y: f32 = nums.next()?.parse().ok()?;
        let w: f32 = nums.next()?.parse().ok()?;
        let h: f32 = nums.next()?.parse().ok()?;
        if x.is_finite() && y.is_finite() && w > 1.0 && h > 1.0 {
            found = Some(Geo { x, y, w, h });
        }
    }
    found
}

/// Move only this window. Other windows are left where they are.
pub fn move_desk(x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
    if ![x, y, w, h].into_iter().any(|value| !value.is_finite()) && w > 1.0 && h > 1.0 {
        let class = crate::PROCESS_NAME;
        let body = format!(
            "var wins = workspace.windowList();\n\
             for (var i = 0; i < wins.length; i++) {{\n\
               var w = wins[i];\n\
               if (String(w.resourceClass || \"\") === \"{class}\")\n\
                 w.frameGeometry = {{ x: {x:.1}, y: {y:.1}, width: {w:.1}, height: {h:.1} }};\n\
             }}\n"
        );
        return run_kwin_script("rustmixplace", &body);
    }
    Err("That screen position is not usable.".into())
}

#[derive(Clone, Debug)]
struct Group {
    name: String,
    lines: Vec<String>,
}

fn parse_ini(text: &str) -> Vec<Group> {
    let mut groups = vec![Group { name: String::new(), lines: Vec::new() }];
    for line in text.replace("\r\n", "\n").lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            groups.push(Group { name: name.to_string(), lines: Vec::new() });
        } else if let Some(group) = groups.last_mut() {
            group.lines.push(line.to_string());
        }
    }
    groups
}

fn render_ini(groups: &[Group]) -> String {
    let mut parts = Vec::new();
    for group in groups {
        if !group.name.is_empty() {
            parts.push(format!("[{}]", group.name));
        }
        parts.extend(group.lines.iter().cloned());
    }
    while parts.last().is_some_and(|line| line.is_empty()) {
        parts.pop();
    }
    if parts.is_empty() {
        return String::new();
    }
    let mut text = parts.join("\n");
    text.push('\n');
    text
}

pub fn write_atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp).map_err(|err| err.to_string())?;
        file.write_all(body).map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|err| err.to_string())?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub name: String,
    pub label: String,
    /// kscreen id, or the output name, or its geometry when neither is known.
    pub key: String,
    pub kwin_index: Option<i32>,
    pub geo: Geo,
}

pub fn parse_kscreen(text: &str) -> Vec<KScreen> {
    let clean = strip_ansi(text);
    let mut blocks = Vec::new();
    let mut current = String::new();
    for line in clean.lines() {
        if line.starts_with("Output:") && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        if line.starts_with("Output:") || !current.is_empty() {
            current.push_str(line);
            current.push('\n');
        }
    }
    if current.contains("Geometry:") {
        blocks.push(current);
    }
    let mut outputs = Vec::new();
    for block in blocks {
        let header = block.lines().next().unwrap_or("");
        let mut parts = header.split_whitespace();
        let _label = parts.next();
        let _number = parts.next();
        let name = parts.next().unwrap_or("").to_string();
        let uuid = parts.next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let Some(geo) = geometry_after(&block, "Geometry:") else { continue };
        outputs.push(KScreen { name, uuid, geo, mode: mode_of(&block) });
    }
    outputs
}

#[derive(Clone, Debug)]
pub struct KScreen {
    pub name: String,
    pub uuid: String,
    pub geo: Geo,
    pub mode: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geo {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn geometry_after(block: &str, key: &str) -> Option<Geo> {
    let line = block.lines().find(|line| line.contains(key))?;
    let rest = line.split(key).nth(1)?.trim();
    // "0,1235 1098x618" or "0,0,2194x1234"
    let rest = rest.replace(' ', ",");
    let mut nums = rest.split(|c: char| c == ',' || c == 'x');
    let x: f32 = nums.next()?.parse().ok()?;
    let y: f32 = nums.next()?.parse().ok()?;
    let w: f32 = nums.next()?.parse().ok()?;
    let h: f32 = nums.next()?.parse().ok()?;
    Some(Geo { x, y, w, h })
}

fn mode_of(block: &str) -> String {
    for token in block.split_whitespace() {
        if !token.contains('!') {
            continue;
        }
        let size = token.split('@').next().unwrap_or(token);
        let size = size.rsplit(':').next().unwrap_or(size);
        if size.contains('x') {
            return size.replace('x', "×");
        }
    }
    String::new()
}

pub fn parse_kwin_screens(text: &str) -> Vec<(i32, String, Geo)> {
    let mut screens = Vec::new();
    let mut current: Option<(i32, String, Option<Geo>)> = None;
    for line in strip_ansi(text).lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Screen ") {
            if let Some((index, name, geo)) = current.take() {
                if let Some(geo) = geo {
                    if !name.is_empty() {
                        screens.push((index, name, geo));
                    }
                }
            }
            let number: i32 = rest.trim_end_matches(':').parse().unwrap_or(-1);
            if number >= 0 && rest.ends_with(':') && !rest.contains(' ') {
                current = Some((number, String::new(), None));
            } else {
                current = None;
            }
        } else if let Some((_, name, geo)) = current.as_mut() {
            if let Some(value) = trimmed.strip_prefix("Name:") {
                *name = value.trim().to_string();
            }
            if trimmed.starts_with("Geometry:") {
                *geo = geometry_after(trimmed, "Geometry:");
            }
        }
    }
    if let Some((index, name, Some(geo))) = current {
        if !name.is_empty() {
            screens.push((index, name, geo));
        }
    }
    screens
}

pub fn screens_from(kscreen: &str, kwin: &str) -> Vec<Screen> {
    let outputs = parse_kscreen(kscreen);
    let kwin_screens = parse_kwin_screens(kwin);
    let list: Vec<Screen> = outputs
        .iter()
        .map(|output| {
            let kwin_index = kwin_screens
                .iter()
                .find(|(_, name, geo)| name == &output.name || centers_match(*geo, output.geo))
                .map(|(index, _, _)| *index);
            Screen {
                name: output.name.clone(),
                label: output_label(output, &outputs),
                key: screen_key(output),
                kwin_index,
                geo: output.geo,
            }
        })
        .collect();
    list
}

fn screen_key(output: &KScreen) -> String {
    if !output.uuid.is_empty() {
        output.uuid.clone()
    } else if !output.name.is_empty() {
        output.name.clone()
    } else {
        format!("{},{} {}x{}", output.geo.x, output.geo.y, output.geo.w, output.geo.h)
    }
}

pub struct ScreenPlan {
    pub screen_key: Option<String>,
    pub screen: Option<i32>,
    /// KWin index to force. None leaves the rule file alone.
    pub pin: Option<i32>,
}

/// The saved key wins. An index with no key is copied onto that output once.
/// One connected screen becomes the lock. An unplugged key is kept and is not
/// aimed at a different output. An empty list changes nothing.
pub fn plan_screen(settings: &Settings, screens: &[Screen]) -> ScreenPlan {
    if screens.is_empty() {
        return ScreenPlan {
            screen_key: settings.screen_key.clone(),
            screen: settings.screen,
            pin: None,
        };
    }
    let mut key = settings.screen_key.clone();
    if key.is_none() {
        if let Some(saved) = settings.screen {
            if let Some(screen) = screens.iter().find(|screen| screen.kwin_index == Some(saved)) {
                key = Some(screen.key.clone());
            }
        } else if screens.len() == 1 {
            key = Some(screens[0].key.clone());
        }
    }
    if let Some(locked) = key {
        if let Some(screen) = screens.iter().find(|screen| screen.key == locked) {
            return ScreenPlan {
                screen_key: Some(locked),
                screen: screen.kwin_index,
                pin: screen.kwin_index,
            };
        }
        return ScreenPlan { screen_key: Some(locked), screen: None, pin: None };
    }
    ScreenPlan { screen_key: None, screen: settings.screen, pin: None }
}

/// Returns whether the saved screen fields changed. The rule is written only
/// when the locked screen is connected.
pub fn apply_screen_plan(settings: &mut Settings, screens: &[Screen]) -> Result<bool, String> {
    let plan = plan_screen(settings, screens);
    let changed = settings.screen_key != plan.screen_key || settings.screen != plan.screen;
    settings.screen_key = plan.screen_key;
    settings.screen = plan.screen;
    if let Some(index) = plan.pin
        && write_screen_rule(Some(index))?
    {
        reconfigure_kwin();
    }
    Ok(changed)
}

/// Where the window should open. The locked screen, or the top-left screen
/// when that one is unplugged. `place` is relative to the screen.
pub fn open_frame(settings: &Settings, screens: &[Screen]) -> Option<FrameHint> {
    if screens.is_empty() {
        return None;
    }
    let locked = settings
        .screen_key
        .as_ref()
        .and_then(|key| screens.iter().find(|screen| &screen.key == key));
    let screen = locked.or_else(|| primary_screen(screens))?;
    let mut hint = FrameHint { x: screen.geo.x, y: screen.geo.y, w: None, h: None };
    if locked.is_some()
        && let Some(place) = settings.place
    {
        let w = place[2];
        let h = place[3];
        let fit_w = w >= 320.0 && w <= screen.geo.w + 2.0;
        let fit_h = h >= 240.0 && h <= screen.geo.h + 2.0;
        let used_w = if fit_w { w } else { 1040.0_f32.min(screen.geo.w) };
        let used_h = if fit_h { h } else { 480.0_f32.min(screen.geo.h) };
        if fit_w && fit_h {
            hint.w = Some(w);
            hint.h = Some(h);
        }
        hint.x = screen.geo.x + place[0].clamp(0.0, (screen.geo.w - used_w).max(0.0));
        hint.y = screen.geo.y + place[1].clamp(0.0, (screen.geo.h - used_h).max(0.0));
    }
    Some(hint)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameHint {
    pub x: f32,
    pub y: f32,
    pub w: Option<f32>,
    pub h: Option<f32>,
}

fn primary_screen(screens: &[Screen]) -> Option<&Screen> {
    screens.iter().min_by(|left, right| {
        left.geo
            .y
            .total_cmp(&right.geo.y)
            .then(left.geo.x.total_cmp(&right.geo.x))
            .then(left.name.cmp(&right.name))
    })
}

pub fn contains_center(screen: &Screen, x: f32, y: f32) -> bool {
    let geo = screen.geo;
    x >= geo.x - 2.0 && x < geo.x + geo.w + 2.0 && y >= geo.y - 2.0 && y < geo.y + geo.h + 2.0
}

/// Position and size relative to `screen`, when the window's center is on it.
pub fn place_on(screen: &Screen, x: f32, y: f32, w: f32, h: f32) -> Option<[f32; 4]> {
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return None;
    }
    if !contains_center(screen, x + w / 2.0, y + h / 2.0) {
        return None;
    }
    Some([x - screen.geo.x, y - screen.geo.y, w, h])
}

fn centers_match(a: Geo, b: Geo) -> bool {
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    (ax - bx).abs() <= 2.0 && (ay - by).abs() <= 2.0
}

fn output_label(output: &KScreen, all: &[KScreen]) -> String {
    if all.len() < 2 {
        return if output.mode.is_empty() {
            "Main screen".into()
        } else {
            format!("{} · Main screen · {}", output.name, output.mode)
        };
    }
    let primary = all.iter().min_by(|a, b| a.geo.y.total_cmp(&b.geo.y).then(a.geo.x.total_cmp(&b.geo.x)));
    let place = if primary.is_some_and(|main| main.name == output.name) {
        "Main screen"
    } else if let Some(main) = primary {
        let dy = output.geo.y - main.geo.y;
        let dx = output.geo.x - main.geo.x;
        if dy.abs() >= dx.abs() {
            if dy >= 0.0 { "Lower screen" } else { "Upper screen" }
        } else if dx >= 0.0 {
            "Screen on the right"
        } else {
            "Screen on the left"
        }
    } else {
        "Other screen"
    };
    if output.mode.is_empty() {
        format!("{} · {place}", output.name)
    } else {
        format!("{} · {place} · {}", output.name, output.mode)
    }
}

pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1B && index + 1 < bytes.len() && bytes[index + 1] == b'[' {
            index += 2;
            while index < bytes.len() && !bytes[index].is_ascii_alphabetic() {
                index += 1;
            }
            index += 1;
        } else {
            out.push(bytes[index] as char);
            index += 1;
        }
    }
    out
}

pub fn load_screens() -> Vec<Screen> {
    let kscreen = command_text("kscreen-doctor", &["-o"]);
    let kwin = command_text("qdbus6", &["org.kde.KWin", "/KWin", "supportInformation"]);
    screens_from(&kscreen, &kwin)
}

fn command_text(program: &str, args: &[&str]) -> String {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_rules() -> &'static str {
        "\
[General]
Order=RustMix,cuemix-crimson

[RustMix]
Description=RustMix
screen=1
screenrule=2
types=1
wmclass=RustMix
wmclasscomplete=false
wmclassmatch=2

[cuemix-crimson]
Description=CueMix Crimson Edition
screen=1
screenrule=2
types=1
wmclass=com.motu.CueMix5
wmclasscomplete=false
wmclassmatch=2
"
    }

    #[test]
    fn kwin_rule_keeps_steam_and_an_untouched_file_stays() {
        let original = "[General]\nOrder=steam\n\n[steam]\nDescription=Steam\n";
        assert_eq!(set_screen_rule(original, None), original);
        let pinned = set_screen_rule(original, Some(0));
        assert!(pinned.contains("Order=steam,RustMix"), "{pinned}");
        assert!(pinned.contains("[steam]"), "{pinned}");
        assert!(pinned.contains("Description=RustMix"), "{pinned}");
        assert!(pinned.contains("wmclass=RustMix"), "{pinned}");
        assert!(pinned.contains("screenrule=2"), "{pinned}");
        assert!(!pinned.contains("screenrule=3"), "{pinned}");
        let cleared = set_screen_rule(&pinned, None);
        assert!(cleared.contains("Order=steam\n"), "{cleared}");
        assert!(!cleared.contains("RustMix"), "{cleared}");
        assert!(cleared.contains("[steam]"), "{cleared}");
    }

    #[test]
    fn an_existing_rule_keeps_its_order_and_forces_the_screen() {
        let original = "\
[General]
Order=steam,RustMix,cuemix-crimson

[steam]
Description=Steam

[RustMix]
Description=RustMix
screen=1
screenrule=3
types=1
wmclass=RustMix
wmclasscomplete=false
wmclassmatch=2

[cuemix-crimson]
Description=CueMix Crimson Edition
screen=1
screenrule=3
types=1
wmclass=com.motu.CueMix5
wmclasscomplete=false
wmclassmatch=2
";
        let pinned = set_screen_rule(original, Some(0));
        assert!(pinned.contains("Order=steam,RustMix,cuemix-crimson\n"), "{pinned}");
        assert!(pinned.find("[RustMix]").unwrap() < pinned.find("[cuemix-crimson]").unwrap());
        let desk = pinned.split("[cuemix-crimson]").next().unwrap();
        let desk = desk.split("[RustMix]").nth(1).unwrap();
        assert!(desk.contains("Description=RustMix\n"), "{desk}");
        assert!(desk.contains("screen=0\n"), "{desk}");
        assert!(desk.contains("screenrule=2\n"), "{desk}");
        assert!(!desk.contains("screenrule=3"), "{desk}");
        assert!(desk.contains("wmclass=RustMix\n"), "{desk}");
        let cue = pinned.split("[cuemix-crimson]").nth(1).unwrap();
        assert!(cue.contains("screen=1\n"), "{cue}");
        assert!(cue.contains("screenrule=3\n"), "{cue}");
        assert!(cue.contains("wmclass=com.motu.CueMix5\n"), "{cue}");
        assert!(cue.contains("Description=CueMix Crimson Edition\n"), "{cue}");
        assert_eq!(set_screen_rule(&pinned, Some(0)), pinned);
        assert_eq!(set_screen_rule(live_rules(), Some(1)), live_rules());
        let cleared = set_screen_rule(original, None);
        assert!(cleared.contains("[steam]\n"), "{cleared}");
        assert!(cleared.contains("[cuemix-crimson]\n"), "{cleared}");
        assert!(cleared.contains("wmclass=com.motu.CueMix5\n"), "{cleared}");
        assert!(!cleared.contains("RustMix"), "{cleared}");
    }

    #[test]
    fn screens_join_by_name() {
        let kscreen = "\
Output: 1 DP-2 4d10045a-9150-48ff-81af-e2121787f42d
\tGeometry: 0,1235 1098x618
\tModes:  1:1920x1080@60.00*!
Output: 2 DP-1 45674e9c-c03c-4a38-9ec1-b09806ca9dca
\tGeometry: 0,0 2194x1234
\tModes:  12:3840x2160@60.00*!
";
        let kwin = "\
Screen Edges
============
Screen 0:
---------
Name: DP-1
Geometry: 0,0,2194x1234
Screen 1:
---------
Name: DP-2
Geometry: 0,1235,1097x617
";
        let screens = screens_from(kscreen, kwin);
        let dp1 = screens.iter().find(|screen| screen.name == "DP-1").unwrap();
        let dp2 = screens.iter().find(|screen| screen.name == "DP-2").unwrap();
        assert_eq!(dp1.kwin_index, Some(0));
        assert_eq!(dp2.kwin_index, Some(1));
        assert_eq!(dp1.key, "45674e9c-c03c-4a38-9ec1-b09806ca9dca");
        assert_eq!(dp2.key, "4d10045a-9150-48ff-81af-e2121787f42d");
        assert_eq!(dp2.geo.y, 1235.0);
        assert!(dp1.label.contains("Main"));
        assert!(dp2.label.contains("Lower"));
    }

    #[test]
    fn one_screen_keeps_its_index() {
        let kscreen = "\
Output: 1 HDMI-A-1 aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee
\tGeometry: 0,0 1920x1080
\tModes:  1:1920x1080@60.00*!
";
        let kwin = "\
Screen 0:
Name: HDMI-A-1
Geometry: 0,0,1920x1080
";
        let screens = screens_from(kscreen, kwin);
        assert_eq!(screens.len(), 1);
        assert_eq!(screens[0].kwin_index, Some(0));
        assert_eq!(screens[0].key, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        assert!(screens[0].label.contains("Main"));
    }

    #[test]
    fn a_screen_to_the_right_is_labeled_from_its_place() {
        let kscreen = "\
Output: 1 DP-1 11111111-1111-1111-1111-111111111111
\tGeometry: 0,0 1000x800
\tModes:  1:1920x1080@60*!
Output: 2 HDMI-A-1 33333333-3333-3333-3333-333333333333
\tGeometry: 1000,0 800x600
\tModes:  1:1600x900@60*!
Output: 3 DP-2 22222222-2222-2222-2222-222222222222
\tGeometry: 0,800 1000x600
\tModes:  1:1920x1080@60*!
";
        let kwin = "\
Screen 0:
Name: DP-1
Geometry: 0,0,1000x800
Screen 1:
Name: DP-2
Geometry: 0,800,1000x600
Screen 2:
Name: HDMI-A-1
Geometry: 1000,0,800x600
";
        let screens = screens_from(kscreen, kwin);
        let hdmi = screens.iter().find(|screen| screen.name == "HDMI-A-1").unwrap();
        let lower = screens.iter().find(|screen| screen.name == "DP-2").unwrap();
        assert_eq!(hdmi.kwin_index, Some(2));
        assert!(hdmi.label.contains("right"), "{}", hdmi.label);
        assert_eq!(lower.kwin_index, Some(1));
        assert!(lower.label.contains("Lower"), "{}", lower.label);
    }

    fn fake_screen(name: &str, key: &str, index: i32, x: f32, y: f32) -> Screen {
        Screen {
            name: name.into(),
            label: name.into(),
            key: key.into(),
            kwin_index: Some(index),
            geo: Geo { x, y, w: 2000.0, h: 1200.0 },
        }
    }

    #[test]
    fn the_saved_index_adopts_that_output_and_an_unplugged_key_is_not_moved() {
        let screens = vec![
            fake_screen("DP-1", "aaa", 0, 0.0, 0.0),
            fake_screen("DP-2", "bbb", 1, 0.0, 1235.0),
        ];
        let mut settings = Settings::default();
        settings.screen = Some(1);
        let plan = plan_screen(&settings, &screens);
        assert_eq!(plan.screen_key.as_deref(), Some("bbb"));
        assert_eq!(plan.screen, Some(1));
        assert_eq!(plan.pin, Some(1));

        settings.screen_key = Some("gone".into());
        settings.screen = Some(1);
        let parked = plan_screen(&settings, &screens);
        assert_eq!(parked.screen_key.as_deref(), Some("gone"));
        assert_eq!(parked.screen, None);
        assert_eq!(parked.pin, None);

        settings.screen_key = Some("bbb".into());
        settings.screen = Some(1);
        let renumbered = vec![fake_screen("DP-2", "bbb", 0, 0.0, 0.0)];
        let moved = plan_screen(&settings, &renumbered);
        assert_eq!(moved.screen, Some(0));
        assert_eq!(moved.pin, Some(0));

        let only = plan_screen(&Settings::default(), &renumbered);
        assert_eq!(only.screen_key.as_deref(), Some("bbb"));
        assert_eq!(only.pin, Some(0));

        let quiet = plan_screen(&settings, &[]);
        assert_eq!(quiet.screen, Some(1));
        assert_eq!(quiet.pin, None);
        assert_eq!(quiet.screen_key.as_deref(), Some("bbb"));
    }

    #[test]
    fn the_window_opens_on_the_locked_screen_in_its_saved_place() {
        let screens = vec![
            fake_screen("DP-1", "aaa", 0, 0.0, 0.0),
            fake_screen("DP-2", "bbb", 1, 0.0, 1235.0),
        ];
        let mut settings = Settings::default();
        settings.screen_key = Some("bbb".into());
        settings.place = Some([12.0, 8.0, 900.0, 500.0]);
        let hint = open_frame(&settings, &screens).unwrap();
        assert_eq!(hint.x, 12.0);
        assert_eq!(hint.y, 1243.0);
        assert_eq!(hint.w, Some(900.0));
        assert_eq!(hint.h, Some(500.0));

        settings.screen_key = Some("gone".into());
        let landed = open_frame(&settings, &screens).unwrap();
        assert_eq!((landed.x, landed.y), (0.0, 0.0));
        assert_eq!(landed.w, None);

        let lower = screens.iter().find(|screen| screen.key == "bbb").unwrap();
        assert_eq!(place_on(lower, 0.0, 1235.0, 1097.0, 581.0), Some([0.0, 0.0, 1097.0, 581.0]));
        assert!(place_on(lower, 0.0, 0.0, 800.0, 400.0).is_none());
    }

    #[test]
    fn a_saved_index_loads_without_a_screen_key() {
        let dir = std::env::temp_dir().join(format!("rustmix-old-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"launch_at_login":false,"start_minimized":false,"close_quits":true,"screen":1,"all_locked":false,"monitor_locked":true,"phones_locked":[false,false],"trims_locked":[false,false,false,false,false,false,false,false,false,false],"cell_locks":[],"bus_locks":[]}"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.screen, Some(1));
        assert_eq!(loaded.screen_key, None);
        assert_eq!(loaded.place, None);
        assert!(loaded.close_quits);
        assert!(loaded.locks.monitor);
        assert!(!loaded.mcp_enabled);
        assert_eq!(loaded.mcp_host, "127.0.0.1");
        assert_eq!(loaded.mcp_port, MCP_PORT);
        assert!(loaded.program_locks.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_settings_are_not_created_by_a_load() {
        let dir = std::env::temp_dir().join(format!("rustmix-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        assert!(Settings::load_from(&path).is_err());
        assert!(!path.exists());
        let mut settings = Settings::default();
        settings.locks.monitor = true;
        settings.save_to(&path).unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert!(loaded.locks.monitor);
        assert!(loaded.close_quits);
        assert!(!loaded.mcp_enabled);
        assert_eq!(loaded.mcp_host, "127.0.0.1");
        assert_eq!(loaded.mcp_port, MCP_PORT);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_stays_off_when_the_keys_are_missing_and_round_trips_when_set() {
        let dir = std::env::temp_dir().join(format!("rustmix-mcp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"launch_at_login":false,"start_minimized":false,"close_quits":true,"screen":null,"all_locked":false,"monitor_locked":false,"phones_locked":[false,false],"trims_locked":[false,false,false,false,false,false,false,false,false,false],"cell_locks":[],"bus_locks":[]}"#).unwrap();
        let mut loaded = Settings::load_from(&path).unwrap();
        assert!(!loaded.mcp_enabled);
        assert_eq!(loaded.mcp_port, 4771);
        assert!(normalize_mcp_host("localhost").unwrap() == "127.0.0.1");
        assert!(normalize_mcp_host("::1").unwrap() == "::1");
        assert!(normalize_mcp_host("0.0.0.0").is_ok());
        assert!(normalize_mcp_host("mixer.local").is_err());
        assert!(mcp_is_loopback("127.0.0.1"));
        assert!(!mcp_is_loopback("0.0.0.0"));
        loaded.mcp_enabled = true;
        loaded.mcp_host = normalize_mcp_host("[::1]").unwrap();
        loaded.mcp_port = 4800;
        loaded.save_to(&path).unwrap();
        let again = Settings::load_from(&path).unwrap();
        assert!(again.mcp_enabled);
        assert_eq!(again.mcp_host, "::1");
        assert_eq!(again.mcp_port, 4800);
        assert!(again.close_quits);
        assert!(again.program_locks.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_program_lock_round_trips_and_stays_out_of_an_empty_file() {
        let dir = std::env::temp_dir().join(format!("rustmix-locks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"launch_at_login":false,"start_minimized":false,"close_quits":true,"screen":1,"all_locked":false,"monitor_locked":false,"phones_locked":[false,false],"trims_locked":[false,false,false,false,false,false,false,false,false,false],"cell_locks":[],"bus_locks":[]}"#,
        )
        .unwrap();
        let mut loaded = Settings::load_from(&path).unwrap();
        assert!(loaded.program_locks.is_empty());
        loaded.save_to(&path).unwrap();
        let bare = fs::read_to_string(&path).unwrap();
        assert!(!bare.contains("program_locks"), "{bare}");
        assert!(!bare.contains("dummies"), "{bare}");
        loaded.program_locks.push(ProgramLock {
            label: "LibreWolf".into(),
            role: "playback".into(),
            target: "Line1".into(),
        });
        loaded.save_to(&path).unwrap();
        let again = Settings::load_from(&path).unwrap();
        assert_eq!(again.program_locks, loaded.program_locks);
        assert!(fs::read_to_string(&path).unwrap().contains("program_locks"));
        loaded.dummies.push(DummyHost::new("Dummy", "dummy"));
        loaded.save_to(&path).unwrap();
        let with_dummy = fs::read_to_string(&path).unwrap();
        assert!(with_dummy.contains("dummies"), "{with_dummy}");
        assert!(!with_dummy.contains("outs"), "{with_dummy}");
        let again = Settings::load_from(&path).unwrap();
        assert_eq!(again.dummies[0].name, "Dummy");
        assert!(again.dummies[0].outs.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}

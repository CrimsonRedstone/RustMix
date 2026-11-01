//! Programs on this PC. A move is a pactl request plus, when the jack needs
//! it, the matching patch. The null sink is created only when a program is
//! sent to another program, and it is never written into a PipeWire config.

use crate::topology::{self, CapDev, PlayPair};
use serde_json::Value;
use std::process::Command;

pub const DESK_SINK: &str = "RustMix";

#[derive(Clone, Debug)]
pub struct Program {
    pub id: String,
    pub label: String,
    pub role: Role,
    pub target: String,
    pub pactl_index: u64,
    /// PipeWire node id. Zero means the list only has the Pulse stream index.
    pub node_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Playback,
    Capture,
}

impl Role {
    pub fn token(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Capture => "capture",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Move {
    pub args: Vec<String>,
    pub patches: Vec<(u16, u8)>,
    pub load_sink: bool,
}

pub trait Runner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<CmdOut, String>;
}

#[derive(Clone, Debug)]
pub struct CmdOut {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct PactlRunner;

impl Runner for PactlRunner {
    fn run(&self, args: &[String]) -> Result<CmdOut, String> {
        // A native stream is moved with pw-metadata. pactl only sees Pulse clients.
        let (program, args) = match args.first().map(String::as_str) {
            Some("pw-dump" | "pw-metadata" | "pw-link") => (args[0].as_str(), &args[1..]),
            _ => ("pactl", args),
        };
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|_| "PipeWire control (pactl) is not available.".to_string())?;
        Ok(CmdOut {
            code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

pub fn refresh(runner: &dyn Runner) -> Result<Vec<Program>, String> {
    let sinks = json_list(runner, &["-f", "json", "list", "sinks"])?;
    let sources = json_list(runner, &["-f", "json", "list", "sources"])?;
    let playback = json_list(runner, &["-f", "json", "list", "sink-inputs"])?;
    let capture = json_list(runner, &["-f", "json", "list", "source-outputs"])?;
    let sink_names = index_names(&sinks);
    let source_names = index_names(&sources);
    let mut programs = Vec::new();
    if let Some(items) = playback.as_array() {
        for item in items {
            if let Some(program) = stream_record(item, Role::Playback, &sink_names) {
                programs.push(program);
            }
        }
    }
    if let Some(items) = capture.as_array() {
        for item in items {
            if let Some(program) = stream_record(item, Role::Capture, &source_names) {
                programs.push(program);
            }
        }
    }
    if let Ok(dump) = runner.run(&["pw-dump".into()]) {
        if dump.code == 0 {
            if let Ok(value) = serde_json::from_str::<Value>(&dump.stdout) {
                merge_nodes(&mut programs, &value);
            }
        }
    }
    programs.sort_by(|a, b| a.label.cmp(&b.label).then(a.id.cmp(&b.id)));
    Ok(programs)
}

pub fn plan_move(program: &Program, target: &str) -> Result<Move, String> {
    if let Some(name) = target.strip_prefix("sink:") {
        return Ok(Move {
            args: vec!["move-sink-input".into(), program.pactl_index.to_string(), name.to_string()],
            patches: Vec::new(),
            load_sink: false,
        });
    }
    if let Some(name) = target.strip_prefix("monitor:") {
        if program.role != Role::Capture {
            return Err("That program plays audio, so it needs a playback target.".into());
        }
        return Ok(Move {
            args: vec!["move-source-output".into(), program.pactl_index.to_string(), format!("{name}.monitor")],
            patches: Vec::new(),
            load_sink: false,
        });
    }
    if target == "desk" {
        let args = match program.role {
            Role::Playback => vec!["move-sink-input".into(), program.pactl_index.to_string(), DESK_SINK.into()],
            Role::Capture => vec!["move-source-output".into(), program.pactl_index.to_string(), format!("{DESK_SINK}.monitor")],
        };
        return Ok(Move { args, patches: Vec::new(), load_sink: true });
    }
    if target == "default" {
        let args = match program.role {
            Role::Playback => vec!["move-sink-input".into(), program.pactl_index.to_string(), "@DEFAULT_SINK@".into()],
            Role::Capture => vec!["move-source-output".into(), program.pactl_index.to_string(), "@DEFAULT_SOURCE@".into()],
        };
        return Ok(Move { args, patches: Vec::new(), load_sink: false });
    }
    match program.role {
        Role::Playback => {
            let pair = playback(target).ok_or_else(|| "That program plays audio, so it needs a playback target.".to_string())?;
            Ok(Move {
                args: vec!["move-sink-input".into(), program.pactl_index.to_string(), node_hint(&pair.suffix)],
                patches: vec![(pair.slots[0], pair.usb[0]), (pair.slots[1], pair.usb[1])],
                load_sink: false,
            })
        }
        Role::Capture => {
            let device = capture(target).ok_or_else(|| "That program records audio, so it needs a capture target.".to_string())?;
            let mut patches = Vec::new();
            for index in 0..device.slot_len as usize {
                if (index as u8) < device.code_len {
                    patches.push((device.slots[index], device.codes[index]));
                }
            }
            Ok(Move {
                args: vec!["move-source-output".into(), program.pactl_index.to_string(), node_hint(&device.suffix)],
                patches,
                load_sink: false,
            })
        }
    }
}

/// The pactl argument is the node name when we know it, otherwise a suffix
/// the caller resolves against the live list.
pub fn resolve_node(nodes: &Value, suffix: &str) -> Option<String> {
    let items = nodes.as_array()?;
    items.iter().find_map(|item| {
        let name = item.get("name").and_then(|value| value.as_str()).unwrap_or("");
        if name == DESK_SINK || name == format!("{DESK_SINK}.monitor") {
            return None;
        }
        if name.contains("usb-MOTU_828_") && name.ends_with(suffix) && !name.contains(".split") {
            Some(name.to_string())
        } else {
            None
        }
    })
}

pub fn sink_loaded(sinks: &Value) -> bool {
    sinks
        .as_array()
        .map(|items| items.iter().any(|item| item.get("name").and_then(|v| v.as_str()) == Some(DESK_SINK)))
        .unwrap_or(false)
}

pub fn load_sink_args() -> Vec<String> {
    load_sink_args_named(DESK_SINK, "RustMix")
}

pub fn load_sink_args_named(name: &str, description: &str) -> Vec<String> {
    vec![
        "load-module".into(),
        "module-null-sink".into(),
        format!("sink_name={name}"),
        format!("sink_properties=device.description={description}"),
    ]
}

pub fn sink_slug(label: &str) -> String {
    let mut out = String::new();
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.len() > 24 {
        out.truncate(24);
    }
    out
}

pub fn sink_loaded_name(sinks: &Value, name: &str) -> bool {
    sinks
        .as_array()
        .map(|items| items.iter().any(|item| item.get("name").and_then(|v| v.as_str()) == Some(name)))
        .unwrap_or(false)
}

/// Asks before a route replaces what an output is playing, or sends audio
/// toward speakers, phones, S/PDIF, or another program.
pub fn route_warning(label: &str, current: &str, target: &str, patch: &[u8]) -> Option<String> {
    if let Some(slug) = target.strip_prefix("hear:") {
        let hear = hear_name(slug);
        return Some(format!("Send {label} to {hear}? {hear} will hear {label} instead of its current input."));
    }
    if target == "desk" {
        return Some(format!("Send {label} to another program? It leaves the output it is using now."));
    }
    let pair = playback(target)?;
    let mut changes = Vec::new();
    for (index, slot) in pair.slots.iter().enumerate() {
        let have = patch.get(*slot as usize).copied().unwrap_or(127);
        if have != pair.usb[index] && have < 127 {
            changes.push(format!("{} is playing {}", jack_phrase(*slot), topology::source_label(have)));
        }
    }
    let heard = matches!(target, "Line1" | "Headphones1" | "Headphones2" | "SPDIF1" | "SPDIF2");
    if changes.is_empty() && !heard {
        return None;
    }
    let mut text = format!("Send {label} to {}?", pair.label);
    if changes.is_empty() {
        text.push_str(&format!(" That output will play {label}."));
    } else {
        text.push(' ');
        text.push_str(&changes.join(". "));
        text.push_str(". Those routes will change.");
    }
    if current == "Line1" && target != "Line1" {
        text.push_str(&format!(" {label} will leave Main."));
    }
    Some(text)
}

fn hear_name(slug: &str) -> String {
    match slug {
        "obs" | "obs_studio" => "OBS".into(),
        "discord" => "Discord".into(),
        _ => slug
            .split('_')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Same question as `route_warning`. A custom output name asks even when the
/// pair is already the program's USB source, so a named jack is not replaced quietly.
pub fn route_warning_named(label: &str, current: &str, target: &str, patch: &[u8], custom: &str) -> Option<String> {
    let custom = custom.trim();
    match route_warning(label, current, target, patch) {
        Some(mut text) => {
            if !custom.is_empty() && !text.contains(custom) {
                text.push_str(&format!(" That output is named {custom}."));
            }
            Some(text)
        }
        None if !custom.is_empty() => {
            let place = playback(target).map(|pair| pair.label.to_string()).unwrap_or_else(|| target.to_string());
            Some(format!("Send {label} to {place}? That output is named {custom}."))
        }
        None => None,
    }
}

fn jack_phrase(slot: u16) -> String {
    let name = topology::destination_name(slot);
    match name.as_str() {
        "Main L" => "Main 1".into(),
        "Main R" => "Main 2".into(),
        "Line 3" | "Line 4" | "Line 5" | "Line 6" | "Line 7" | "Line 8" | "Line 9" | "Line 10" => {
            format!("Line out {}", &name[5..])
        }
        other => other.to_string(),
    }
}

fn merge_nodes(programs: &mut Vec<Program>, dump: &Value) {
    let Some(items) = dump.as_array() else { return };
    let mut names = std::collections::HashMap::<u64, String>::new();
    struct Stream {
        id: u64,
        serial: u64,
        role: Role,
        binary: String,
        application: String,
    }
    let mut streams = Vec::new();
    for item in items {
        if item.get("type").and_then(|value| value.as_str()) != Some("PipeWire:Interface:Node") {
            continue;
        }
        let id = item.get("id").and_then(|value| value.as_u64()).unwrap_or(0);
        let props = item.get("info").and_then(|value| value.get("props"));
        let name = props.and_then(|value| value.get("node.name")).and_then(|value| value.as_str()).unwrap_or("");
        if id == 0 || name.is_empty() {
            continue;
        }
        names.insert(id, name.to_string());
        let class = props.and_then(|value| value.get("media.class")).and_then(|value| value.as_str()).unwrap_or("");
        let role = match class {
            "Stream/Output/Audio" => Role::Playback,
            "Stream/Input/Audio" => Role::Capture,
            _ => continue,
        };
        if name.contains(".split") {
            continue;
        }
        let binary = props.and_then(|value| value.get("application.process.binary")).and_then(|value| value.as_str()).unwrap_or("");
        let application = props.and_then(|value| value.get("application.name")).and_then(|value| value.as_str()).unwrap_or("");
        if skip_program(binary, application) {
            continue;
        }
        let serial = props.and_then(|value| value.get("object.serial")).and_then(|value| value.as_u64()).unwrap_or(0);
        streams.push(Stream { id, serial, role, binary: binary.to_string(), application: application.to_string() });
    }
    let mut linked = std::collections::HashMap::<u64, String>::new();
    for item in items {
        if item.get("type").and_then(|value| value.as_str()) != Some("PipeWire:Interface:Link") {
            continue;
        }
        let info = item.get("info");
        let out_id = info.and_then(|value| value.get("output-node-id")).and_then(|value| value.as_u64()).unwrap_or(0);
        let in_id = info.and_then(|value| value.get("input-node-id")).and_then(|value| value.as_u64()).unwrap_or(0);
        let out_name = names.get(&out_id);
        let in_name = names.get(&in_id);
        if streams.iter().any(|stream| stream.id == out_id) {
            if let Some(name) = in_name {
                linked.insert(out_id, name.clone());
            }
        }
        if streams.iter().any(|stream| stream.id == in_id) {
            if let Some(name) = out_name {
                linked.insert(in_id, name.clone());
            }
        }
    }
    for stream in streams {
        let endpoint = linked.get(&stream.id).map(String::as_str).unwrap_or("");
        let target = logical_target(endpoint);
        if stream.serial != 0 {
            if let Some(program) = programs.iter_mut().find(|program| program.pactl_index == stream.serial && program.role == stream.role) {
                program.node_id = stream.id;
                continue;
            }
        }
        if programs.iter().any(|program| program.node_id == stream.id) {
            continue;
        }
        programs.push(Program {
            id: format!("n:{}", stream.id),
            label: app_label(&stream.binary, &stream.application),
            role: stream.role,
            target,
            pactl_index: 0,
            node_id: stream.id,
        });
    }
}

fn node_hint(suffix: &str) -> String {
    format!("suffix:{suffix}")
}

fn playback(id: &str) -> Option<PlayPair> {
    topology::playback_pairs().into_iter().find(|pair| pair.id == id)
}

fn capture(id: &str) -> Option<CapDev> {
    topology::capture_devices().into_iter().find(|device| device.id == id)
}

fn json_list(runner: &dyn Runner, args: &[&str]) -> Result<Value, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let out = runner.run(&owned)?;
    if out.code != 0 {
        return Err("PipeWire control (pactl) is not available.".into());
    }
    serde_json::from_str(&out.stdout).map_err(|_| "PipeWire did not return a program list.".into())
}

fn index_names(list: &Value) -> Vec<(u64, String)> {
    list.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let index = item.get("index").and_then(|v| v.as_u64())?;
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    Some((index, name))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn stream_record(item: &Value, role: Role, names: &[(u64, String)]) -> Option<Program> {
    let props = item.get("properties")?;
    let binary = props.get("application.process.binary").and_then(|v| v.as_str()).unwrap_or("");
    let app_name = props.get("application.name").and_then(|v| v.as_str()).unwrap_or("");
    let node = props.get("node.name").and_then(|v| v.as_str()).unwrap_or("");
    let class = props.get("media.class").and_then(|v| v.as_str()).unwrap_or("");
    let expected = if role == Role::Capture { "Stream/Input/Audio" } else { "Stream/Output/Audio" };
    if !class.is_empty() && class != expected {
        return None;
    }
    if node.contains(".split") || node.starts_with("loopback-") {
        return None;
    }
    if skip_program(binary, app_name) {
        return None;
    }
    let index = item.get("index").and_then(|v| v.as_u64())?;
    let endpoint = match role {
        Role::Playback => item.get("sink").and_then(|v| v.as_u64()),
        Role::Capture => item.get("source").and_then(|v| v.as_u64()),
    };
    let endpoint_name = endpoint
        .and_then(|id| names.iter().find(|(have, _)| *have == id).map(|(_, name)| name.as_str()))
        .unwrap_or("");
    let prefix = if role == Role::Playback { "p" } else { "c" };
    Some(Program {
        id: format!("{prefix}:{index}"),
        label: app_label(binary, app_name),
        role,
        target: logical_target(endpoint_name),
        pactl_index: index,
        node_id: 0,
    })
}

fn logical_target(name: &str) -> String {
    if name == DESK_SINK || name == format!("{DESK_SINK}.monitor") {
        return "desk".into();
    }
    if let Some(rest) = name.strip_prefix("crimson_dummy_") {
        let slug = rest.strip_suffix(".monitor").unwrap_or(rest);
        if !slug.is_empty() {
            return format!("dummy:{slug}");
        }
    }
    if let Some(rest) = name.strip_prefix("crimson_") {
        let slug = rest.strip_suffix(".monitor").unwrap_or(rest);
        if !slug.is_empty() && slug != "desk" {
            return format!("hear:{slug}");
        }
    }
    for pair in topology::playback_pairs() {
        if name.contains("usb-MOTU_828_") && name.ends_with(pair.suffix) {
            return pair.id.into();
        }
    }
    for device in topology::capture_devices() {
        if name.contains("usb-MOTU_828_") && name.ends_with(device.suffix) {
            return device.id.into();
        }
    }
    String::new()
}

fn app_label(binary: &str, application: &str) -> String {
    let known = [
        ("librewolf", "LibreWolf"),
        ("firefox", "Firefox"),
        ("discord", "Discord"),
        ("obs", "OBS"),
        ("obs-studio", "OBS"),
        ("steam", "Steam"),
        ("steamwebhelper", "Steam"),
        ("chromium", "Chromium"),
        ("spotify", "Spotify"),
        ("vlc", "VLC"),
        ("mpv", "mpv"),
    ];
    let key = binary.to_ascii_lowercase();
    if let Some((_, label)) = known.iter().find(|(name, _)| *name == key) {
        return (*label).into();
    }
    let name = application.trim();
    let generic = ["webrtc voiceengine", "chromium input", "discord_hardware_mute", "audio stream", "playback", "alsa playback"];
    if !name.is_empty() && !generic.iter().any(|item| item.eq_ignore_ascii_case(name)) {
        return name.to_string();
    }
    if binary.is_empty() {
        return if name.is_empty() { "Program".into() } else { name.into() };
    }
    let mut chars = binary.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Program".into(),
    }
}

fn skip_program(binary: &str, application: &str) -> bool {
    let key = binary.to_ascii_lowercase();
    let name = application.to_ascii_lowercase();
    if key.is_empty() && name.is_empty() {
        return true;
    }
    const SKIP: &[&str] = &[
        "kwin_wayland", "wireplumber", "pipewire", "pipewire-pulse", "pipewire-media-session",
        "kded6", "libcanberra", "plasmashell", "polkit", "polkit-kde-authentication-agent-1",
        "pactl", "wpctl", "pw-cli", "pw-dump", "systemd", "cuemix5", "com.motu.cuemix5",
        "rustmix",
    ];
    if SKIP.contains(&key.as_str()) || key.contains("cuemix") || key.contains("rustmix") {
        return true;
    }
    if key.starts_with("pipewire") || key.starts_with("wireplumber") || key.starts_with("xdg-desktop-portal") || key.starts_with("kwin") {
        return true;
    }
    name == "wireplumber" || name == "pipewire" || name == "pactl"
}

pub fn playback_choices() -> Vec<(&'static str, &'static str)> {
    let mut choices: Vec<(&'static str, &'static str)> = topology::playback_pairs()
        .into_iter()
        .map(|pair| (pair.id, pair.label))
        .collect();
    choices.push(("desk", "To another program"));
    choices.push(("default", "Default output"));
    choices
}

pub fn target_label(role: Role, target: &str) -> String {
    if let Some(slug) = target.strip_prefix("dummy:") {
        return crate::dummy::label_from_slug(slug);
    }
    if let Some(slug) = target.strip_prefix("hear:") {
        return hear_name(slug);
    }
    if target.is_empty() {
        return match role {
            Role::Playback => "System output".into(),
            Role::Capture => "System input".into(),
        };
    }
    let choices = match role {
        Role::Playback => playback_choices(),
        Role::Capture => capture_choices(),
    };
    choices
        .into_iter()
        .find(|(id, _)| *id == target)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| target.to_string())
}

pub fn capture_choices() -> Vec<(&'static str, &'static str)> {
    let mut choices: Vec<(&'static str, &'static str)> = topology::capture_devices()
        .into_iter()
        .map(|device| (device.id, device.label))
        .collect();
    choices.push(("desk", "From another program"));
    choices.push(("default", "Default input"));
    choices
}

/// One desktop volume row. Percent may sit above 100 when PipeWire is already
/// past unity. A slider still writes 0–100, and only after the person lets go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    pub kind: LevelKind,
    pub index: u64,
    pub title: String,
    pub detail: String,
    pub percent: u32,
    pub muted: bool,
    pub default_node: bool,
}

impl Level {
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.token(), self.index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelKind {
    Sink,
    Source,
    Playback,
    Capture,
}

impl LevelKind {
    pub fn token(self) -> &'static str {
        match self {
            Self::Sink => "sink",
            Self::Source => "source",
            Self::Playback => "playback",
            Self::Capture => "capture",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Levels {
    pub outputs: Vec<Level>,
    pub playback: Vec<Level>,
    pub inputs: Vec<Level>,
    pub recording: Vec<Level>,
}

/// pactl arguments for a volume. Never a move, a module load, or a route.
pub fn volume_args(kind: LevelKind, index: u64, percent: u8) -> Vec<String> {
    let verb = match kind {
        LevelKind::Sink => "set-sink-volume",
        LevelKind::Source => "set-source-volume",
        LevelKind::Playback => "set-sink-input-volume",
        LevelKind::Capture => "set-source-output-volume",
    };
    vec![verb.into(), index.to_string(), format!("{percent}%")]
}

/// pactl arguments for mute. The flag is 1 or 0.
pub fn mute_args(kind: LevelKind, index: u64, muted: bool) -> Vec<String> {
    let verb = match kind {
        LevelKind::Sink => "set-sink-mute",
        LevelKind::Source => "set-source-mute",
        LevelKind::Playback => "set-sink-input-mute",
        LevelKind::Capture => "set-source-output-mute",
    };
    vec![verb.into(), index.to_string(), if muted { "1" } else { "0" }.into()]
}

/// Output devices, playback programs, input devices, and recording programs.
/// Monitor sources are left out. A missing default name does not fail the read.
pub fn read_levels(runner: &dyn Runner) -> Result<Levels, String> {
    let sinks = json_list(runner, &["-f", "json", "list", "sinks"])?;
    let sources = json_list(runner, &["-f", "json", "list", "sources"])?;
    let playback = json_list(runner, &["-f", "json", "list", "sink-inputs"])?;
    let recording = json_list(runner, &["-f", "json", "list", "source-outputs"])?;
    let default_sink = pactl_line(runner, &["get-default-sink"]);
    let default_source = pactl_line(runner, &["get-default-source"]);
    let sink_labels = device_labels(&sinks);
    let source_labels = device_labels(&sources);
    Ok(Levels {
        outputs: device_rows(&sinks, LevelKind::Sink, &default_sink, false),
        inputs: device_rows(&sources, LevelKind::Source, &default_source, true),
        playback: stream_rows(&playback, LevelKind::Playback, &sink_labels),
        recording: stream_rows(&recording, LevelKind::Capture, &source_labels),
    })
}

fn pactl_line(runner: &dyn Runner, args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    match runner.run(&owned) {
        Ok(out) if out.code == 0 => out.stdout.trim().to_string(),
        _ => String::new(),
    }
}

fn device_labels(list: &Value) -> Vec<(u64, String)> {
    list.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| Some((item_index(item)?, device_title(item))))
                .collect()
        })
        .unwrap_or_default()
}

fn device_rows(list: &Value, kind: LevelKind, default_name: &str, skip_monitors: bool) -> Vec<Level> {
    let mut rows = Vec::new();
    let Some(items) = list.as_array() else { return rows };
    for item in items {
        let name = item.get("name").and_then(|value| value.as_str()).unwrap_or("");
        if skip_monitors && name.ends_with(".monitor") {
            continue;
        }
        let Some(index) = item_index(item) else { continue };
        rows.push(Level {
            kind,
            index,
            title: device_title(item),
            detail: String::new(),
            percent: volume_percent(item.get("volume").unwrap_or(&Value::Null)),
            muted: is_muted(item),
            default_node: !default_name.is_empty() && name == default_name,
        });
    }
    sort_levels(&mut rows);
    rows
}

fn stream_rows(list: &Value, kind: LevelKind, labels: &[(u64, String)]) -> Vec<Level> {
    let mut rows = Vec::new();
    let Some(items) = list.as_array() else { return rows };
    let expected = if kind == LevelKind::Capture { "Stream/Input/Audio" } else { "Stream/Output/Audio" };
    for item in items {
        let props = item.get("properties").cloned().unwrap_or(Value::Null);
        let binary = props.get("application.process.binary").and_then(|v| v.as_str()).unwrap_or("");
        let app_name = props.get("application.name").and_then(|v| v.as_str()).unwrap_or("");
        let node = props.get("node.name").and_then(|v| v.as_str()).unwrap_or("");
        let class = props.get("media.class").and_then(|v| v.as_str()).unwrap_or("");
        if !class.is_empty() && class != expected {
            continue;
        }
        if node.contains(".split") || node.starts_with("loopback-") {
            continue;
        }
        if skip_program(binary, app_name) {
            continue;
        }
        let Some(index) = item_index(item) else { continue };
        let endpoint = match kind {
            LevelKind::Capture => item.get("source").and_then(|v| v.as_u64()),
            _ => item.get("sink").and_then(|v| v.as_u64()),
        };
        let detail = endpoint
            .and_then(|id| labels.iter().find(|(have, _)| *have == id).map(|(_, title)| title.clone()))
            .unwrap_or_default();
        rows.push(Level {
            kind,
            index,
            title: app_label(binary, app_name),
            detail,
            percent: volume_percent(item.get("volume").unwrap_or(&Value::Null)),
            muted: is_muted(item),
            default_node: false,
        });
    }
    sort_levels(&mut rows);
    rows
}

fn sort_levels(rows: &mut [Level]) {
    rows.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then(left.index.cmp(&right.index))
    });
}

fn device_title(item: &Value) -> String {
    if let Some(text) = item.get("description").and_then(|value| value.as_str()) {
        let text = text.trim();
        if !text.is_empty() {
            return text.to_string();
        }
    }
    if let Some(text) = item
        .get("properties")
        .and_then(|props| props.get("device.description"))
        .and_then(|value| value.as_str())
    {
        let text = text.trim();
        if !text.is_empty() {
            return text.to_string();
        }
    }
    item.get("name").and_then(|value| value.as_str()).filter(|text| !text.is_empty()).unwrap_or("Device").to_string()
}

fn volume_percent(volume: &Value) -> u32 {
    let Some(map) = volume.as_object() else { return 0 };
    let mut best = 0u32;
    for channel in map.values() {
        let Some(raw) = channel.get("value").and_then(|value| value.as_f64()) else { continue };
        if !raw.is_finite() || raw < 0.0 {
            continue;
        }
        let percent = (raw * 100.0 / 65536.0).round() as u32;
        best = best.max(percent);
    }
    best
}

fn is_muted(item: &Value) -> bool {
    item.get("mute").and_then(|value| value.as_bool()).unwrap_or(false)
}

fn item_index(item: &Value) -> Option<u64> {
    item.get("index").and_then(|value| value.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        sinks: String,
        inputs: String,
        sources: String,
        outputs: String,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl Runner for Fake {
        fn run(&self, args: &[String]) -> Result<CmdOut, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            let stdout = if args.ends_with(&["list".to_string(), "sinks".to_string()]) {
                self.sinks.clone()
            } else if args.ends_with(&["list".to_string(), "sources".to_string()]) {
                self.sources.clone()
            } else if args.ends_with(&["list".to_string(), "sink-inputs".to_string()]) {
                self.inputs.clone()
            } else if args.ends_with(&["list".to_string(), "source-outputs".to_string()]) {
                self.outputs.clone()
            } else {
                String::new()
            };
            Ok(CmdOut { code: 0, stdout, stderr: String::new() })
        }
    }

    fn sample() -> Fake {
        Fake {
            sinks: r#"[{"index":7,"name":"alsa_output.usb-MOTU_828_123-00.HiFi__Line1__sink"},{"index":8,"name":"alsa_output.usb-MOTU_828_123-00.HiFi__SPDIF1__sink"}]"#.into(),
            sources: r#"[{"index":9,"name":"alsa_input.usb-MOTU_828_123-00.HiFi__SPDIF3__source"}]"#.into(),
            inputs: r#"[{"index":4,"sink":7,"properties":{"application.name":"LibreWolf","application.process.binary":"librewolf","media.class":"Stream/Output/Audio","node.name":"LibreWolf"}},{"index":5,"sink":7,"properties":{"application.name":"WirePlumber","application.process.binary":"wireplumber","media.class":"Stream/Output/Audio","node.name":"WirePlumber"}}]"#.into(),
            outputs: r#"[{"index":6,"source":9,"properties":{"application.name":"Recorder","application.process.binary":"recorder","media.class":"Stream/Input/Audio","node.name":"Recorder"}}]"#.into(),
            calls: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn a_route_label_names_the_output_and_the_system_device() {
        assert_eq!(target_label(Role::Playback, ""), "System output");
        assert_eq!(target_label(Role::Playback, "Line1"), "Main");
        assert_eq!(target_label(Role::Capture, ""), "System input");
        assert_eq!(target_label(Role::Capture, "Mic1"), "Mic 1");
        assert_eq!(Role::Playback.token(), "playback");
        assert_eq!(Role::Capture.token(), "capture");
        assert_eq!(target_label(Role::Playback, "hear:discord"), "Discord");
        assert_eq!(sink_slug("Discord"), "discord");
        assert_eq!(sink_slug("OBS Studio"), "obs_studio");
    }

    #[test]
    fn replacing_a_live_output_asks_and_an_open_pair_does_not() {
        let mut patch = vec![127u8; 128];
        assert!(route_warning("LibreWolf", "Line1", "Line2", &patch).is_none());
        patch[topology::line_slot(2) as usize] = 78;
        patch[topology::line_slot(3) as usize] = 79;
        let asked = route_warning("LibreWolf", "Line1", "Line2", &patch).unwrap();
        assert!(asked.contains("Line out 3"), "{asked}");
        assert!(asked.contains("Analog 1 mix"), "{asked}");
        assert!(asked.contains("leave Main"), "{asked}");
        patch[topology::line_slot(2) as usize] = 2;
        patch[topology::line_slot(3) as usize] = 3;
        assert!(route_warning("LibreWolf", "Line2", "Line2", &patch).is_none());
        let spdif = route_warning("LibreWolf", "Line1", "SPDIF1", &vec![127u8; 128]).unwrap();
        assert!(spdif.contains("S/PDIF"), "{spdif}");
        let hear = route_warning("LibreWolf", "Line1", "hear:discord", &patch).unwrap();
        assert!(hear.contains("Discord"), "{hear}");
        let mut open = vec![127u8; 128];
        open[topology::line_slot(4) as usize] = 4;
        open[topology::line_slot(5) as usize] = 5;
        assert!(route_warning("LibreWolf", "Line1", "Line3", &open).is_none());
        let named = route_warning_named("LibreWolf", "Line1", "Line3", &open, "I HEAR THIS").unwrap();
        assert!(named.contains("I HEAR THIS"), "{named}");
        assert!(named.contains("Line out 5-6"), "{named}");
        let mut live = vec![127u8; 128];
        live[topology::line_slot(2) as usize] = 78;
        live[topology::line_slot(3) as usize] = 79;
        let both = route_warning_named("LibreWolf", "Line1", "Line2", &live, "Studio").unwrap();
        assert!(both.contains("Analog 1 mix"), "{both}");
        assert!(both.contains("Line out 4"), "{both}");
        assert!(both.contains("named Studio"), "{both}");
        assert_eq!(target_label(Role::Playback, "hear:obs_studio"), "OBS");
        assert_eq!(target_label(Role::Playback, "dummy:dummy"), "Dummy");
        assert_eq!(target_label(Role::Playback, "dummy:dummy_2"), "Dummy 2");
        assert_eq!(logical_target("crimson_dummy_dummy"), "dummy:dummy");
        assert_eq!(logical_target("crimson_dummy_dummy.monitor"), "dummy:dummy");
        assert_eq!(logical_target("crimson_discord"), "hear:discord");
        assert_eq!(logical_target("RustMix"), "desk");
    }

    #[test]
    fn a_native_stream_keeps_the_pulse_row_and_gains_its_node() {
        let dump = r#"[{"id":492,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"LibreWolf","application.name":"LibreWolf","application.process.binary":"librewolf","media.class":"Stream/Output/Audio","object.serial":4}}},{"id":92,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"alsa_output.usb-MOTU_828_123-00.HiFi__Line1__sink","media.class":"Audio/Sink","object.serial":7}}},{"id":1,"type":"PipeWire:Interface:Link","info":{"output-node-id":492,"input-node-id":92,"state":"active"}}]"#;
        let mut programs = refresh(&sample()).unwrap();
        merge_nodes(&mut programs, &serde_json::from_str::<Value>(dump).unwrap());
        let wolf = programs.iter().find(|program| program.label == "LibreWolf").unwrap();
        assert_eq!(wolf.node_id, 492);
        assert_eq!(wolf.pactl_index, 4);
        assert_eq!(programs.iter().filter(|program| program.label == "LibreWolf").count(), 1);
    }

    #[test]
    fn lists_programs_and_skips_the_session() {
        let programs = refresh(&sample()).unwrap();
        assert!(programs.iter().any(|program| program.label == "LibreWolf" && program.target == "Line1"));
        assert!(programs.iter().all(|program| program.label != "WirePlumber"));
        assert!(programs.iter().any(|program| program.id == "c:6" && program.target == "SPDIF3"));
    }

    #[test]
    fn desk_sink_name_is_ours_and_spdif_playback_is_the_usb_pair() {
        let programs = refresh(&sample()).unwrap();
        let wolf = programs.iter().find(|program| program.label == "LibreWolf").unwrap();
        let planned = plan_move(wolf, "SPDIF1").unwrap();
        assert_eq!(planned.patches, vec![(58, 14), (59, 15)]);
        assert!(!planned.args.iter().any(|arg| arg.contains("cuemix_pc")));
        let desk = plan_move(wolf, "desk").unwrap();
        assert!(desk.load_sink);
        assert!(desk.args.iter().any(|arg| arg == DESK_SINK));
        assert!(load_sink_args().iter().any(|arg| arg.contains(DESK_SINK)));
        assert!(load_sink_args().iter().all(|arg| !arg.contains("cuemix")));
        let capture = programs.iter().find(|program| program.role == Role::Capture).unwrap();
        let bad = plan_move(capture, "Line1").unwrap_err();
        assert!(bad.contains("records"));
    }

    struct LevelFake {
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl Runner for LevelFake {
        fn run(&self, args: &[String]) -> Result<CmdOut, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            let stdout = match args {
                [flag, format, list, what] if flag == "-f" && format == "json" && list == "list" && what == "sinks" => {
                    r#"[{"index":64,"name":"hdmi","description":"HDMI","mute":false,"volume":{"front-left":{"value":26214}}},{"index":97,"name":"motu","description":"Main","mute":false,"volume":{"front-left":{"value":65536},"front-right":{"value":65536}}},{"index":12,"name":"hot","description":"Hot","mute":true,"volume":{"front-left":{"value":98304}}}]"#
                }
                [flag, format, list, what] if flag == "-f" && format == "json" && list == "list" && what == "sources" => {
                    r#"[{"index":1,"name":"hdmi.monitor","description":"Monitor of HDMI","mute":false,"volume":{"front-left":{"value":65536}}},{"index":2,"name":"webcam","description":"Webcam","mute":false,"volume":{"front-left":{"value":32768}}}]"#
                }
                [flag, format, list, what] if flag == "-f" && format == "json" && list == "list" && what == "sink-inputs" => {
                    r#"[{"index":2371,"sink":97,"mute":false,"volume":{"front-left":{"value":65536}},"properties":{"application.name":"LibreWolf","application.process.binary":"librewolf","media.class":"Stream/Output/Audio","node.name":"LibreWolf"}},{"index":8,"sink":97,"mute":false,"volume":{"front-left":{"value":65536}},"properties":{"application.name":"WirePlumber","application.process.binary":"wireplumber","media.class":"Stream/Output/Audio","node.name":"WirePlumber"}}]"#
                }
                [flag, format, list, what] if flag == "-f" && format == "json" && list == "list" && what == "source-outputs" => {
                    r#"[{"index":6,"source":2,"mute":false,"volume":{"front-left":{"value":65536}},"properties":{"application.name":"Recorder","application.process.binary":"recorder","media.class":"Stream/Input/Audio","node.name":"Recorder"}}]"#
                }
                [what] if what == "get-default-sink" => "motu\n",
                [what] if what == "get-default-source" => "webcam\n",
                _ => "",
            };
            Ok(CmdOut { code: 0, stdout: stdout.into(), stderr: String::new() })
        }
    }

    #[test]
    fn desktop_levels_read_volume_and_do_not_route() {
        let fake = LevelFake { calls: Mutex::new(Vec::new()) };
        let levels = read_levels(&fake).unwrap();
        assert!(levels.outputs.iter().any(|node| node.title == "HDMI" && node.percent == 40 && !node.muted));
        assert!(levels.outputs.iter().any(|node| node.title == "Hot" && node.percent == 150 && node.muted));
        assert!(levels.outputs.iter().any(|node| node.default_node && node.title == "Main" && node.percent == 100));
        assert!(levels.inputs.iter().all(|node| !node.title.contains("Monitor")));
        assert!(levels.inputs.iter().any(|node| node.title == "Webcam" && node.percent == 50 && node.default_node));
        assert!(levels.playback.iter().any(|node| node.title == "LibreWolf" && node.detail == "Main" && node.percent == 100));
        assert!(levels.playback.iter().all(|node| node.title != "WirePlumber"));
        assert!(levels.recording.iter().any(|node| node.title == "Recorder" && node.detail == "Webcam"));
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        for bad in ["move-sink-input", "move-source-output", "load-module", "set-sink-volume", "set-source-volume", "set-sink-mute"] {
            assert!(!joined.contains(bad), "{joined}");
        }
    }

    #[test]
    fn volume_and_mute_commands_only_set_a_level() {
        let cases = [
            volume_args(LevelKind::Sink, 64, 40),
            volume_args(LevelKind::Source, 3, 0),
            volume_args(LevelKind::Playback, 2371, 100),
            volume_args(LevelKind::Capture, 9, 15),
            mute_args(LevelKind::Sink, 64, true),
            mute_args(LevelKind::Source, 3, false),
            mute_args(LevelKind::Playback, 2371, true),
            mute_args(LevelKind::Capture, 9, false),
        ];
        assert_eq!(cases[0], vec!["set-sink-volume".to_string(), "64".into(), "40%".into()]);
        assert_eq!(cases[4], vec!["set-sink-mute".to_string(), "64".into(), "1".into()]);
        assert_eq!(cases[5][2], "0");
        for args in &cases {
            let joined = args.join(" ");
            for bad in ["move-sink-input", "move-source-output", "load-module", "set-default", "suspend-sink"] {
                assert!(!joined.contains(bad), "{joined}");
            }
            assert!(args[0].starts_with("set-"), "{joined}");
            assert_eq!(args.len(), 3, "{joined}");
        }
    }
}

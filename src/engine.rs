//! The window and the socket share one planner. A live connection replaces the
//! desk with the unit's dump; demo edits are not written to the port.

use crate::desk::Desk;
use crate::dummy;
use crate::pipewire::{self, PactlRunner, Program, Runner};
use crate::plan::{self, loop_text, Cmd, Origin};
use crate::protocol::Write;
use crate::settings::{self, Settings};
use crate::topology::{self, NOTHING};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Debug)]
pub enum DeviceEvent {
    Synced { desk: Desk, readonly: Option<String> },
    Report { prop: u16, index: u16, raw: Vec<u8> },
    Failed(String),
    Lost(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Demo,
    Live,
    ReadOnly { reason: String },
    Offline { reason: String },
}

impl Link {
    pub fn name(&self) -> &'static str {
        match self {
            Link::Demo => "demo",
            Link::Live => "live",
            Link::ReadOnly { .. } => "readonly",
            Link::Offline { .. } => "offline",
        }
    }

    pub fn writable(&self) -> bool {
        matches!(self, Link::Demo | Link::Live)
    }

    fn sends(&self) -> bool {
        matches!(self, Link::Live)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LockChange {
    All(bool),
    Monitor(bool),
    Phones(u8, bool),
    Trim(u8, bool),
    Cell(u16, u16, bool),
    Bus(u16, bool),
}

pub struct Engine {
    pub desk: Desk,
    pub settings: Settings,
    pub link: Link,
    pub notice: String,
    pub loop_text: String,
    pub programs: Vec<Program>,
    pub connecting: bool,
    runner: Arc<dyn Runner>,
    events: Option<Receiver<DeviceEvent>>,
    frame_tx: Option<Sender<Vec<u8>>>,
    stop: Option<Arc<AtomicBool>>,
    restores: HashMap<String, Vec<(u16, u8)>>,
    /// The newest full meter picture. Older pictures are dropped, not queued.
    pending_meter: Option<(u16, Vec<u8>)>,
    /// Changes when the desk changes for a reason other than a meter picture.
    pub(crate) desk_gen: u64,
    /// Saved dummy cables are linked once, after the live patch arrives. Never rewritten.
    dummies_relinked: bool,
    pub(crate) pending_page: Option<(String, Option<u16>)>,
    pub(crate) shown_page: String,
    pub(crate) shown_bus: u16,
    /// What Control shows under the address. Not saved.
    pub mcp_listen: String,
}

pub struct Hub {
    pub engine: Mutex<Engine>,
    #[cfg(feature = "ui")]
    pub ctx: Mutex<Option<eframe::egui::Context>>,
    /// The window is on screen. Meter traffic does not wake a minimized one.
    show_live: AtomicBool,
}

impl Hub {
    pub fn new(engine: Engine) -> Arc<Self> {
        Arc::new(Self {
            engine: Mutex::new(engine),
            #[cfg(feature = "ui")]
            ctx: Mutex::new(None),
            show_live: AtomicBool::new(true),
        })
    }

    pub fn set_show_live(&self, show: bool) {
        self.show_live.store(show, Ordering::Relaxed);
    }

    pub fn show_live(&self) -> bool {
        self.show_live.load(Ordering::Relaxed)
    }

    /// Keep the latest full meter picture. A stalled window holds one, not every frame since it stalled.
    pub fn store_meter(&self, index: u16, raw: Vec<u8>) {
        self.lock().pending_meter = Some((index, raw));
    }

    pub fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn poke(&self) {
        #[cfg(feature = "ui")]
        {
            if let Some(ctx) = self.ctx.lock().unwrap_or_else(|err| err.into_inner()).as_ref() {
                ctx.request_repaint();
            }
        }
    }
}

impl Engine {
    pub fn new() -> Self {
        let (settings, problem) = Settings::load();
        Self::from_parts(settings, problem, Arc::new(PactlRunner))
    }

    pub fn from_parts(settings: Settings, problem: Option<String>, runner: Arc<dyn Runner>) -> Self {
        let desk = Desk::demo();
        let loop_text = loop_text(&desk);
        let mcp_listen = if settings.mcp_enabled { String::new() } else { "MCP is off.".into() };
        Self {
            desk,
            settings,
            link: Link::Demo,
            notice: problem.unwrap_or_else(|| "Demo desk. Nothing is sent to the 828.".into()),
            loop_text,
            programs: Vec::new(),
            connecting: false,
            runner,
            events: None,
            frame_tx: None,
            stop: None,
            restores: HashMap::new(),
            pending_meter: None,
            desk_gen: 0,
            dummies_relinked: false,
            pending_page: None,
            shown_page: "home".into(),
            shown_bus: 0,
            mcp_listen,
        }
    }

    pub fn attach_connect(&mut self, events: Receiver<DeviceEvent>, frames: Sender<Vec<u8>>, stop: Arc<AtomicBool>) {
        self.events = Some(events);
        self.frame_tx = Some(frames);
        self.stop = Some(stop);
        self.connecting = true;
    }

    pub fn drain(&mut self) {
        let pending = if let Some(events) = self.events.as_ref() {
            let mut pending = Vec::new();
            while let Ok(event) = events.try_recv() {
                pending.push(event);
            }
            pending
        } else {
            Vec::new()
        };
        for event in pending {
            self.on_event(event);
        }
        self.take_meter();
    }

    fn take_meter(&mut self) {
        let Some((index, raw)) = self.pending_meter.take() else { return };
        if !matches!(self.link, Link::Demo) {
            self.desk.apply_report(crate::protocol::PROP_METERS, index, &raw);
        }
    }

    fn bump_desk(&mut self) {
        self.desk_gen = self.desk_gen.wrapping_add(1);
    }

    pub fn on_event(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::Synced { desk, readonly } => {
                self.desk = desk;
                self.bump_desk();
                self.connecting = false;
                self.loop_text = loop_text(&self.desk);
                if let Some(reason) = readonly {
                    self.link = Link::ReadOnly { reason: reason.clone() };
                    self.notice = reason;
                } else {
                    self.link = Link::Live;
                    self.notice = "Connected to the 828.".into();
                    self.relink_dummies();
                }
            }
            DeviceEvent::Report { prop, index, raw } => {
                if !matches!(self.link, Link::Demo) {
                    if crate::gear::full_meter(index, &raw) && prop == crate::protocol::PROP_METERS {
                        self.pending_meter = Some((index, raw));
                    } else {
                        self.desk.apply_report(prop, index, &raw);
                        if prop != crate::protocol::PROP_METERS {
                            self.loop_text = loop_text(&self.desk);
                            self.bump_desk();
                        }
                    }
                }
            }
            DeviceEvent::Failed(message) => {
                self.connecting = false;
                self.frame_tx = None;
                self.stop = None;
                self.notice = message;
            }
            DeviceEvent::Lost(message) => {
                self.connecting = false;
                self.frame_tx = None;
                self.stop = None;
                if !matches!(self.link, Link::Demo) {
                    self.link = Link::Offline { reason: message.clone() };
                }
                self.notice = message;
            }
        }
    }

    pub fn call(&mut self, origin: Origin, cmds: &[Cmd]) -> Result<(), String> {
        self.drain();
        if self.connecting {
            return Err(format!("{} is still opening the 828.", crate::APP_NAME));
        }
        let writes = plan::plan(&self.desk, &self.settings.locks, origin, self.link.writable(), cmds)?;
        self.apply_writes(&writes)
    }

    pub fn release(&mut self) {
        if let Some(stop) = &self.stop {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.frame_tx = None;
        self.stop = None;
        self.connecting = false;
        if !matches!(self.link, Link::Demo) {
            self.link = Link::Offline { reason: "The 828 control port is released.".into() };
            self.notice = "The 828 control port is released.".into();
        }
    }

    pub fn use_demo(&mut self) {
        self.release();
        self.pending_meter = None;
        self.desk = Desk::demo();
        self.bump_desk();
        self.link = Link::Demo;
        self.loop_text = loop_text(&self.desk);
        self.notice = "Demo desk. Nothing is sent to the 828.".into();
    }

    pub fn refresh_programs(&mut self) -> Result<(), String> {
        self.reload_programs()?;
        self.restore_locked_programs();
        Ok(())
    }

    /// Reload the list without putting locks back. A move calls this so a
    /// restore cannot call the move again.
    fn reload_programs(&mut self) -> Result<(), String> {
        self.programs = pipewire::refresh(self.runner.as_ref())?;
        Ok(())
    }

    pub fn route_program(&mut self, id: &str, target: &str) -> Result<(), String> {
        if let Some(slug) = target.strip_prefix("hear:") {
            return self.route_toward_program(id, slug);
        }
        if let Some(slug) = target.strip_prefix("dummy:") {
            return self.route_into_dummy(id, slug);
        }
        self.drain();
        self.ensure_program(id)?;
        let program = self.programs.iter().find(|item| item.id == id).cloned().ok_or("That program is not in the list.")?;
        if self.program_is_locked(&program) {
            return Err("That program route is locked.".into());
        }
        if self.route_is_already_applied(&program, target) {
            return Ok(());
        }
        if let Err(err) = self.move_program(&program, target) {
            if !err.contains("not playing anymore") {
                return Err(err);
            }
            self.reload_programs()?;
            let same: Vec<_> = self
                .programs
                .iter()
                .filter(|item| item.label == program.label && item.role == program.role)
                .cloned()
                .collect();
            if same.len() != 1 {
                return Err(err);
            }
            self.move_program(&same[0], target)?;
        }
        let _ = self.reload_programs();
        Ok(())
    }

    /// Playback goes to a private sink, and the recording program listens to that sink.
    fn route_toward_program(&mut self, id: &str, slug: &str) -> Result<(), String> {
        self.drain();
        self.ensure_program(id)?;
        let program = self.programs.iter().find(|item| item.id == id).cloned().ok_or("That program is not in the list.")?;
        if program.role != pipewire::Role::Playback {
            return Err("That program plays audio, so it needs a playback target.".into());
        }
        if self.program_is_locked(&program) {
            return Err("That program route is locked.".into());
        }
        let capture = self
            .programs
            .iter()
            .find(|item| item.role == pipewire::Role::Capture && pipewire::sink_slug(&item.label) == slug)
            .cloned()
            .ok_or("That program is not recording.")?;
        if self.program_is_locked(&capture) {
            return Err("That program route is locked.".into());
        }
        let sink = format!("crimson_{slug}");
        self.ensure_named_sink(&sink)?;
        self.move_program(&program, &format!("sink:{sink}"))?;
        let capture = self
            .programs
            .iter()
            .find(|item| item.role == pipewire::Role::Capture && pipewire::sink_slug(&item.label) == slug)
            .cloned()
            .unwrap_or(capture);
        self.move_program(&capture, &format!("monitor:{sink}"))?;
        let _ = self.reload_programs();
        Ok(())
    }

    /// Playback mixes into the dummy. The 828 patch is left alone.
    fn route_into_dummy(&mut self, id: &str, slug: &str) -> Result<(), String> {
        self.drain();
        let host = self.dummy_host(slug)?.clone();
        self.ensure_program(id)?;
        let program = self.programs.iter().find(|item| item.id == id).cloned().ok_or("That program is not in the list.")?;
        if program.role != pipewire::Role::Playback {
            return Err("That program plays audio, so it needs a playback target.".into());
        }
        if self.program_is_locked(&program) {
            return Err("That program route is locked.".into());
        }
        let sink = dummy::sink_name(slug);
        self.ensure_described(&sink, &dummy::escaped_description(&host.name))?;
        self.move_program(&program, &format!("sink:{sink}"))?;
        let _ = self.reload_programs();
        Ok(())
    }

    pub fn create_dummy(&mut self) -> Result<String, String> {
        let host = dummy::next_host(&self.settings.dummies)?;
        let sink = dummy::sink_name(&host.slug);
        self.ensure_described(&sink, &dummy::escaped_description(&host.name))?;
        let name = host.name.clone();
        self.settings.dummies.push(host);
        Ok(name)
    }

    pub fn remove_dummy(&mut self, slug: &str) -> Result<(), String> {
        let host = self.dummy_host(slug)?.clone();
        let target = format!("dummy:{slug}");
        if self.programs.iter().any(|program| program.target == target && self.program_is_locked(program)) {
            return Err("A locked program is using that dummy.".into());
        }
        let mut cmds = Vec::new();
        for out in &host.outs {
            if self.others_hold(slug, out.slot) {
                continue;
            }
            let current = self.desk.patch.get(out.slot as usize).copied().unwrap_or(NOTHING);
            let usb = dummy::legs(out.slot, false).ok().and_then(|items| items.into_iter().next()).map(|leg| leg.usb);
            if usb == Some(current) && current != out.previous {
                cmds.push(Cmd::Patch { slot: out.slot, source: out.previous });
            }
        }
        if !cmds.is_empty() {
            self.call(Origin::Person, &cmds)?;
        }
        self.unlink_host(&host);
        let leaving: Vec<String> = self.programs.iter().filter(|program| program.target == target).map(|program| program.id.clone()).collect();
        for id in leaving {
            let _ = self.unroute_program(&id);
        }
        let _ = self.unload_sink(&dummy::sink_name(slug));
        self.settings.dummies.retain(|item| item.slug != slug);
        for item in &mut self.settings.dummies {
            item.from_dummies.retain(|from| from != slug);
        }
        Ok(())
    }

    /// Link the dummy's monitor into the chosen jacks, then patch only those slots.
    /// A jack this dummy already feeds stays. Other jacks are not cleared.
    pub fn connect_dummy_out(&mut self, slug: &str, slot: u16, stereo: bool) -> Result<(), String> {
        if !self.link.writable() {
            return Err(format!("{} is read-only on this unit.", crate::APP_NAME));
        }
        let host_name = self.dummy_host(slug)?.name.clone();
        let legs = dummy::legs(slot, stereo)?;
        let sink = dummy::sink_name(slug);
        self.ensure_described(&sink, &dummy::escaped_description(&host_name))?;
        let already: Vec<u16> = self.dummy_host(slug)?.outs.iter().map(|out| out.slot).collect();
        let mut fresh: Vec<(String, String)> = Vec::new();
        for leg in &legs {
            let node = self.resolve_hifi(leg.suffix, "sinks")?;
            let from = format!("{sink}:monitor_{}", leg.channel);
            let to = format!("{node}:playback_{}", leg.channel);
            if let Err(err) = self.link_ports(&from, &to) {
                for (old_from, old_to) in &fresh {
                    self.unlink_ports(old_from, old_to);
                }
                return Err(err);
            }
            if !already.contains(&leg.slot) {
                fresh.push((from, to));
            }
        }
        let mut held = Vec::new();
        let mut cmds = Vec::new();
        for leg in &legs {
            let current = self.desk.patch.get(leg.slot as usize).copied().unwrap_or(NOTHING);
            let previous = self
                .settings
                .dummies
                .iter()
                .find_map(|host| host.outs.iter().find(|out| out.slot == leg.slot).map(|out| out.previous))
                .unwrap_or(current);
            if !already.contains(&leg.slot) {
                held.push(settings::DummyOut { slot: leg.slot, previous });
            }
            if current != leg.usb {
                cmds.push(Cmd::Patch { slot: leg.slot, source: leg.usb });
            }
        }
        if !cmds.is_empty() {
            if let Err(err) = self.call(Origin::Person, &cmds) {
                for (from, to) in &fresh {
                    self.unlink_ports(from, to);
                }
                return Err(err);
            }
        }
        if let Some(host) = self.settings.dummies.iter_mut().find(|host| host.slug == slug) {
            for out in held {
                if !host.outs.iter().any(|have| have.slot == out.slot) {
                    host.outs.push(out);
                }
            }
        }
        Ok(())
    }

    pub fn uncable_dummy_out(&mut self, slug: &str, slot: u16, stereo: bool) -> Result<(), String> {
        let legs = dummy::legs(slot, stereo)?;
        let sink = dummy::sink_name(slug);
        let mut cmds = Vec::new();
        for leg in &legs {
            if let Ok(node) = self.resolve_hifi(leg.suffix, "sinks") {
                self.unlink_ports(&format!("{sink}:monitor_{}", leg.channel), &format!("{node}:playback_{}", leg.channel));
            }
            let Some(previous) = self.dummy_host(slug)?.outs.iter().find(|out| out.slot == leg.slot).map(|out| out.previous) else {
                continue;
            };
            let current = self.desk.patch.get(leg.slot as usize).copied().unwrap_or(NOTHING);
            if !self.others_hold(slug, leg.slot) && current == leg.usb && current != previous {
                cmds.push(Cmd::Patch { slot: leg.slot, source: previous });
            }
        }
        if !cmds.is_empty() {
            self.call(Origin::Person, &cmds)?;
        }
        if let Some(host) = self.settings.dummies.iter_mut().find(|host| host.slug == slug) {
            let slots: Vec<u16> = legs.iter().map(|leg| leg.slot).collect();
            host.outs.retain(|out| !slots.contains(&out.slot));
        }
        Ok(())
    }

    pub fn feed_dummy_from_source(&mut self, slug: &str, source: u16, stereo: bool) -> Result<(), String> {
        let plan = dummy::capture_plan(source, stereo)?;
        let host_name = self.dummy_host(slug)?.name.clone();
        let sink = dummy::sink_name(slug);
        self.ensure_described(&sink, &dummy::escaped_description(&host_name))?;
        let node = self.resolve_hifi(plan.suffix, "sources")?;
        for link in &plan.links {
            self.link_ports(&format!("{node}:{}", link.source_port), &format!("{sink}:{}", link.sink_port))?;
        }
        if let Some(host) = self.settings.dummies.iter_mut().find(|host| host.slug == slug) {
            if !host.captures.iter().any(|id| id == plan.device_id) {
                host.captures.push(plan.device_id.to_string());
            }
        }
        Ok(())
    }

    pub fn feed_dummy_from_dummy(&mut self, into_slug: &str, from_slug: &str, stereo: bool) -> Result<(), String> {
        if into_slug == from_slug {
            let name = self.dummy_host(into_slug).map(|host| host.name.clone()).unwrap_or_else(|_| "Dummy".into());
            return Err(format!("{name} would loop into itself."));
        }
        let into_name = self.dummy_host(into_slug)?.name.clone();
        let _ = self.dummy_host(from_slug)?;
        let into_sink = dummy::sink_name(into_slug);
        let from_sink = dummy::sink_name(from_slug);
        self.ensure_described(&into_sink, &dummy::escaped_description(&into_name))?;
        self.link_ports(&format!("{from_sink}:monitor_FL"), &format!("{into_sink}:playback_FL"))?;
        if stereo {
            self.link_ports(&format!("{from_sink}:monitor_FR"), &format!("{into_sink}:playback_FR"))?;
        }
        if let Some(host) = self.settings.dummies.iter_mut().find(|host| host.slug == into_slug) {
            if !host.from_dummies.iter().any(|slug| slug == from_slug) {
                host.from_dummies.push(from_slug.to_string());
            }
        }
        Ok(())
    }

    pub fn drop_dummy_capture(&mut self, slug: &str, device_id: &str) -> Result<(), String> {
        let sink = dummy::sink_name(slug);
        let _ = self.dummy_host(slug)?;
        if let Some(device) = topology::capture_devices().into_iter().find(|device| device.id == device_id) {
            if let Ok(node) = self.resolve_hifi(device.suffix, "sources") {
                if device.slot_len == 1 {
                    self.unlink_ports(&format!("{node}:capture_MONO"), &format!("{sink}:playback_FL"));
                    self.unlink_ports(&format!("{node}:capture_MONO"), &format!("{sink}:playback_FR"));
                } else {
                    self.unlink_ports(&format!("{node}:capture_FL"), &format!("{sink}:playback_FL"));
                    self.unlink_ports(&format!("{node}:capture_FR"), &format!("{sink}:playback_FR"));
                }
            }
        }
        if let Some(host) = self.settings.dummies.iter_mut().find(|host| host.slug == slug) {
            host.captures.retain(|id| id != device_id);
        }
        Ok(())
    }

    pub fn drop_dummy_loop(&mut self, into_slug: &str, from_slug: &str) -> Result<(), String> {
        let into_sink = dummy::sink_name(into_slug);
        let from_sink = dummy::sink_name(from_slug);
        self.unlink_ports(&format!("{from_sink}:monitor_FL"), &format!("{into_sink}:playback_FL"));
        self.unlink_ports(&format!("{from_sink}:monitor_FR"), &format!("{into_sink}:playback_FR"));
        if let Some(host) = self.settings.dummies.iter_mut().find(|host| host.slug == into_slug) {
            host.from_dummies.retain(|slug| slug != from_slug);
        }
        Ok(())
    }

    pub fn listen_to_dummy(&mut self, id: &str, slug: &str) -> Result<(), String> {
        let host = self.dummy_host(slug)?.clone();
        let sink = dummy::sink_name(slug);
        self.ensure_described(&sink, &dummy::escaped_description(&host.name))?;
        self.route_program(id, &format!("monitor:{sink}"))
    }

    fn dummy_host(&self, slug: &str) -> Result<&settings::DummyHost, String> {
        self.settings.dummies.iter().find(|host| host.slug == slug).ok_or_else(|| "That dummy is not on the desk.".to_string())
    }

    fn others_hold(&self, slug: &str, slot: u16) -> bool {
        self.settings.dummies.iter().any(|host| host.slug != slug && host.outs.iter().any(|out| out.slot == slot))
    }

    /// Put the saved links back only where the live patch is already that USB source.
    fn relink_dummies(&mut self) {
        if self.dummies_relinked {
            return;
        }
        self.dummies_relinked = true;
        if self.settings.dummies.is_empty() {
            return;
        }
        let hosts = self.settings.dummies.clone();
        for host in hosts {
            let sink = dummy::sink_name(&host.slug);
            let _ = self.ensure_described(&sink, &dummy::escaped_description(&host.name));
            for out in &host.outs {
                let Ok(legs) = dummy::legs(out.slot, false) else { continue };
                let Some(leg) = legs.into_iter().next() else { continue };
                let current = self.desk.patch.get(leg.slot as usize).copied().unwrap_or(NOTHING);
                if current != leg.usb {
                    continue;
                }
                if let Ok(node) = self.resolve_hifi(leg.suffix, "sinks") {
                    let _ = self.link_ports(&format!("{sink}:monitor_{}", leg.channel), &format!("{node}:playback_{}", leg.channel));
                }
            }
        }
    }

    fn unlink_host(&self, host: &settings::DummyHost) {
        let sink = dummy::sink_name(&host.slug);
        for out in &host.outs {
            if let Ok(legs) = dummy::legs(out.slot, false) {
                for leg in legs {
                    if let Ok(node) = self.resolve_hifi(leg.suffix, "sinks") {
                        self.unlink_ports(&format!("{sink}:monitor_{}", leg.channel), &format!("{node}:playback_{}", leg.channel));
                    }
                }
            }
        }
        for device_id in &host.captures {
            let Some(device) = topology::capture_devices().into_iter().find(|device| device.id == device_id) else { continue };
            let Ok(node) = self.resolve_hifi(device.suffix, "sources") else { continue };
            if device.slot_len == 1 {
                self.unlink_ports(&format!("{node}:capture_MONO"), &format!("{sink}:playback_FL"));
                self.unlink_ports(&format!("{node}:capture_MONO"), &format!("{sink}:playback_FR"));
            } else {
                self.unlink_ports(&format!("{node}:capture_FL"), &format!("{sink}:playback_FL"));
                self.unlink_ports(&format!("{node}:capture_FR"), &format!("{sink}:playback_FR"));
            }
        }
        for from in &host.from_dummies {
            let from_sink = dummy::sink_name(from);
            self.unlink_ports(&format!("{from_sink}:monitor_FL"), &format!("{sink}:playback_FL"));
            self.unlink_ports(&format!("{from_sink}:monitor_FR"), &format!("{sink}:playback_FR"));
        }
    }

    fn link_ports(&self, from: &str, to: &str) -> Result<(), String> {
        let from_node = from.split(':').next().unwrap_or("");
        let to_node = to.split(':').next().unwrap_or("");
        if !from_node.is_empty() && from_node == to_node {
            return Err("That cable would loop back into the same dummy.".into());
        }
        let args = vec!["pw-link".into(), from.to_string(), to.to_string()];
        match self.run_pactl(&args) {
            Ok(()) => Ok(()),
            Err(err) if err.to_ascii_lowercase().contains("exists") => Ok(()),
            Err(err) => Err(if err.trim().is_empty() { "PipeWire did not link that cable.".into() } else { err }),
        }
    }

    fn unlink_ports(&self, from: &str, to: &str) {
        let args = vec!["pw-link".into(), "-d".into(), from.to_string(), to.to_string()];
        let _ = self.run_pactl(&args);
    }

    fn resolve_hifi(&self, suffix: &str, kind: &str) -> Result<String, String> {
        let nodes = self.pactl_json(&["-f", "json", "list", kind])?;
        pipewire::resolve_node(&nodes, suffix).ok_or_else(|| "The 828 device for that route is not available.".to_string())
    }

    fn unload_sink(&self, sink: &str) -> Result<(), String> {
        let out = self.runner.run(&["list".into(), "short".into(), "modules".into()])?;
        if out.code != 0 {
            return Ok(());
        }
        let needle = format!("sink_name={sink}");
        for line in out.stdout.lines() {
            let mut parts = line.split('\t');
            let id = parts.next().unwrap_or("");
            let name = parts.next().unwrap_or("");
            let argument = parts.next().unwrap_or("");
            if name == "module-null-sink" && argument.split_whitespace().any(|token| token == needle) && !id.is_empty() {
                self.run_pactl(&["unload-module".into(), id.to_string()])?;
            }
        }
        Ok(())
    }

    pub fn unroute_program(&mut self, id: &str) -> Result<(), String> {
        self.drain();
        self.ensure_program(id)?;
        let program = self.programs.iter().find(|item| item.id == id).cloned().ok_or("That program is not in the list.")?;
        if self.program_is_locked(&program) {
            return Err("That program route is locked.".into());
        }
        if program.target.is_empty() {
            return Ok(());
        }
        let planned = pipewire::plan_move(&program, "default")?;
        let args = self.resolve_args(&planned.args)?;
        if program.node_id != 0 {
            let clear = vec![
                "pw-metadata".into(),
                "-n".into(),
                "default".into(),
                program.node_id.to_string(),
                "-d".into(),
                "target.object".into(),
            ];
            let _ = self.run_pactl(&clear);
        }
        if program.pactl_index != 0 {
            if let Err(err) = self.run_pactl(&args) {
                if program.node_id == 0 {
                    return Err(friendly_move(err));
                }
            }
        } else if program.node_id == 0 {
            return Err("That program is not playing anymore.".into());
        }
        if let Some(previous) = self.restores.remove(id) {
            let cmds: Vec<Cmd> = previous.into_iter().map(|(slot, source)| Cmd::Patch { slot, source }).collect();
            if let Ok(writes) = plan::plan(&self.desk, &self.settings.locks, Origin::Person, self.link.writable(), &cmds) {
                self.apply_writes(&writes)?;
            }
        }
        let _ = self.reload_programs();
        Ok(())
    }

    pub fn set_program_lock(&mut self, label: &str, role: &str, target: &str, locked: bool) -> Result<(), String> {
        let previous = self.settings.program_locks.clone();
        self.settings.program_locks.retain(|item| item.label != label || item.role != role);
        if locked {
            self.settings.program_locks.push(settings::ProgramLock {
                label: label.to_string(),
                role: role.to_string(),
                target: target.to_string(),
            });
        }
        if let Err(err) = self.settings.save() {
            self.settings.program_locks = previous;
            return Err(err);
        }
        Ok(())
    }

    fn program_is_locked(&self, program: &Program) -> bool {
        let role = program.role.token();
        self.settings.program_locks.iter().any(|item| item.label == program.label && item.role == role)
    }

    fn move_program(&mut self, program: &Program, target: &str) -> Result<(), String> {
        let planned = pipewire::plan_move(program, target)?;
        if planned.load_sink {
            self.ensure_sink()?;
        }
        let args = self.resolve_args(&planned.args)?;
        let cmds: Vec<Cmd> = planned.patches.iter().copied().map(|(slot, source)| Cmd::Patch { slot, source }).collect();
        let writes = if cmds.is_empty() {
            Vec::new()
        } else {
            plan::plan(&self.desk, &self.settings.locks, Origin::Person, self.link.writable(), &cmds)?
        };
        let previous: Vec<(u16, u8)> = planned
            .patches
            .iter()
            .map(|(slot, _)| (*slot, self.desk.patch.get(*slot as usize).copied().unwrap_or(NOTHING)))
            .collect();
        self.steer(program, &args)?;
        if !previous.is_empty() {
            self.restores.insert(program.id.clone(), previous);
        }
        self.apply_writes(&writes)
    }

    /// The stream is already on that device and the patch already matches, so
    /// a repeat does not call pactl or write the 828 again.
    fn route_is_already_applied(&self, program: &Program, target: &str) -> bool {
        if !Self::program_target_matches(program, target) {
            return false;
        }
        let Ok(planned) = pipewire::plan_move(program, target) else {
            return false;
        };
        planned.patches.iter().all(|(slot, source)| {
            self.desk.patch.get(*slot as usize).copied().unwrap_or(NOTHING) == *source
        })
    }

    fn program_target_matches(program: &Program, target: &str) -> bool {
        if target == "default" {
            program.target.is_empty()
        } else {
            program.target == target
        }
    }

    /// One pass. An empty locked target is the system device, which has no
    /// name to put back, so only a named target is sent again.
    fn restore_locked_programs(&mut self) {
        let due: Vec<(String, String)> = self
            .programs
            .iter()
            .filter_map(|program| {
                let role = program.role.token();
                let lock = self.settings.program_locks.iter().find(|item| item.label == program.label && item.role == role)?;
                if lock.target.is_empty() || lock.target == program.target {
                    None
                } else {
                    Some((program.id.clone(), lock.target.clone()))
                }
            })
            .collect();
        if due.is_empty() {
            return;
        }
        let mut failed: Option<String> = None;
        for (id, target) in due {
            let Some(program) = self.programs.iter().find(|item| item.id == id).cloned() else {
                continue;
            };
            if let Err(err) = self.move_program(&program, &target) {
                failed = Some(err);
            }
        }
        if let Err(err) = self.reload_programs() {
            failed = Some(err);
        }
        if let Some(err) = failed {
            self.notice = err;
        }
    }

    pub fn set_lock(&mut self, change: LockChange) -> Result<(), String> {
        match change {
            LockChange::All(locked) => self.settings.locks.all = locked,
            LockChange::Monitor(locked) => self.settings.locks.monitor = locked,
            LockChange::Phones(pair, locked) => {
                if let Some(slot) = self.settings.locks.phones.get_mut(pair as usize) {
                    *slot = locked;
                }
            }
            LockChange::Trim(which, locked) => {
                if let Some(slot) = self.settings.locks.trims.get_mut(which as usize) {
                    *slot = locked;
                }
            }
            LockChange::Cell(bus, input, locked) => self.settings.locks.set_cell(bus, input, locked),
            LockChange::Bus(bus, locked) => self.settings.locks.set_bus(bus, locked),
        }
        self.settings.save()
    }

    pub fn set_launch(&mut self, enabled: bool) -> Result<(), String> {
        let exec = std::env::current_exe().map_err(|err| err.to_string())?;
        let text = settings::autostart_text(&exec.display().to_string(), &icon_path());
        settings::set_autostart(&settings::autostart_dir(), enabled, &text)?;
        self.settings.launch_at_login = enabled;
        self.settings.save()
    }

    pub fn set_minimized(&mut self, enabled: bool) -> Result<(), String> {
        self.settings.start_minimized = enabled;
        self.settings.save()
    }

    pub fn set_close_quits(&mut self, quits: bool) -> Result<(), String> {
        self.settings.close_quits = quits;
        self.settings.save()
    }

    pub fn set_mcp_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if self.settings.mcp_enabled == enabled {
            return Ok(());
        }
        self.settings.mcp_enabled = enabled;
        self.settings.save()
    }

    pub fn set_mcp_host(&mut self, host: &str) -> Result<(), String> {
        let host = settings::normalize_mcp_host(host)?;
        if self.settings.mcp_host == host {
            return Ok(());
        }
        self.settings.mcp_host = host;
        self.settings.save()
    }

    pub fn set_mcp_port(&mut self, port: u16) -> Result<(), String> {
        if port == 0 {
            return Err("The port is a number from 1 to 65535.".into());
        }
        if self.settings.mcp_port == port {
            return Ok(());
        }
        self.settings.mcp_port = port;
        self.settings.save()
    }

    pub fn lock_screen(&mut self, key: String, index: Option<i32>, place: [f32; 4]) -> Result<(), String> {
        self.settings.screen_key = Some(key);
        self.settings.screen = index;
        self.settings.place = Some(place);
        self.settings.save()?;
        if let Some(index) = index
            && settings::write_screen_rule(Some(index))?
        {
            settings::reconfigure_kwin();
        }
        Ok(())
    }

    pub fn remember_place(&mut self, place: [f32; 4]) -> Result<(), String> {
        if let Some(saved) = self.settings.place
            && saved.iter().zip(place).all(|(left, right)| (left - right).abs() <= 1.0)
        {
            return Ok(());
        }
        self.settings.place = Some(place);
        self.settings.save()
    }

    pub fn request(&mut self, text: &str) -> String {
        self.drain();
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(_) => return reply(None, false, Value::String("The request was not valid.".into())),
        };
        let id = value.get("id").cloned();
        if denied_keys(&value) || denied_method(value.get("method").and_then(|item| item.as_str()).unwrap_or("")) {
            return reply(id.as_ref(), false, Value::String("That control is not available here.".into()));
        }
        let method = value.get("method").and_then(|item| item.as_str()).unwrap_or("");
        // Ping stays up when MCP is off so a second window still sees this one.
        // Every other method is the assistant, and it waits until Control turns MCP on.
        if method != "ping" && !self.settings.mcp_enabled {
            return reply(id.as_ref(), false, Value::String("MCP is off.".into()));
        }
        let result = match method {
            "status" => Ok(self.status_value()),
            "list_routes" => Ok(self.routes_value()),
            "list_programs" => self.refresh_programs().map(|_| self.programs_value()),
            "connect_route" => self.socket_patch(&value, false),
            "disconnect_route" => self.socket_patch(&value, true),
            "route_program" => self.socket_program(&value, false),
            "unroute_program" => self.socket_program(&value, true),
            "set_mix" => self.socket_mix(&value),
            "set_bus" => self.socket_bus(&value),
            "list_pages" => Ok(pages_value()),
            "show_page" => self.show_page(&value),
            "ping" => Ok(Value::String("ok".into())),
            "look" => self.look(&value),
            "set_phase" => self.socket_phase(&value),
            "set_midi" => self.socket_midi(&value),
            "set_band" => self.socket_band(&value),
            "set_comp" => self.socket_comp(&value),
            "set_foot_key" => self.socket_foot_key(&value),
            "set_name" => self.socket_name(&value),
            _ => Err("That request is not available.".into()),
        };
        match result {
            Ok(body) => reply(id.as_ref(), true, body),
            Err(message) => reply(id.as_ref(), false, Value::String(message)),
        }
    }

    fn status_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("app".into(), Value::String(crate::APP_NAME.into()));
        map.insert("link".into(), Value::String(self.link.name().into()));
        map.insert("model".into(), Value::String(self.desk.model.clone()));
        map.insert("api".into(), Value::from(self.desk.api));
        map.insert("mix_loaded".into(), Value::from(self.desk.mix_loaded));
        map.insert("loop".into(), Value::String(self.loop_text.clone()));
        map.insert("notice".into(), Value::String(self.notice.clone()));
        map.insert("connecting".into(), Value::from(self.connecting));
        map.insert("page".into(), Value::String(self.shown_page.clone()));
        if self.shown_page == "mixer" {
            map.insert("bus".into(), Value::from(self.shown_bus));
        }
        Value::Object(map)
    }

    fn show_page(&mut self, value: &Value) -> Result<Value, String> {
        let page = value.get("page").and_then(|item| item.as_str()).unwrap_or("");
        let name = match page {
            "home" | "device" | "control" | "input" | "output" | "routing" | "discovery" | "pc" => page,
            "mixer" | "mix" => "mixer",
            _ => return Err("That page is not on this desk.".into()),
        };
        let bus = if name == "mixer" {
            let bus = match value.get("bus") {
                None | Some(Value::Null) => self.shown_bus,
                Some(_) => number(value, "bus")?,
            };
            if bus > 15 {
                return Err("That page is not on this desk.".into());
            }
            Some(bus)
        } else if value.get("bus").is_some_and(|item| !item.is_null()) {
            return Err("That page is not on this desk.".into());
        } else {
            None
        };
        self.pending_page = Some((name.to_string(), bus));
        self.shown_page = name.to_string();
        if let Some(bus) = bus {
            self.shown_bus = bus;
        }
        let mut map = Map::new();
        map.insert("page".into(), Value::String(name.into()));
        if let Some(bus) = bus {
            map.insert("bus".into(), Value::from(bus));
        }
        Ok(Value::Object(map))
    }

    fn routes_value(&self) -> Value {
        let routes: Vec<Value> = topology::destinations()
            .into_iter()
            .map(|jack| {
                let source = self.desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING);
                let mut row = Map::new();
                row.insert("slot".into(), Value::from(jack.id));
                row.insert("destination".into(), Value::String(jack.name.into()));
                row.insert("group".into(), Value::String(jack.group.into()));
                if source >= NOTHING {
                    row.insert("source".into(), Value::Null);
                    row.insert("source_name".into(), Value::String("Nothing".into()));
                } else {
                    row.insert("source".into(), Value::from(source));
                    row.insert("source_name".into(), Value::String(topology::source_label(source)));
                }
                Value::Object(row)
            })
            .collect();
        let mut map = Map::new();
        map.insert("routes".into(), Value::Array(routes));
        Value::Object(map)
    }

    fn programs_value(&self) -> Value {
        let items = self.programs.iter().map(|program| {
            let mut row = Map::new();
            row.insert("id".into(), Value::String(program.id.clone()));
            row.insert("label".into(), Value::String(program.label.clone()));
            row.insert("role".into(), Value::String(program.role.token().into()));
            row.insert("target".into(), Value::String(program.target.clone()));
            Value::Object(row)
        }).collect();
        let mut map = Map::new();
        map.insert("programs".into(), Value::Array(items));
        Value::Object(map)
    }

    fn socket_patch(&mut self, value: &Value, clear: bool) -> Result<Value, String> {
        let slot = number(value, "slot")?;
        let source = if clear { u16::from(NOTHING) } else { number(value, "source")? };
        if source > u16::from(NOTHING) {
            return Err("That source is not on this unit.".into());
        }
        self.call(Origin::Mcp, &[Cmd::Patch { slot, source: source as u8 }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_program(&mut self, value: &Value, clear: bool) -> Result<Value, String> {
        let id = value.get("id").and_then(|item| item.as_str()).unwrap_or("");
        if id.is_empty() {
            return Err("That program is not in the list.".into());
        }
        if clear {
            self.unroute_program(id)?;
        } else {
            let target = value.get("target").and_then(|item| item.as_str()).unwrap_or("");
            self.route_program(id, target)?;
        }
        Ok(Value::String("ok".into()))
    }

    fn socket_mix(&mut self, value: &Value) -> Result<Value, String> {
        let bus = number(value, "bus")?;
        let input = number(value, "input")?;
        let db = optional_f32(value, "db");
        let pan = optional_f32(value, "pan");
        let mute = value.get("mute").and_then(|item| item.as_bool());
        let solo = value.get("solo").and_then(|item| item.as_bool());
        if db.is_none() && pan.is_none() && mute.is_none() && solo.is_none() {
            return Err("Nothing to change.".into());
        }
        self.call(Origin::Mcp, &[Cmd::Mix { bus, input, gain: None, db, pan, mute, solo }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_bus(&mut self, value: &Value) -> Result<Value, String> {
        let bus = number(value, "bus")?;
        let db = optional_f32(value, "db");
        let mute = value.get("mute").and_then(|item| item.as_bool());
        if db.is_none() && mute.is_none() {
            return Err("Nothing to change.".into());
        }
        self.call(Origin::Mcp, &[Cmd::Bus { bus, gain: None, db, mute }])?;
        Ok(Value::String("ok".into()))
    }

    fn apply_writes(&mut self, writes: &[Write]) -> Result<(), String> {
        if writes.is_empty() {
            self.loop_text = loop_text(&self.desk);
            return Ok(());
        }
        for write in writes {
            self.desk.apply_write(write.prop, write.index, &write.raw);
        }
        self.bump_desk();
        if self.link.sends() {
            if let Some(tx) = &self.frame_tx {
                for write in writes {
                    tx.send(write.frame()).map_err(|_| "The 828 control port closed.".to_string())?;
                }
            }
        }
        self.loop_text = loop_text(&self.desk);
        self.notice.clear();
        Ok(())
    }

    fn ensure_program(&mut self, id: &str) -> Result<(), String> {
        if self.programs.iter().any(|program| program.id == id) {
            return Ok(());
        }
        self.refresh_programs()?;
        Ok(())
    }

    fn ensure_sink(&self) -> Result<(), String> {
        self.ensure_named_sink(pipewire::DESK_SINK)
    }

    fn ensure_named_sink(&self, name: &str) -> Result<(), String> {
        let description = if name == pipewire::DESK_SINK { "RustMix" } else { "Crimson" };
        self.ensure_described(name, description)
    }

    fn ensure_described(&self, name: &str, description: &str) -> Result<(), String> {
        let sinks = self.pactl_json(&["-f", "json", "list", "sinks"])?;
        if pipewire::sink_loaded_name(&sinks, name) {
            return Ok(());
        }
        self.run_pactl(&pipewire::load_sink_args_named(name, description))
    }

    /// A native client is steered by the session metadata. Pulse clients still use pactl.
    fn steer(&self, program: &pipewire::Program, args: &[String]) -> Result<(), String> {
        let endpoint = args.last().map(String::as_str).unwrap_or("");
        let kind = if args.first().map(String::as_str) == Some("move-source-output") { "sources" } else { "sinks" };
        if program.node_id != 0 && !endpoint.is_empty() && !endpoint.starts_with('@') {
            if let Ok(serial) = self.node_serial(endpoint, kind) {
                let meta = vec![
                    "pw-metadata".into(),
                    "-n".into(),
                    "default".into(),
                    program.node_id.to_string(),
                    "target.object".into(),
                    serial.to_string(),
                    "Spa:Id".into(),
                ];
                return self.run_pactl(&meta).map_err(friendly_move);
            }
        }
        if program.pactl_index == 0 {
            return Err("That program is not playing anymore.".into());
        }
        self.run_pactl(args).map_err(friendly_move)
    }

    fn node_serial(&self, name: &str, kind: &str) -> Result<u64, String> {
        let nodes = self.pactl_json(&["-f", "json", "list", kind])?;
        nodes
            .as_array()
            .and_then(|items| {
                items.iter().find_map(|item| {
                    let have = item.get("name").and_then(|value| value.as_str())?;
                    if have == name { item.get("index").and_then(|value| value.as_u64()) } else { None }
                })
            })
            .filter(|serial| *serial != 0)
            .ok_or_else(|| "The 828 device for that route is not available.".to_string())
    }

    fn resolve_args(&self, args: &[String]) -> Result<Vec<String>, String> {
        let mut resolved = Vec::with_capacity(args.len());
        for arg in args {
            if let Some(suffix) = arg.strip_prefix("suffix:") {
                let kind = if args.first().map(String::as_str) == Some("move-source-output") { "sources" } else { "sinks" };
                let nodes = self.pactl_json(&["-f", "json", "list", kind])?;
                let name = pipewire::resolve_node(&nodes, suffix).ok_or_else(|| "The 828 device for that route is not available.".to_string())?;
                resolved.push(name);
            } else {
                resolved.push(arg.clone());
            }
        }
        Ok(resolved)
    }

    fn pactl_json(&self, args: &[&str]) -> Result<Value, String> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let out = self.runner.run(&owned)?;
        if out.code != 0 {
            return Err("PipeWire control (pactl) is not available.".into());
        }
        serde_json::from_str(&out.stdout).map_err(|_| "PipeWire did not return a program list.".into())
    }

    fn run_pactl(&self, args: &[String]) -> Result<(), String> {
        let out = self.runner.run(args)?;
        if out.code != 0 {
            Err(if out.stderr.is_empty() { "PipeWire did not move that program.".into() } else { out.stderr })
        } else {
            Ok(())
        }
    }
}

impl Engine {
    fn look(&self, value: &Value) -> Result<Value, String> {
        let what = value.get("what").and_then(|item| item.as_str()).unwrap_or("");
        let mut map = Map::new();
        match what {
            "cell" => {
                let bus = number(value, "bus")?;
                let input = number(value, "input")?;
                let index = topology::cell_index(bus, input).ok_or("That channel is outside the mixer.")?;
                let db = self.desk.gain_at(bus, input).map(topology::db_of);
                map.insert("db".into(), json_opt_f32(db));
                map.insert("pan".into(), Value::from(self.desk.pans.get(index).copied().unwrap_or(0.5)));
                map.insert("mute".into(), Value::from(self.desk.mutes.get(index).copied().unwrap_or(0) != 0));
                map.insert("solo".into(), Value::from(self.desk.solos.get(index).copied().unwrap_or(0) != 0));
            }
            "band" => {
                let bus = value.get("bus").and_then(|item| item.as_bool()).unwrap_or(false);
                let channel = number(value, "channel")?;
                let band = number(value, "band")? as usize;
                let bands = if bus { 3 } else { 4 };
                let index = channel as usize * bands + band;
                let (mode, bypass, hz, gain, q) = if bus {
                    let g = &self.desk.gear;
                    (g.bus_mode.get(index).copied(), g.bus_bypass.get(index).copied(), g.bus_hz.get(index).copied(), g.bus_gain.get(index).copied(), g.bus_q.get(index).copied())
                } else {
                    let g = &self.desk.gear;
                    (g.in_mode.get(index).copied(), g.in_bypass.get(index).copied(), g.in_hz.get(index).copied(), g.in_gain.get(index).copied(), g.in_q.get(index).copied())
                };
                let (Some(mode), Some(bypass), Some(hz), Some(gain), Some(q)) = (mode, bypass, hz, gain, q) else {
                    return Err("That EQ band is not on this unit.".into());
                };
                map.insert("on".into(), Value::from(bypass == 0));
                map.insert("hz".into(), Value::from(hz));
                map.insert("mode".into(), Value::from(mode));
                map.insert("q".into(), Value::from(q));
                map.insert("eq_db".into(), Value::from(gain));
            }
            "phase" => {
                let index = number(value, "index")? as usize;
                let byte = self.desk.gear.phase.get(index).copied().flatten().ok_or("That switch has not arrived yet.")?;
                map.insert("invert".into(), Value::from(byte != 0));
            }
            "midi" => {
                let byte = self.desk.gear.midi.ok_or("That switch has not arrived yet.")?;
                map.insert("on".into(), Value::from(byte != 0));
            }
            "name" => {
                let output = value.get("output").and_then(|item| item.as_bool()).unwrap_or(false);
                let index = number(value, "index")? as usize;
                let text = if output {
                    self.desk.gear.out_names.get(index)
                } else {
                    self.desk.gear.in_names.get(index)
                }.ok_or("That name is not on this unit.")?;
                map.insert("text".into(), Value::String(text.clone()));
            }
            "device" => {
                let gear = &self.desk.gear;
                map.insert("firmware".into(), Value::String(gear.firmware.clone()));
                map.insert("rate".into(), Value::from(gear.sample_rate));
                map.insert("clock".into(), json_opt_u8(gear.clock));
                map.insert("midi".into(), json_opt_u8(gear.midi));
                map.insert("word".into(), json_opt_u8(gear.word));
                map.insert("optical_in".into(), json_opt_u8(gear.optical[0]));
                map.insert("optical_out".into(), json_opt_u8(gear.optical[1]));
                map.insert("expander".into(), json_opt_u8(gear.expander));
                map.insert("host".into(), json_opt_u8(gear.host_mix));
                map.insert("loop_first".into(), json_opt_u8(gear.loop_first));
                map.insert("uid".into(), Value::String(self.desk.uid.clone()));
                map.insert("name".into(), Value::String(self.desk.name.clone()));
            }
            "silent" => {
                let input = number(value, "input")?;
                map.insert("silent".into(), Value::from(input_is_quiet(&self.desk, input)));
            }
            "comp" => {
                let index = number(value, "index")? as usize;
                if index > 9 {
                    return Err("That compressor is not on this unit.".into());
                }
                let gear = &self.desk.gear;
                map.insert("on".into(), Value::from(gear.comp_byp[index] == 0));
                map.insert("db".into(), Value::from(gear.comp_thr[index]));
            }
            "loop" => {
                let gear = &self.desk.gear;
                map.insert("left".into(), json_opt_u8(gear.loop_l));
                map.insert("right".into(), json_opt_u8(gear.loop_r));
            }
            "foot" => {
                let gear = &self.desk.gear;
                map.insert("enable".into(), json_opt_u8(gear.foot_en));
                map.insert("down".into(), Value::String(gear.foot_down.clone()));
                map.insert("up".into(), Value::String(gear.foot_up.clone()));
            }
            _ => return Err("That request is not available.".into()),
        }
        Ok(Value::Object(map))
    }

    fn socket_phase(&mut self, value: &Value) -> Result<Value, String> {
        let index = number(value, "index")?;
        let invert = value.get("invert").and_then(|item| item.as_bool()).ok_or("That request is not available.")?;
        self.call(Origin::Mcp, &[Cmd::Phase { index, invert }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_midi(&mut self, value: &Value) -> Result<Value, String> {
        let on = value.get("on").and_then(|item| item.as_bool()).ok_or("That request is not available.")?;
        self.call(Origin::Mcp, &[Cmd::MidiThru { on }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_band(&mut self, value: &Value) -> Result<Value, String> {
        if value.get("gain").is_some() || value.get("eq_db").is_some() || value.get("makeup").is_some() {
            return Err("That control is not available here.".into());
        }
        let bus = value.get("bus").and_then(|item| item.as_bool()).unwrap_or(false);
        let channel = number(value, "channel")?;
        let band = number(value, "band")? as u8;
        let on = value.get("on").and_then(|item| item.as_bool());
        let mode = value.get("mode").and_then(|item| item.as_u64()).map(|item| item as u8);
        let hz = value.get("hz").and_then(|item| item.as_i64()).map(|item| item as i32);
        let q = optional_f32(value, "q");
        if on.is_none() && mode.is_none() && hz.is_none() && q.is_none() {
            return Err("Nothing to change.".into());
        }
        self.call(Origin::Mcp, &[Cmd::Eq { bus, channel, band, on, mode, hz, gain: None, q }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_comp(&mut self, value: &Value) -> Result<Value, String> {
        let index = number(value, "index")?;
        let db = optional_f32(value, "db").ok_or("Nothing to change.")?;
        self.call(Origin::Mcp, &[Cmd::Comp {
            index, on: None, db: Some(db), ratio: None, attack: None, release: None, makeup: None,
        }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_foot_key(&mut self, value: &Value) -> Result<Value, String> {
        let up = value.get("up").and_then(|item| item.as_bool()).unwrap_or(false);
        let text = value.get("text").and_then(|item| item.as_str()).unwrap_or("");
        self.call(Origin::Mcp, &[Cmd::FootKey { up, bytes: crate::protocol::text_bytes(text) }])?;
        Ok(Value::String("ok".into()))
    }

    fn socket_name(&mut self, value: &Value) -> Result<Value, String> {
        let output = value.get("output").and_then(|item| item.as_bool()).unwrap_or(false);
        let index = number(value, "index")?;
        let text = value.get("text").and_then(|item| item.as_str()).unwrap_or("");
        self.call(Origin::Mcp, &[Cmd::ChannelName { output, index, bytes: crate::protocol::text_bytes(text) }])?;
        Ok(Value::String("ok".into()))
    }
}

fn input_is_quiet(desk: &Desk, input: u16) -> bool {
    for bus in 0..16u16 {
        let Some(gain) = desk.gain_at(bus, input) else { continue };
        let Some(index) = topology::cell_index(bus, input) else { continue };
        let muted = desk.mutes.get(index).copied().unwrap_or(0) != 0;
        if !muted && gain > 0.0 && topology::db_of(gain) > -60.0 {
            return false;
        }
    }
    true
}

fn json_opt_u8(value: Option<u8>) -> Value {
    value.map(|item| Value::from(item)).unwrap_or(Value::Null)
}

fn json_opt_f32(value: Option<f32>) -> Value {
    value.map(|item| Value::from(item)).unwrap_or(Value::Null)
}

fn number(value: &Value, key: &str) -> Result<u16, String> {
    value
        .get(key)
        .and_then(|item| item.as_u64().or_else(|| item.as_i64().filter(|n| *n >= 0).map(|n| n as u64)))
        .and_then(|item| u16::try_from(item).ok())
        .ok_or_else(|| "That request is not available.".into())
}

fn optional_f32(value: &Value, key: &str) -> Option<f32> {
    value.get(key).and_then(|item| item.as_f64()).map(|item| item as f32)
}

fn reply(id: Option<&Value>, ok: bool, body: Value) -> String {
    let mut map = Map::new();
    if let Some(id) = id {
        map.insert("id".into(), id.clone());
    }
    map.insert("ok".into(), Value::from(ok));
    map.insert(if ok { "result" } else { "error" }.into(), body);
    serde_json::to_string(&Value::Object(map)).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"The reply could not be written.\"}".into())
}

fn pages_value() -> Value {
    let pages = ["home", "device", "control", "input", "output", "routing", "discovery", "mixer", "pc"]
        .into_iter()
        .map(|page| Value::String(page.into()))
        .collect();
    let mut map = Map::new();
    map.insert("pages".into(), Value::Array(pages));
    Value::Object(map)
}

fn denied_method(method: &str) -> bool {
    const DENIED: &[&str] = &[
        "set_monitor", "set_phones", "set_headphones", "set_trim", "set_output_trim",
        "set_phantom", "set_preamp", "set_pad", "set_talkback", "set_eq", "set_makeup",
    ];
    DENIED.contains(&method)
}

fn denied_keys(value: &Value) -> bool {
    const DENIED: &[&str] = &[
        "kmaintrim", "kotrim", "kpreamp", "kpad", "kphantom", "phantom", "preamp", "pad",
        "talkback", "makeup", "output_trim", "phones", "headphones", "headphone", "monitor",
        "5011", "5000", "5001", "5003", "5004",
    ];
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            DENIED.contains(&key.to_ascii_lowercase().as_str()) || denied_keys(child)
        }),
        Value::Array(items) => items.iter().any(denied_keys),
        _ => false,
    }
}

pub fn friendly_move(err: String) -> String {
    if err.to_ascii_lowercase().contains("no such entity") {
        "That program is not playing anymore.".into()
    } else {
        err.trim().to_string()
    }
}

fn icon_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/.local/share/icons/hicolor/256x256/apps/{}.png", crate::PROCESS_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipewire::CmdOut;
    use std::sync::Mutex as StdMutex;

    struct Fake {
        calls: StdMutex<Vec<Vec<String>>>,
        fail_pw_link: bool,
    }

    impl Runner for Fake {
        fn run(&self, args: &[String]) -> Result<CmdOut, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            if self.fail_pw_link && args.first().map(String::as_str) == Some("pw-link") && !args.iter().any(|arg| arg == "-d") {
                return Ok(CmdOut { code: 1, stdout: String::new(), stderr: "link failed".into() });
            }
            let stdout = if args.ends_with(&["list".into(), "sinks".into()]) {
                r#"[{"index":8,"name":"alsa_output.usb-MOTU_828_123-00.HiFi__SPDIF1__sink"},{"index":9,"name":"alsa_output.usb-MOTU_828_123-00.HiFi__Line2__sink"},{"index":10,"name":"alsa_output.usb-MOTU_828_123-00.HiFi__Line1__sink"},{"index":11,"name":"alsa_output.usb-MOTU_828_123-00.HiFi__Line4__sink"}]"#.into()
            } else if args.ends_with(&["list".into(), "sink-inputs".into()]) {
                r#"[{"index":4,"sink":8,"properties":{"application.name":"Player","application.process.binary":"player","media.class":"Stream/Output/Audio","node.name":"Player"}}]"#.into()
            } else if args.ends_with(&["list".into(), "sources".into()]) {
                r#"[{"index":12,"name":"alsa_input.usb-MOTU_828_123-00.HiFi__Mic1__source"}]"#.into()
            } else if args.ends_with(&["list".into(), "source-outputs".into()]) {
                "[]".into()
            } else {
                String::new()
            };
            Ok(CmdOut { code: 0, stdout, stderr: String::new() })
        }
    }

    fn engine() -> Engine {
        let mut engine = rig().0;
        engine.settings.mcp_enabled = true;
        engine
    }

    #[test]
    fn a_newer_meter_replaces_the_one_still_waiting() {
        let mut live = engine();
        live.link = Link::Live;
        let generation = live.desk_gen;
        live.pending_meter = Some((0, vec![1, 2, 3]));
        live.pending_meter = Some((0, vec![9, 8]));
        live.drain();
        assert_eq!(live.desk.gear.meters, vec![9, 8]);
        assert_eq!(live.desk_gen, generation);
        assert!(live.pending_meter.is_none());
        live.drain();
        assert_eq!(live.desk.gear.meters, vec![9, 8]);

        live.on_event(DeviceEvent::Report {
            prop: crate::protocol::PROP_METERS,
            index: 0,
            raw: vec![7, 7, 7],
        });
        assert_eq!(live.desk.gear.meters, vec![9, 8]);
        assert_eq!(live.desk_gen, generation);
        live.drain();
        assert_eq!(live.desk.gear.meters, vec![7, 7, 7]);
        assert_eq!(live.desk_gen, generation);

        live.on_event(DeviceEvent::Report {
            prop: crate::protocol::PROP_METERS,
            index: 0,
            raw: vec![1],
        });
        assert_eq!(live.desk.gear.meters, vec![1, 7, 7]);
        assert_eq!(live.desk_gen, generation);

        live.on_event(DeviceEvent::Report {
            prop: crate::protocol::PROP_MIDI,
            index: 0,
            raw: vec![1],
        });
        assert_eq!(live.desk.gear.midi, Some(1));
        assert_eq!(live.desk_gen, generation.wrapping_add(1));

        let mut demo = engine();
        demo.pending_meter = Some((0, vec![1, 2]));
        demo.drain();
        assert!(demo.desk.gear.meters.is_empty());
        assert!(demo.pending_meter.is_none());
    }

    fn rig() -> (Engine, Arc<Fake>) {
        let fake = Arc::new(Fake { calls: StdMutex::new(Vec::new()), fail_pw_link: false });
        let engine = Engine::from_parts(Settings::default(), None, fake.clone());
        (engine, fake)
    }

    #[test]
    fn socket_refuses_private_levels_and_a_direct_loop_changes_nothing() {
        let mut engine = engine();
        let before = engine.desk.patch.clone();
        let denied = engine.request(r#"{"id":1,"method":"set_monitor","kMainTrim":0}"#);
        assert!(denied.contains("\"ok\":false"), "{denied}");
        assert!(!denied.contains("kMainTrim"), "{denied}");
        assert_eq!(engine.desk.patch, before);
        let looped = engine.request(r#"{"id":2,"method":"connect_route","slot":58,"source":58}"#);
        assert!(looped.contains("stays off"), "{looped}");
        assert_eq!(engine.desk.patch, before);
        let status = engine.request(r#"{"id":3,"method":"status"}"#);
        assert!(status.contains("\"link\":\"demo\""));
        assert!(!status.contains("monitor"));
        assert!(!status.contains("phones"));
    }

    #[test]
    fn sync_discards_the_demo_and_a_program_move_follows_the_guard() {
        let mut engine = engine();
        engine.desk.monitor = Some(10);
        engine.desk.patch[37] = 76;
        let mut incoming = Desk::blank();
        incoming.model = "828E".into();
        incoming.api = 9;
        incoming.name = "828".into();
        incoming.uid = "UNIT".into();
        incoming.saw_model = true;
        incoming.saw_api = true;
        incoming.saw_name = true;
        incoming.saw_uid = true;
        incoming.patch[37] = 127;
        incoming.monitor = Some(80);
        engine.on_event(DeviceEvent::Synced { desk: incoming, readonly: None });
        assert_eq!(engine.desk.monitor, Some(80));
        assert_eq!(engine.desk.patch[37], 127);
        assert!(engine.frame_tx.is_none());
        assert!(matches!(engine.link, Link::Live));

        engine.link = Link::Demo;
        engine.desk = Desk::demo();
        let moved = engine.request(r#"{"id":4,"method":"route_program","id":"p:4","target":"SPDIF1"}"#);
        assert!(moved.contains("\"ok\":true"), "{moved}");
        assert_eq!(engine.desk.patch[58], 14);
        assert_eq!(engine.desk.patch[59], 15);
        let calls = engine.runner.as_ref();
        let _ = calls;
    }

    #[test]
    fn desk_sink_is_not_the_old_name() {
        let (mut engine, fake) = rig();
        engine.settings.mcp_enabled = true;
        let moved = engine.request(r#"{"id":5,"method":"route_program","id":"p:4","target":"desk"}"#);
        assert!(moved.contains("\"ok\":true"), "{moved}");
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().flatten().cloned().collect::<Vec<_>>().join(" ");
        assert!(joined.contains("RustMix"), "{joined}");
        assert!(!joined.contains("crimson_desk"), "{joined}");
        assert!(joined.contains("load-module"), "{joined}");
        assert!(!joined.contains("cuemix"), "{joined}");
        let load_at = calls.iter().position(|args| args.iter().any(|arg| arg == "load-module")).unwrap();
        let move_at = calls.iter().position(|args| args.first().map(String::as_str) == Some("move-sink-input")).unwrap();
        assert!(load_at < move_at, "the sink is created before the program moves");
    }

    #[test]
    fn a_refused_plan_does_not_move_the_program() {
        let (mut engine, fake) = rig();
        engine.settings.mcp_enabled = true;
        engine.link = Link::ReadOnly { reason: "read-only".into() };
        let before = engine.desk.patch.clone();
        let moved = engine.request(r#"{"id":9,"method":"route_program","id":"p:4","target":"SPDIF1"}"#);
        assert!(moved.contains("\"ok\":false"), "{moved}");
        assert!(moved.contains("read-only"), "{moved}");
        let calls = fake.calls.lock().unwrap();
        assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("move-sink-input")));
        assert_eq!(engine.desk.patch, before);
    }

    #[test]
    fn mcp_is_off_until_control_turns_it_on() {
        let mut engine = rig().0;
        assert!(!engine.settings.mcp_enabled);
        let status = engine.request(r#"{"method":"status"}"#);
        assert!(status.contains("MCP is off."), "{status}");
        let ping = engine.request(r#"{"method":"ping"}"#);
        assert!(ping.contains("\"ok\":true"), "{ping}");
        let denied = engine.request(r#"{"method":"set_monitor"}"#);
        assert!(denied.contains("not available"), "{denied}");
        assert!(!denied.contains("MCP is off."), "{denied}");
        engine.settings.mcp_enabled = true;
        let status = engine.request(r#"{"method":"status"}"#);
        assert!(status.contains("\"ok\":true"), "{status}");
    }

    #[test]
    fn show_page_sets_the_page_and_leaves_the_patch_alone() {
        let mut engine = engine();
        let before = engine.desk.patch.clone();
        let monitor = engine.desk.monitor;
        let phones = engine.desk.phones.clone();
        let shown = engine.request(r#"{"id":11,"method":"show_page","page":"output"}"#);
        assert!(shown.contains("\"ok\":true"), "{shown}");
        assert!(shown.contains("\"page\":\"output\""), "{shown}");
        assert_eq!(engine.pending_page.as_ref().map(|(page, _)| page.as_str()), Some("output"));
        assert_eq!(engine.desk.patch, before);
        assert_eq!(engine.desk.monitor, monitor);
        assert_eq!(engine.desk.phones, phones);
        let mix = engine.request(r#"{"id":12,"method":"show_page","page":"mixer","bus":10}"#);
        assert!(mix.contains("\"page\":\"mixer\""), "{mix}");
        assert!(mix.contains("\"bus\":10"), "{mix}");
        assert_eq!(engine.desk.patch, before);
        let missing = engine.request(r#"{"id":13,"method":"show_page","page":"logout"}"#);
        assert!(missing.contains("\"ok\":false"), "{missing}");
        let denied = engine.request(r#"{"id":14,"method":"set_monitor"}"#);
        assert!(denied.contains("\"ok\":false"), "{denied}");
        assert!(!denied.contains("5011"), "{denied}");
        let status = engine.request(r#"{"id":15,"method":"status"}"#);
        assert!(status.contains("\"page\":\"mixer\""), "{status}");
        assert!(!status.contains("monitor"), "{status}");
        assert!(!status.contains("phones"), "{status}");
        let pc = engine.request(r#"{"id":16,"method":"show_page","page":"pc"}"#);
        assert!(pc.contains("\"ok\":true"), "{pc}");
        assert!(pc.contains("\"page\":\"pc\""), "{pc}");
        assert!(!pc.contains("\"bus\""), "{pc}");
        let pc_bus = engine.request(r#"{"id":17,"method":"show_page","page":"pc","bus":0}"#);
        assert!(pc_bus.contains("\"ok\":false"), "{pc_bus}");
        let listed = engine.request(r#"{"id":18,"method":"list_pages"}"#);
        assert!(listed.contains("\"pc\""), "{listed}");
        let status_pc = engine.request(r#"{"id":19,"method":"status"}"#);
        assert!(status_pc.contains("\"page\":\"pc\""), "{status_pc}");
        assert!(!status_pc.contains("\"bus\""), "{status_pc}");
        assert!(!status_pc.contains("monitor"), "{status_pc}");
        assert_eq!(engine.desk.patch, before);
        assert_eq!(engine.desk.monitor, monitor);
    }

    #[test]
    fn a_locked_program_route_is_refused_and_only_a_named_target_is_put_back() {
        let (mut engine, fake) = rig();
        engine.settings.mcp_enabled = true;
        engine.settings.program_locks.push(settings::ProgramLock {
            label: "Player".into(),
            role: "playback".into(),
            target: "SPDIF1".into(),
        });
        let before = engine.desk.patch.clone();
        let refused = engine.request(r#"{"id":20,"method":"route_program","id":"p:4","target":"desk"}"#);
        assert!(refused.contains("locked"), "{refused}");
        assert!(refused.contains("\"ok\":false"), "{refused}");
        let cleared = engine.request(r#"{"id":21,"method":"unroute_program","id":"p:4"}"#);
        assert!(cleared.contains("locked"), "{cleared}");
        assert_eq!(engine.desk.patch, before);
        {
            let calls = fake.calls.lock().unwrap();
            assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("move-sink-input")));
        }

        engine.settings.program_locks[0].target.clear();
        engine.refresh_programs().unwrap();
        {
            let calls = fake.calls.lock().unwrap();
            assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("move-sink-input")));
            assert!(calls.iter().all(|args| !args.iter().any(|arg| arg == "load-module")));
        }

        engine.settings.program_locks[0].target = "desk".into();
        engine.refresh_programs().unwrap();
        let calls = fake.calls.lock().unwrap();
        assert!(calls.iter().any(|args| args.iter().any(|arg| arg == "RustMix")));
        assert!(calls.iter().any(|args| args.first().map(String::as_str) == Some("move-sink-input")));
    }

    #[test]
    fn routing_a_program_where_it_already_plays_does_not_move_it() {
        let (mut engine, fake) = rig();
        engine.route_program("p:4", "SPDIF1").unwrap();
        engine.desk.patch[58] = 14;
        engine.desk.patch[59] = 15;
        fake.calls.lock().unwrap().clear();
        engine.route_program("p:4", "SPDIF1").unwrap();
        let calls = fake.calls.lock().unwrap();
        assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("move-sink-input")));
        assert_eq!(engine.desk.patch[58], 14);
        assert_eq!(engine.desk.patch[59], 15);
    }

    #[test]
    fn a_native_stream_is_steered_by_metadata_and_still_patches() {
        let (mut engine, fake) = rig();
        engine.refresh_programs().unwrap();
        let player = engine.programs.iter_mut().find(|program| program.id == "p:4").unwrap();
        player.node_id = 492;
        fake.calls.lock().unwrap().clear();
        engine.route_program("p:4", "SPDIF1").unwrap();
        assert_eq!(engine.desk.patch[58], 14);
        assert_eq!(engine.desk.patch[59], 15);
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("pw-metadata"), "{joined}");
        assert!(joined.contains("492"), "{joined}");
        assert!(joined.contains("target.object"), "{joined}");
        assert!(joined.contains("Spa:Id"), "{joined}");
        assert!(calls.iter().any(|args| args.windows(2).any(|pair| pair[0] == "target.object" && pair[1] == "8")), "{joined}");
        assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("move-sink-input")), "{joined}");
    }

    #[test]
    fn sending_toward_a_recording_program_loads_its_own_sink_and_leaves_the_patch() {
        let (mut engine, fake) = rig();
        engine.refresh_programs().unwrap();
        engine.programs.push(Program {
            id: "c:6".into(),
            label: "Discord".into(),
            role: pipewire::Role::Capture,
            target: String::new(),
            pactl_index: 6,
            node_id: 0,
        });
        let before = engine.desk.patch.clone();
        fake.calls.lock().unwrap().clear();
        engine.route_program("p:4", "hear:discord").unwrap();
        assert_eq!(engine.desk.patch, before);
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("crimson_discord"), "{joined}");
        assert!(joined.contains("crimson_discord.monitor"), "{joined}");
        assert!(!joined.contains("crimson_desk"), "{joined}");
        assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("pw-metadata")), "{joined}");
    }

    fn with_dummy(engine: &mut Engine) {
        engine.settings.dummies.push(settings::DummyHost::new("Dummy", "dummy"));
    }

    #[test]
    fn a_dummy_target_moves_the_stream_and_does_not_patch() {
        let (mut engine, fake) = rig();
        with_dummy(&mut engine);
        let before = engine.desk.patch.clone();
        fake.calls.lock().unwrap().clear();
        engine.route_program("p:4", "dummy:dummy").unwrap();
        assert_eq!(engine.desk.patch, before);
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("crimson_dummy_dummy"), "{joined}");
        assert!(!joined.contains("crimson_desk"), "{joined}");
        let load_at = calls.iter().position(|args| args.iter().any(|arg| arg == "load-module")).unwrap();
        let move_at = calls.iter().position(|args| args.first().map(String::as_str) == Some("move-sink-input")).unwrap();
        assert!(load_at < move_at);
        assert!(calls.iter().all(|args| args.first().map(String::as_str) != Some("pw-link")), "{joined}");
    }

    #[test]
    fn dummy_out_links_one_channel_before_the_patch_and_keeps_the_other() {
        let (mut engine, fake) = rig();
        with_dummy(&mut engine);
        let left = topology::line_slot(2);
        let right = topology::line_slot(3);
        engine.desk.patch[left as usize] = 78;
        let right_was = engine.desk.patch[right as usize];
        fake.calls.lock().unwrap().clear();
        engine.connect_dummy_out("dummy", left, false).unwrap();
        assert_eq!(engine.desk.patch[left as usize], 2);
        assert_eq!(engine.desk.patch[right as usize], right_was);
        assert_eq!(engine.settings.dummies[0].outs.len(), 1);
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("monitor_FL"), "{joined}");
        assert!(joined.contains("playback_FL"), "{joined}");
        assert!(!joined.contains("monitor_FR"), "{joined}");
        let link_at = calls.iter().position(|args| args.first().map(String::as_str) == Some("pw-link")).unwrap();
        let load_at = calls.iter().position(|args| args.iter().any(|arg| arg == "load-module")).unwrap();
        assert!(load_at < link_at);
    }

    #[test]
    fn a_failed_link_does_not_patch() {
        let fake = Arc::new(Fake { calls: StdMutex::new(Vec::new()), fail_pw_link: true });
        let mut engine = Engine::from_parts(Settings::default(), None, fake);
        with_dummy(&mut engine);
        let before = engine.desk.patch.clone();
        let err = engine.connect_dummy_out("dummy", topology::line_slot(2), false).unwrap_err();
        assert!(err.contains("link"), "{err}");
        assert_eq!(engine.desk.patch, before);
        assert!(engine.settings.dummies[0].outs.is_empty());
    }

    #[test]
    fn stereo_adds_the_partner_and_a_second_jack_keeps_the_first() {
        let (mut engine, fake) = rig();
        with_dummy(&mut engine);
        engine.connect_dummy_out("dummy", topology::line_slot(2), true).unwrap();
        assert_eq!(engine.desk.patch[topology::line_slot(2) as usize], 2);
        assert_eq!(engine.desk.patch[topology::line_slot(3) as usize], 3);
        let main_right = engine.desk.patch[topology::line_slot(1) as usize];
        engine.connect_dummy_out("dummy", topology::line_slot(0), false).unwrap();
        assert_eq!(engine.desk.patch[topology::line_slot(0) as usize], 0);
        assert_eq!(engine.desk.patch[topology::line_slot(1) as usize], main_right);
        assert_eq!(engine.desk.patch[topology::line_slot(2) as usize], 2);
        assert_eq!(engine.settings.dummies[0].outs.len(), 3);
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("monitor_FR"), "{joined}");
        assert!(joined.contains("HiFi__Line1__sink:playback_FL"), "{joined}");
    }

    #[test]
    fn a_mic_mixes_in_without_a_patch_and_a_self_link_is_not_planned() {
        let (mut engine, fake) = rig();
        with_dummy(&mut engine);
        let before = engine.desk.patch.clone();
        engine.feed_dummy_from_source("dummy", u16::from(topology::local_source(0)), false).unwrap();
        assert_eq!(engine.desk.patch, before);
        assert_eq!(engine.settings.dummies[0].captures, vec!["Mic1".to_string()]);
        let err = engine.feed_dummy_from_dummy("dummy", "dummy", false).unwrap_err();
        assert!(err.contains("loop"), "{err}");
        let calls = fake.calls.lock().unwrap();
        let joined = calls.iter().map(|args| args.join(" ")).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("capture_MONO"), "{joined}");
        assert!(joined.contains("playback_FL"), "{joined}");
        assert!(!joined.contains("playback_FR"), "{joined}");
        assert_eq!(calls.iter().filter(|args| args.first().map(String::as_str) == Some("pw-link")).count(), 1);
    }
}

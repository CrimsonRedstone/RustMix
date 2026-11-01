//! The desk window, laid out like the mixer the 828 is used with.
//! Monitor, phones, and output trims move one stored step per click, and only here.
//! Preamp, pad, phantom, and talkback locks stay on the control page and are not written.

mod fx;
mod home;
mod mixer;
mod pages;
mod pc;
mod routing;

use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::engine::{Hub, Link, LockChange};
use crate::ipc;
use crate::plan::{Cmd, Origin};
use crate::session;
use crate::settings;
use crate::topology;

pub(crate) const SIDEBAR: egui::Color32 = egui::Color32::from_rgb(17, 21, 24);
pub(crate) const CONTENT: egui::Color32 = egui::Color32::from_rgb(27, 29, 28);
pub(crate) const CARD: egui::Color32 = egui::Color32::from_rgb(22, 24, 26);
pub(crate) const LINE: egui::Color32 = egui::Color32::from_rgb(92, 98, 106);
pub(crate) const WHITE: egui::Color32 = egui::Color32::from_rgb(244, 244, 244);
pub(crate) const DIM: egui::Color32 = egui::Color32::from_rgb(127, 127, 127);
pub(crate) const SELECTED: egui::Color32 = egui::Color32::from_rgb(54, 58, 62);
pub(crate) const HOVER: egui::Color32 = egui::Color32::from_rgb(40, 44, 48);
pub(crate) const CYAN: egui::Color32 = egui::Color32::from_rgb(0, 206, 224);
pub(crate) const CYAN_DIM: egui::Color32 = egui::Color32::from_rgb(0, 92, 104);
pub(crate) const CRIMSON: egui::Color32 = egui::Color32::from_rgb(255, 58, 58);
pub(crate) const TAG_SHADOW: egui::Color32 = egui::Color32::from_rgb(154, 18, 18);
pub(crate) const WARN: egui::Color32 = egui::Color32::from_rgb(239, 183, 28);
pub(crate) const TRACK: egui::Color32 = egui::Color32::from_rgb(18, 20, 22);
pub(crate) const BLUE_PILL: egui::Color32 = egui::Color32::from_rgb(32, 128, 214);
pub(crate) const UNAVAILABLE: &str = "That control is not available here.";
const NAV_W: f32 = 220.0;
const NAV_NARROW: f32 = 42.0;
const TAB_H: f32 = 50.0;

pub enum Exit {
    Quit,
    AlreadyRunning,
}

fn window_icon() -> Option<std::sync::Arc<egui::IconData>> {
    let decoded = image::load_from_memory(include_bytes!("../../assets/rmce.png")).ok()?.to_rgba8();
    let width = decoded.width();
    let height = decoded.height();
    Some(std::sync::Arc::new(egui::IconData {
        rgba: decoded.into_raw(),
        width,
        height,
    }))
}

pub fn run() -> Result<Exit, String> {
    let hub = Hub::new(crate::engine::Engine::new());
    match ipc::spawn(Arc::clone(&hub)) {
        Ok(()) => {}
        Err(ipc::SpawnError::AlreadyRunning) => return Ok(Exit::AlreadyRunning),
        Err(ipc::SpawnError::Failed(message)) => return Err(message),
    }
    let screens = settings::load_screens();
    let (pinned, hint) = {
        let mut engine = hub.lock();
        match settings::apply_screen_plan(&mut engine.settings, &screens) {
            Ok(true) => {
                if let Err(err) = engine.settings.save() {
                    engine.notice = err;
                }
            }
            Ok(false) => {}
            Err(err) => {
                if let Err(save) = engine.settings.save() {
                    engine.notice = save;
                } else {
                    engine.notice = err;
                }
            }
        }
        (engine.settings.screen.is_some(), settings::open_frame(&engine.settings, &screens))
    };
    crate::mcp::sync(Arc::clone(&hub));
    if pinned {
        settings::reconfigure_kwin();
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(hint.and_then(|frame| frame.w.zip(frame.h)).map_or(egui::vec2(1040.0, 480.0), |(w, h)| egui::vec2(w, h)))
        .with_min_inner_size([720.0, 420.0])
        .with_title(crate::APP_NAME);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    if let Some(frame) = hint {
        viewport = viewport.with_position(egui::pos2(frame.x, frame.y));
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let checked = std::time::Instant::now();
    eframe::run_native(
        crate::PROCESS_NAME,
        options,
        Box::new(move |_cc| {
            let mut app = DeskApp::new(hub);
            app.screens = Some(screens);
            app.screen_checked = Some(checked);
            Ok(Box::new(app))
        }),
    )
    .map(|()| Exit::Quit)
    .map_err(|err| err.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Home,
    Device,
    Control,
    Input,
    Output,
    Routing,
    Discovery,
    Mix,
    Pc,
}

#[derive(Clone, Copy)]
pub(crate) enum Drag {
    Gain { bus: u16, input: Option<u16>, linear: f32 },
    Pan { bus: u16, input: u16, pan: f32 },
}

#[derive(Clone, Copy)]
pub(crate) enum Hold {
    Free,
    Locked,
    Frozen,
}

pub(crate) enum Bar {
    Idle,
    Moved(f32),
    Released,
    Double,
    Rejected,
}

#[derive(Clone, Copy)]
pub(crate) struct Pick {
    pub from_dest: bool,
    pub id: u16,
}

pub(crate) struct Retract {
    pub from: egui::Pos2,
    pub to: egui::Pos2,
    pub started: f64,
}

pub(crate) struct Snap {
    pub desk: Arc<crate::desk::Desk>,
    pub link: Link,
    pub notice: String,
    pub loop_text: String,
    pub programs: Vec<crate::pipewire::Program>,
    pub connecting: bool,
    pub launch: bool,
    pub start_minimized: bool,
    pub close_quits: bool,
    pub screen_key: Option<String>,
    pub locks: crate::settings::Locks,
    pub program_locks: Vec<crate::settings::ProgramLock>,
    pub dummies: Vec<crate::settings::DummyHost>,
    pub mcp_enabled: bool,
    pub mcp_host: String,
    pub mcp_port: u16,
    pub mcp_listen: String,
}

pub(crate) struct MixRow {
    pub bus: u16,
    pub label: String,
    pub color: egui::Color32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rename {
    Device,
    Input(u16),
    Output(u16),
}

#[derive(Clone, Copy)]
pub(crate) enum AbButton {
    On,
    A,
    B,
    Both,
}

/// A route that would replace what an output is playing, or that can howl.
/// Nothing is moved until the person chooses Send it.
#[derive(Clone)]
pub(crate) struct RouteAsk {
    pub text: String,
    pub job: RouteJob,
}

#[derive(Clone)]
pub(crate) enum RouteJob {
    Program { id: String, target: String },
    DummyOut { slug: String, slot: u16, stereo: bool },
    DummyCapture { slug: String, source: u16, stereo: bool },
    DummyLoop { from_slug: String, into_slug: String, stereo: bool },
    DummyListen { id: String, slug: String },
    RemoveDummy { slug: String },
}

/// A dummy cable drawn last frame. A click pulls that one cable off.
#[derive(Clone)]
pub(crate) enum DummyCable {
    Out { slug: String, slot: u16 },
    Capture { slug: String, device: String },
    Loop { into_slug: String, from_slug: String },
}

pub(crate) struct DeskApp {
    pub hub: Arc<Hub>,
    page: Page,
    pub bus: u16,
    pub all_inputs: bool,
    /// Bus column open. A collapsed column keeps the master fader and hides the name.
    pub bus_strip_open: bool,
    /// The bus EQ / reverb plate is open. It edits the on-screen model and is not sent.
    pub focus_bus: bool,
    /// Which plate the open strip is showing.
    pub plate: fx::Plate,
    /// Mic, line, and bus effects. Kept off the device model so a dump cannot replace them.
    pub fx: fx::Board,
    /// Bus whose plate is open. Input plates use `focus` instead.
    pub fx_bus: u16,
    pub drag: Option<Drag>,
    close_latch: bool,
    saw_minimized: bool,
    sent_minimize: bool,
    styled: bool,
    pub screens: Option<Vec<settings::Screen>>,
    screen_checked: Option<std::time::Instant>,
    opened_at: std::time::Instant,
    kwin_next: std::time::Instant,
    kwin_tries: u8,
    kwin_done: bool,
    programs_loaded: bool,
    programs_at: std::time::Instant,
    pub route_ask: Option<RouteAsk>,
    pub src_pos: HashMap<u16, egui::Pos2>,
    pub dst_pos: HashMap<u16, egui::Pos2>,
    /// Program jack centers. The id is the PipeWire stream, not a patch slot.
    pub pc_pos: HashMap<String, egui::Pos2>,
    /// Pick ids at and above the program base, in the order the jacks were addressed.
    pub pc_ids: Vec<String>,
    pub pc_cable_hits: Vec<(String, egui::Pos2, egui::Pos2)>,
    pub pc_cable_click: Option<String>,
    pub dummy_cable_hits: Vec<(DummyCable, egui::Pos2, egui::Pos2)>,
    pub dummy_cable_click: Option<DummyCable>,
    nav_open: bool,
    mix_open: bool,
    /// Scroll the hardware row to the top once after opening that part of the list.
    nav_pinned: Option<Page>,
    /// Last frame's cord geometry, in screen points. A click uses this, which is what was on screen.
    cable_hits: Vec<(u16, egui::Pos2, egui::Pos2)>,
    /// Set when the click this frame is on a cord, so the jack under it does not also arm.
    cable_click: Option<u16>,
    /// Gray sidebar thumb from the previous frame, so a drag can start on it.
    nav_thumb: Option<egui::Rect>,
    nav_span: f32,
    nav_travel: f32,
    nav_thumb_drag: bool,
    pub(crate) force_quit: bool,
    pub focus: Option<u16>,
    pub mic_insert: bool,
    pub src_bank: u8,
    pub dst_bank: u8,
    /// Stereo pairs the partner jack on connect and on detach. Off until the routing chip is turned on.
    pub stereo: bool,
    pub arm: Option<Pick>,
    pub retract: Option<Retract>,
    pub was_drag: bool,
    pub landed: bool,
    pub drag_from: Option<egui::Pos2>,
    pub drag_to: Option<egui::Pos2>,
    /// Home input-monitoring note.
    pub im_info: bool,
    /// Mix input meters. Local only. The 828 has no property for this.
    pub meter_post: bool,
    pub name_edit: Option<String>,
    pub rename: Option<Rename>,
    /// Some(true) is the foot-switch up key. Some(false) is the down key.
    pub foot_arm: Option<bool>,
    /// The 828 is looked for once, when the window opens.
    sought_device: bool,
    mcp_host_buf: String,
    mcp_port_buf: String,
    pc_audio: pc::Audio,
    logo: Option<egui::TextureHandle>,
    face: Option<egui::TextureHandle>,
    wordmark: Option<egui::TextureHandle>,
    /// Last desk handed to a frame. Meter pictures update it in place.
    painted: Option<Arc<crate::desk::Desk>>,
    painted_gen: u64,
}

impl DeskApp {
    fn new(hub: Arc<Hub>) -> Self {
        let mcp_host_buf = hub.lock().settings.mcp_host.clone();
        let mcp_port_buf = hub.lock().settings.mcp_port.to_string();
        Self {
            hub,
            page: Page::Home,
            bus: 0,
            all_inputs: true,
            bus_strip_open: true,
            focus_bus: false,
            plate: fx::Plate::Eq,
            fx: fx::Board::demo(),
            fx_bus: 0,
            drag: None,
            close_latch: false,
            saw_minimized: false,
            sent_minimize: false,
            styled: false,
            screens: None,
            screen_checked: None,
            opened_at: std::time::Instant::now(),
            kwin_next: std::time::Instant::now() + std::time::Duration::from_millis(400),
            kwin_tries: 0,
            kwin_done: false,
            programs_loaded: false,
            programs_at: std::time::Instant::now(),
            route_ask: None,
            src_pos: HashMap::new(),
            dst_pos: HashMap::new(),
            pc_pos: HashMap::new(),
            pc_ids: Vec::new(),
            pc_cable_hits: Vec::new(),
            pc_cable_click: None,
            dummy_cable_hits: Vec::new(),
            dummy_cable_click: None,
            nav_open: true,
            mix_open: false,
            nav_pinned: None,
            cable_hits: Vec::new(),
            cable_click: None,
            nav_thumb: None,
            nav_span: 1.0,
            nav_travel: 1.0,
            nav_thumb_drag: false,
            force_quit: false,
            focus: None,
            mic_insert: false,
            src_bank: 0,
            dst_bank: 0,
            stereo: false,
            arm: None,
            retract: None,
            was_drag: false,
            landed: false,
            drag_from: None,
            drag_to: None,
            im_info: false,
            meter_post: false,
            name_edit: None,
            rename: None,
            foot_arm: None,
            sought_device: false,
            mcp_host_buf,
            mcp_port_buf,
            pc_audio: pc::Audio::new(),
            logo: None,
            face: None,
            wordmark: None,
            painted: None,
            painted_gen: 0,
        }
    }

    pub(crate) fn snapshot(&mut self) -> Snap {
        let mut engine = self.hub.lock();
        engine.drain();
        let desk = if self.painted.is_none() || self.painted_gen != engine.desk_gen {
            let desk = Arc::new(engine.desk.clone());
            self.painted = Some(Arc::clone(&desk));
            self.painted_gen = engine.desk_gen;
            desk
        } else {
            let slot = self.painted.as_mut().expect("painted desk");
            if Arc::get_mut(slot).is_some() {
                let meters = engine.desk.gear.meters.clone();
                Arc::get_mut(slot).expect("painted desk").gear.meters = meters;
            } else {
                *slot = Arc::new(engine.desk.clone());
            }
            Arc::clone(slot)
        };
        Snap {
            desk,
            link: engine.link.clone(),
            notice: engine.notice.clone(),
            loop_text: engine.loop_text.clone(),
            programs: engine.programs.clone(),
            connecting: engine.connecting,
            launch: engine.settings.launch_at_login,
            start_minimized: engine.settings.start_minimized,
            close_quits: engine.settings.close_quits,
            screen_key: engine.settings.screen_key.clone(),
            locks: engine.settings.locks.clone(),
            program_locks: engine.settings.program_locks.clone(),
            dummies: engine.settings.dummies.clone(),
            mcp_enabled: engine.settings.mcp_enabled,
            mcp_host: engine.settings.mcp_host.clone(),
            mcp_port: engine.settings.mcp_port,
            mcp_listen: engine.mcp_listen.clone(),
        }
    }

    pub(crate) fn set_mcp_enabled(&mut self, enabled: bool) {
        let err = self.hub.lock().set_mcp_enabled(enabled).err();
        self.finish_mcp(err);
    }

    pub(crate) fn set_mcp_host(&mut self, host: &str) {
        let err = self.hub.lock().set_mcp_host(host).err();
        self.mcp_host_buf = self.hub.lock().settings.mcp_host.clone();
        self.finish_mcp(err);
    }

    pub(crate) fn set_mcp_port_text(&mut self, text: &str) {
        let parsed = text.trim().parse::<u16>().ok().filter(|port| *port != 0);
        let err = match parsed {
            Some(port) => self.hub.lock().set_mcp_port(port).err(),
            None => Some("The port is a number from 1 to 65535.".into()),
        };
        self.mcp_port_buf = self.hub.lock().settings.mcp_port.to_string();
        self.finish_mcp(err);
    }

    fn finish_mcp(&mut self, err: Option<String>) {
        if let Some(err) = err {
            self.hub.lock().notice = err;
        }
        crate::mcp::sync(Arc::clone(&self.hub));
    }

    fn reload_screens(&mut self) {
        let fresh = settings::load_screens();
        let mut engine = self.hub.lock();
        match settings::apply_screen_plan(&mut engine.settings, &fresh) {
            Ok(true) => {
                if let Err(err) = engine.settings.save() {
                    engine.notice = err;
                }
            }
            Ok(false) => {}
            Err(err) => {
                if let Err(save) = engine.settings.save() {
                    engine.notice = save;
                } else {
                    engine.notice = err;
                }
            }
        }
        drop(engine);
        self.screens = Some(fresh);
        self.screen_checked = Some(std::time::Instant::now());
    }

    pub(crate) fn choose_screen(&mut self, screen: &settings::Screen) {
        let frame = settings::desk_frame();
        let (w, h, rx, ry) = if let Some(frame) = frame {
            let from = self.screens.as_ref().and_then(|list| {
                list.iter().find(|item| settings::contains_center(item, frame.x + frame.w / 2.0, frame.y + frame.h / 2.0))
            });
            let (rx, ry) = from.map(|item| (frame.x - item.geo.x, frame.y - item.geo.y)).unwrap_or((0.0, 0.0));
            (frame.w, frame.h, rx, ry)
        } else {
            (1040.0, 480.0, 0.0, 0.0)
        };
        let w = w.clamp(320.0_f32.min(screen.geo.w), screen.geo.w.max(1.0));
        let h = h.clamp(240.0_f32.min(screen.geo.h), screen.geo.h.max(1.0));
        let x = screen.geo.x + rx.clamp(0.0, (screen.geo.w - w).max(0.0));
        let y = screen.geo.y + ry.clamp(0.0, (screen.geo.h - h).max(0.0));
        if let Err(err) = settings::move_desk(x, y, w, h) {
            self.hub.lock().notice = err;
            return;
        }
        let place = [x - screen.geo.x, y - screen.geo.y, w, h];
        let mut engine = self.hub.lock();
        if let Err(err) = engine.lock_screen(screen.key.clone(), screen.kwin_index, place) {
            engine.notice = err;
        }
    }

    fn locked_screen(&self) -> Option<settings::Screen> {
        let key = self.hub.lock().settings.screen_key.clone()?;
        self.screens.as_ref().and_then(|list| list.iter().find(|screen| screen.key == key)).cloned()
    }

    fn settle_place(&mut self) {
        if self.kwin_tries >= 4 {
            self.kwin_done = true;
            return;
        }
        if std::time::Instant::now() < self.kwin_next {
            return;
        }
        self.kwin_tries += 1;
        self.kwin_next = std::time::Instant::now() + Duration::from_millis(700);
        let Some(screen) = self.locked_screen() else {
            self.kwin_done = true;
            return;
        };
        let saved = self.hub.lock().settings.place;
        if let Some(place) = saved {
            let w = place[2].clamp(320.0, screen.geo.w.max(320.0));
            let h = place[3].clamp(240.0, screen.geo.h.max(240.0));
            let x = screen.geo.x + place[0].clamp(0.0, (screen.geo.w - w).max(0.0));
            let y = screen.geo.y + place[1].clamp(0.0, (screen.geo.h - h).max(0.0));
            let _ = settings::move_desk(x, y, w, h);
            self.kwin_done = true;
            return;
        }
        let Some(frame) = settings::desk_frame() else { return };
        if let Some(place) = settings::place_on(&screen, frame.x, frame.y, frame.w, frame.h) {
            let _ = self.hub.lock().remember_place(place);
            self.kwin_done = true;
        }
    }

    fn remember_window(&mut self) {
        let Some(screen) = self.locked_screen() else { return };
        let Some(frame) = settings::desk_frame() else { return };
        let Some(place) = settings::place_on(&screen, frame.x, frame.y, frame.w, frame.h) else { return };
        let _ = self.hub.lock().remember_place(place);
    }

    pub(crate) fn act(&self, cmd: Cmd) {
        let mut engine = self.hub.lock();
        let rate_change = match &cmd {
            Cmd::SampleRate { rate } => engine.desk.gear.sample_rate != *rate && engine.desk.gear.sample_rate != 0,
            _ => false,
        };
        if let Err(err) = engine.call(Origin::Person, &[cmd]) {
            engine.notice = err;
        } else if rate_change {
            engine.notice = "The 828 resets USB for a new sample rate. Playback sounds metallic until PipeWire reopens at that rate.".into();
        }
    }

    pub(crate) fn act_all(&self, cmds: &[Cmd]) {
        let mut engine = self.hub.lock();
        if let Err(err) = engine.call(Origin::Person, cmds) {
            engine.notice = err;
        }
    }

    pub(crate) fn lock(&self, change: LockChange) {
        let mut engine = self.hub.lock();
        if let Err(err) = engine.set_lock(change) {
            engine.notice = err;
        }
    }

    pub(crate) fn note(&self, text: impl Into<String>) {
        self.hub.lock().notice = text.into();
    }

    /// Look for the 828 once. A missing unit leaves Connect at the top of the window.
    fn seek_device(&mut self) {
        if self.sought_device {
            return;
        }
        self.sought_device = true;
        let busy = {
            let engine = self.hub.lock();
            engine.connecting || matches!(engine.link, Link::Live | Link::ReadOnly { .. })
        };
        if busy {
            return;
        }
        if let Err(err) = session::begin(&self.hub) {
            self.note(err);
        }
    }

    pub(crate) fn writable(&self, snap: &Snap) -> bool {
        snap.link.writable() && !snap.connecting
    }

    pub(crate) fn hold(&self, snap: &Snap, locked: bool) -> Hold {
        if !self.writable(snap) {
            Hold::Frozen
        } else if locked {
            Hold::Locked
        } else {
            Hold::Free
        }
    }

    pub(crate) fn preview_gain(&self, bus: u16, input: Option<u16>) -> Option<f32> {
        match self.drag {
            Some(Drag::Gain { bus: have, input: which, linear }) if have == bus && which == input => Some(linear),
            _ => None,
        }
    }

    pub(crate) fn preview_pan(&self, bus: u16, input: u16) -> Option<f32> {
        match self.drag {
            Some(Drag::Pan { bus: have, input: which, pan }) if have == bus && which == input => Some(pan),
            _ => None,
        }
    }

    pub(crate) fn take_gain(&mut self, event: Bar, bus: u16, input: Option<u16>, locked: bool) {
        match event {
            Bar::Moved(t) => {
                self.drag = Some(Drag::Gain { bus, input, linear: (t * 4.0).clamp(0.0, 4.0) });
            }
            Bar::Released => {
                if let Some(Drag::Gain { bus: have, input: which, linear }) = self.drag.take() {
                    if have == bus && which == input {
                        self.commit_gain(bus, input, linear);
                    }
                }
            }
            Bar::Double => {
                self.drag = None;
                self.commit_gain(bus, input, 1.0);
            }
            Bar::Rejected => {
                self.drag = None;
                if locked {
                    self.note("That level is locked.");
                } else {
                    self.note(format!("{} is read-only on this unit.", crate::APP_NAME));
                }
            }
            Bar::Idle => {}
        }
    }

    pub(crate) fn commit_gain(&self, bus: u16, input: Option<u16>, linear: f32) {
        match input {
            Some(input) => self.act(Cmd::Mix { bus, input, gain: Some(linear), db: None, pan: None, mute: None, solo: None }),
            None => self.act(Cmd::Bus { bus, gain: Some(linear), db: None, mute: None }),
        }
    }

    pub(crate) fn take_pan(&mut self, event: Bar, bus: u16, input: u16) {
        match event {
            Bar::Moved(t) => self.drag = Some(Drag::Pan { bus, input, pan: t.clamp(0.0, 1.0) }),
            Bar::Released => {
                if let Some(Drag::Pan { bus: have, input: which, pan }) = self.drag.take() {
                    if have == bus && which == input {
                        self.act(Cmd::Mix { bus, input, gain: None, db: None, pan: Some(pan), mute: None, solo: None });
                    }
                }
            }
            Bar::Double => {
                self.drag = None;
                self.act(Cmd::Mix { bus, input, gain: None, db: None, pan: Some(0.5), mute: None, solo: None });
            }
            Bar::Rejected => {
                self.drag = None;
                self.note(format!("{} is read-only on this unit.", crate::APP_NAME));
            }
            Bar::Idle => {}
        }
    }

    pub(crate) fn begin_rename(&mut self, target: Rename, current: &str) {
        self.foot_arm = None;
        self.rename = Some(target);
        self.name_edit = Some(current.to_string());
    }

    pub(crate) fn edit_name(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let mut text = self.name_edit.clone().unwrap_or_default();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let response = child.add(
            egui::TextEdit::singleline(&mut text)
                .id(child.make_persistent_id("rename-field"))
                .desired_width((rect.width() - 4.0).max(40.0)),
        );
        self.name_edit = Some(text);
        let enter = child.input(|input| input.key_pressed(egui::Key::Enter));
        let escape = child.input(|input| input.key_pressed(egui::Key::Escape));
        if enter {
            self.commit_rename();
        } else if escape || response.clicked_elsewhere() {
            self.rename = None;
            self.name_edit = None;
        } else {
            response.request_focus();
        }
    }

    fn commit_rename(&mut self) {
        let Some(target) = self.rename.take() else { return };
        let text = self.name_edit.take().unwrap_or_default();
        let bytes = crate::protocol::text_bytes(&text);
        match target {
            Rename::Device => self.act(Cmd::DeviceName { bytes }),
            Rename::Input(index) => self.act(Cmd::ChannelName { output: false, index, bytes }),
            Rename::Output(index) => self.act(Cmd::ChannelName { output: true, index, bytes }),
        }
    }

    fn take_foot(&mut self, ctx: &egui::Context) {
        let up = self.foot_arm == Some(true);
        let event = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, mods)) = event else { return };
        ctx.input_mut(|input| {
            input.consume_key(mods, key);
        });
        if key == egui::Key::Escape {
            self.foot_arm = None;
            return;
        }
        if key == egui::Key::Backspace {
            self.foot_arm = None;
            self.act(Cmd::FootKey { up, bytes: crate::protocol::text_bytes("") });
            return;
        }
        let Some(token) = foot_token(key) else { return };
        let mut parts = Vec::new();
        if mods.shift {
            parts.push("Shift");
        }
        if mods.mac_cmd {
            parts.push("Meta");
        }
        if mods.ctrl {
            parts.push("Ctrl");
        }
        if mods.alt {
            parts.push("Alt");
        }
        parts.push(token);
        self.foot_arm = None;
        self.act(Cmd::FootKey { up, bytes: crate::protocol::text_bytes(&parts.join(":")) });
    }

    fn take_keys(&mut self, ctx: &egui::Context) {
        if self.foot_arm.is_some() {
            self.take_foot(ctx);
            return;
        }
        if self.rename.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let pressed = |key| ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
        if pressed(egui::Key::H) {
            self.page = Page::Home;
        } else if pressed(egui::Key::D) {
            self.page = Page::Device;
        } else if pressed(egui::Key::C) {
            self.page = Page::Control;
        } else if pressed(egui::Key::I) {
            self.page = Page::Input;
        } else if pressed(egui::Key::O) {
            self.page = Page::Output;
        } else if pressed(egui::Key::R) {
            self.page = Page::Routing;
        } else if pressed(egui::Key::M) {
            self.mix_open = !self.mix_open;
        } else if pressed(egui::Key::P) {
            self.go(Page::Pc);
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if !self.sent_minimize {
            self.sent_minimize = true;
            if self.hub.lock().settings.start_minimized {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        if self.force_quit {
            return;
        }
        let close_quits = self.hub.lock().settings.close_quits;
        let (requested, minimized) = ctx.input(|input| {
            let view = input.viewport();
            (view.close_requested(), view.minimized)
        });
        if minimized == Some(true) {
            self.saw_minimized = true;
        }
        if !close_quits && requested {
            if !self.close_latch {
                self.close_latch = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        } else if self.saw_minimized && minimized == Some(false) {
            self.close_latch = false;
            self.saw_minimized = false;
        }
        if self.hub.lock().connecting || self.retract.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn go(&mut self, page: Page) {
        if self.page != page && matches!(page, Page::Pc | Page::Mix) {
            self.nav_pinned = Some(page);
        }
        self.page = page;
        self.drag = None;
    }

    fn load_pictures(&mut self, ctx: &egui::Context) {
        if self.logo.is_none() {
            self.logo = load_png(ctx, "motu-logo", include_bytes!("../../assets/motu-logo.png"));
        }
        if self.face.is_none() {
            self.face = load_png(ctx, "828-face", include_bytes!("../../assets/828m5.png"));
        }
        if self.wordmark.is_none() {
            self.wordmark = load_png(ctx, "828-wordmark", include_bytes!("../../assets/828-wordmark.png"));
        }
    }

    fn apply_page_request(&mut self) {
        let pending = self.hub.lock().pending_page.take();
        if let Some((name, bus)) = pending {
            match name.as_str() {
                "home" => self.go(Page::Home),
                "device" => self.go(Page::Device),
                "control" => self.go(Page::Control),
                "input" => self.go(Page::Input),
                "output" => self.go(Page::Output),
                "routing" => self.go(Page::Routing),
                "discovery" => self.go(Page::Discovery),
                "pc" => self.go(Page::Pc),
                "mixer" => {
                    if let Some(bus) = bus {
                        let even = bus & !1;
                        let linked = pair_linked(&self.hub.lock().desk, even);
                        // A stereo pair is one mix. An unlinked odd bus is its own mix.
                        self.bus = if linked { even } else { bus };
                    }
                    self.mix_open = true;
                    self.go(Page::Mix);
                }
                _ => {}
            }
        }
        let page = page_name(self.page).to_string();
        let bus = self.bus;
        let mut engine = self.hub.lock();
        engine.shown_page = page;
        engine.shown_bus = bus;
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, snap: &Snap) {
        // The whole column scrolls, the way the reference tab list does.
        // Discovery and the demo line sit in that list, so they cannot cover a row.
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let panel = ui.max_rect();
        let thumb_id = ui.make_persistent_id("nav-thumb");
        let mut thumb_delta = 0.0;
        if self.nav_open {
            if let Some(thumb) = self.nav_thumb {
                let layer = egui::LayerId::new(egui::Order::Foreground, thumb_id);
                let over = ui.new_child(egui::UiBuilder::new().max_rect(panel).layer_id(layer));
                let response = over.interact(thumb, thumb_id, egui::Sense::drag());
                self.nav_thumb_drag = response.dragged();
                if response.dragged() && self.nav_travel > 1.0 {
                    thumb_delta = response.drag_delta().y * (self.nav_span / self.nav_travel);
                }
                if response.hovered() || response.dragged() {
                    over.ctx().set_cursor_icon(if response.dragged() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    });
                }
            }
        } else {
            self.nav_thumb_drag = false;
        }
        let rows = mix_rows(&snap.desk);
        let output = egui::ScrollArea::vertical()
            .id_salt("nav")
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .scroll_source(egui::scroll_area::ScrollSource {
                scroll_bar: false,
                drag: egui::scroll_area::DragScroll::Never,
                mouse_wheel: true,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if thumb_delta != 0.0 {
                    ui.scroll_with_delta_animation(egui::vec2(0.0, -thumb_delta), egui::style::ScrollAnimation::none());
                }
                // The wordmark sits inside the first row. A taller slot pushed every tab down.
                let (logo, logo_drag) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), TAB_H),
                    egui::Sense::CLICK | egui::Sense::DRAG,
                );
                drag_scroll(ui, &logo_drag);
                if self.nav_open {
                    if let Some(texture) = &self.logo {
                        let mark = egui::Rect::from_min_size(logo.left_top() + egui::vec2(60.0, 15.0), egui::vec2(120.0, 19.0));
                        paint_image(ui.painter(), texture, mark);
                        paint_unofficial(ui, mark);
                    }
                }
                // The pair sits left of the wordmark. The top of that column is the button.
                let hit_w = if self.nav_open { 48.0 } else { panel.width() };
                let arrows = egui::Rect::from_min_size(logo.left_top(), egui::vec2(hit_w, 50.0));
                if ui.interact(arrows, ui.make_persistent_id("nav-collapse"), egui::Sense::click()).clicked() {
                    self.nav_open = !self.nav_open;
                }
                paint_nav_arrows(ui.painter(), logo.left_top(), self.nav_open);
                paint_tag(ui, logo.left_top());
                self.nav_item(ui, Page::Home, "HOME", NavMark::Home);
                self.nav_item(ui, Page::Device, "DEVICE", NavMark::Gear);
                self.nav_item(ui, Page::Control, "CONTROL", NavMark::Letter("C"));
                self.nav_item(ui, Page::Input, "INPUT", NavMark::Letter("I"));
                self.nav_item(ui, Page::Output, "OUTPUT", NavMark::Letter("O"));
                self.nav_item(ui, Page::Routing, "ROUTING", NavMark::Letter("R"));
                self.mix_header(ui);
                if self.mix_open {
                    for row in &rows {
                        let selected = self.page == Page::Mix && self.bus == row.bus;
                        if mix_item(ui, row, selected, self.nav_open) {
                            self.bus = row.bus;
                            self.focus = None;
                            self.focus_bus = false;
                            self.fx.drag = None;
                            self.go(Page::Mix);
                        }
                    }
                }
                self.nav_item(ui, Page::Pc, "PC MIXER", NavMark::Speaker);
                let (rule, rule_drag) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 1.0),
                    egui::Sense::CLICK | egui::Sense::DRAG,
                );
                drag_scroll(ui, &rule_drag);
                ui.painter().hline(
                    rule.x_range(),
                    rule.center().y,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 44, 48)),
                );
                let _ = self.nav_item(ui, Page::Discovery, "DISCOVERY", NavMark::Globe);
                if self.port_row(ui, snap) {
                    if matches!(snap.link, Link::Live | Link::ReadOnly { .. }) {
                        self.hub.lock().release();
                    } else if let Err(err) = session::begin(&self.hub) {
                        self.note(err);
                    }
                }
                // Below the list, so it does not open a gap between PC Mixer and Discovery.
                if self.nav_open {
                    let demo = ui.add_sized(
                        [ui.available_width(), 22.0],
                        egui::Label::new(egui::RichText::new(link_line(&snap.link)).color(DIM).size(11.0))
                            .truncate()
                            .sense(egui::Sense::CLICK | egui::Sense::DRAG),
                    );
                    drag_scroll(ui, &demo);
                }
            });
        let offset = output.state.clamped_offset().y;
        let view_h = output.inner_rect.height();
        let content_h = output.content_size.y;
        let over_menu = ui.rect_contains_pointer(panel);
        self.nav_thumb = None;
        if self.nav_open && (over_menu || self.nav_thumb_drag) {
            if let Some((thumb, span, travel)) = nav_thumb_rect(panel, offset, content_h, view_h) {
                ui.painter().rect_filled(thumb, 8.0, egui::Color32::from_rgb(89, 92, 94));
                self.nav_thumb = Some(thumb);
                self.nav_span = span;
                self.nav_travel = travel;
            }
        }
    }

    /// One sidebar control. It opens the 828, or lets the port go.
    fn port_row(&mut self, ui: &mut egui::Ui, snap: &Snap) -> bool {
        let (label, hot) = match &snap.link {
            Link::Live | Link::ReadOnly { .. } => ("DISCONNECT", true),
            _ if snap.connecting => ("CONNECTING", false),
            _ => ("QUICK CONNECT", false),
        };
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), TAB_H),
            egui::Sense::CLICK | egui::Sense::DRAG,
        );
        drag_scroll(ui, &response);
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, HOVER);
        }
        let color = if hot { CYAN } else { DIM };
        let mark_pos = if self.nav_open {
            rect.left_center() + egui::vec2(22.0, 0.0)
        } else {
            rect.center()
        };
        draw_plug(ui.painter(), mark_pos, color);
        if self.nav_open {
            ui.painter().text(
                rect.left_center() + egui::vec2(44.0, 0.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(14.0),
                color,
            );
        }
        response.clicked() && !snap.connecting
    }

    fn nav_item(&mut self, ui: &mut egui::Ui, page: Page, label: &str, mark: NavMark) -> bool {
        let selected = self.page == page;
        let (clicked, _) = tab_row(ui, selected, self.nav_open, label, mark);
        if clicked {
            self.go(page);
        }
        clicked
    }

    fn mix_header(&mut self, ui: &mut egui::Ui) {
        // Lit while any mix is showing, including when the bus list is collapsed.
        let on = self.page == Page::Mix;
        let (clicked, rect) = tab_row(ui, on, self.nav_open, "", NavMark::Letter("M"));
        if self.nav_open && matches!(self.nav_pinned, Some(Page::Pc | Page::Mix)) {
            // Reveal the section if it is clipped. Pinning it to the top hid the logo.
            ui.scroll_to_rect(rect, None);
            self.nav_pinned = None;
        }
        if self.nav_open {
            let ink = if on { WHITE } else { DIM };
            ui.painter().text(
                rect.left_center() + egui::vec2(44.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "HARDWARE MIXER",
                egui::FontId::proportional(13.0),
                ink,
            );
            draw_mix_chevron(ui.painter(), rect.right_center() - egui::vec2(22.0, 0.0), self.mix_open, ink);
        }
        if clicked {
            self.mix_open = !self.mix_open;
        }
    }

    fn banners(&self, ui: &mut egui::Ui, snap: &Snap) {
        if snap.connecting {
            banner(ui, CYAN, "Opening the 828. This demo will be replaced, and edits here are not sent.");
        }
        // The live 828 is shown on the wordmark. This notice is not a second banner.
        if !snap.notice.is_empty()
            && snap.notice != "Demo desk. Nothing is sent to the 828."
            && snap.notice != "Connected to the 828."
        {
            banner(ui, WHITE, &snap.notice);
        }
        if !snap.loop_text.is_empty() {
            banner(ui, WARN, &snap.loop_text);
        }
        if snap.locks.all && self.page == Page::Mix {
            banner(ui, CRIMSON, "Every level is locked.");
        }
    }
}

impl eframe::App for DeskApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.styled {
            self.styled = true;
            apply_style(ctx);
        }
        {
            let mut slot = self.hub.ctx.lock().unwrap_or_else(|err| err.into_inner());
            if slot.is_none() {
                *slot = Some(ctx.clone());
            }
        }
        let show = ctx.input(|input| input.viewport().minimized != Some(true));
        self.hub.set_show_live(show);
        self.hub.lock().drain();
        self.seek_device();
        self.load_pictures(ctx);
        self.apply_page_request();
        self.handle_close(ctx);
        if !self.kwin_done {
            ctx.request_repaint_after(Duration::from_millis(400));
            if self.opened_at.elapsed() >= Duration::from_millis(400) {
                self.settle_place();
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.take_keys(ui.ctx());
        pc::service(self, ui.ctx());
        if self.page == Page::Routing && !self.programs_loaded {
            self.programs_loaded = true;
            self.programs_at = std::time::Instant::now();
            let mut engine = self.hub.lock();
            if let Err(err) = engine.refresh_programs() {
                engine.notice = err;
            }
        }
        // The list is what the route menu and the jacks use. Skip it while a
        // question is open so that question still names the same stream.
        if self.page == Page::Routing && self.route_ask.is_none() {
            ui.ctx().request_repaint_after(Duration::from_secs(2));
            if self.programs_at.elapsed() >= Duration::from_secs(2) {
                self.programs_at = std::time::Instant::now();
                let mut engine = self.hub.lock();
                if let Err(err) = engine.refresh_programs() {
                    engine.notice = err;
                }
            }
        }
        let stale = self.screen_checked.is_none_or(|checked| checked.elapsed() >= Duration::from_secs(2));
        if self.page == Page::Control && (self.screens.is_none() || stale) {
            self.reload_screens();
        }
        if self.page == Page::Control {
            ui.ctx().request_repaint_after(Duration::from_secs(2));
        }
        let snap = self.snapshot();
        let width = if self.nav_open { NAV_W } else { NAV_NARROW };
        let frame = egui::Frame::NONE.fill(SIDEBAR);
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(width)
            .frame(frame)
            .show_separator_line(false)
            .show(ui, |ui| {
            self.sidebar(ui, &snap);
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(CONTENT)).show(ui, |ui| {
            let full = ui.max_rect();
            let inner = full.shrink2(egui::vec2(18.0, 12.0));
            let mut page = ui.new_child(egui::UiBuilder::new().max_rect(inner));
            self.banners(&mut page, &snap);
            let home_scroll = match self.page {
                Page::Home => Some(home::show(self, &mut page, &snap)),
                Page::Device => {
                    pages::device(self, &mut page, &snap);
                    None
                }
                Page::Control => {
                    pages::control(self, &mut page, &snap);
                    None
                }
                Page::Input => {
                    // The other pages keep an 18px side inset. This block is a fixed
                    // 760px canvas with a 50px margin, and that inset clips it.
                    let top = page.cursor().min.y;
                    let input_rect = egui::Rect::from_min_max(
                        egui::pos2(full.left(), top),
                        egui::pos2(full.right(), inner.bottom()),
                    );
                    let mut input_page = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
                    pages::input(self, &mut input_page, &snap);
                    None
                }
                Page::Output => {
                    // Same fixed canvas as Input. An 18px side inset clips the 685px block.
                    let top = page.cursor().min.y;
                    let output_rect = egui::Rect::from_min_max(
                        egui::pos2(full.left(), top),
                        egui::pos2(full.right(), inner.bottom()),
                    );
                    let mut output_page = ui.new_child(egui::UiBuilder::new().max_rect(output_rect));
                    pages::output(self, &mut output_page, &snap);
                    None
                }
                Page::Routing => {
                    routing::show(self, &mut page, &snap);
                    None
                }
                Page::Discovery => {
                    pages::discovery(&mut page, &snap);
                    None
                }
                Page::Mix => {
                    // The strips run to the sidebar and to the top of the page.
                    // A notice still keeps its place above them.
                    let cursor = page.cursor().min.y;
                    let top = if cursor > full.top() + 14.0 { cursor } else { full.top() };
                    let mix_rect = egui::Rect::from_min_max(
                        egui::pos2(full.left(), top),
                        egui::pos2(full.right(), full.bottom()),
                    );
                    let mut mix_page = ui.new_child(egui::UiBuilder::new().max_rect(mix_rect));
                    mixer::show(self, &mut mix_page, &snap);
                    None
                }
                Page::Pc => {
                    pc::show(self, &mut page);
                    None
                }
            };
            if let Some(scroll) = home_scroll {
                paint_home_thumb(ui, full, &scroll);
            }
            // Cast after the page, or an opaque fill covers it and the edge reads as a border.
            paint_edge_shadow(ui.painter(), full.left(), full.top(), full.bottom(), true);
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.remember_window();
        self.hub.lock().release();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        CONTENT.to_normalized_gamma_f32()
    }
}

#[derive(Clone, Copy)]
enum NavMark {
    Home,
    Gear,
    Globe,
    Letter(&'static str),
    Speaker,
}

fn drag_scroll(ui: &egui::Ui, response: &egui::Response) {
    // A drag on the menu follows the pointer. A click that does not move still selects the row.
    if response.dragged() {
        ui.scroll_with_delta_animation(egui::vec2(0.0, response.drag_delta().y), egui::style::ScrollAnimation::none());
    }
}

/// Gray pill on the right of the whole sidebar. Height is 98% of viewport/content, as in the reference.
fn nav_thumb_rect(panel: egui::Rect, offset: f32, content_h: f32, view_h: f32) -> Option<(egui::Rect, f32, f32)> {
    if content_h <= view_h + 1.0 || view_h < 24.0 {
        return None;
    }
    let ratio = view_h / content_h;
    let thumb_h = (ratio * 0.98 * view_h).clamp(24.0, view_h);
    let span = (content_h - view_h).max(1.0);
    let travel = (view_h - thumb_h).max(1.0);
    let top = panel.top() + (offset / span).clamp(0.0, 1.0) * travel;
    let thumb = egui::Rect::from_min_size(egui::pos2(panel.right() - 10.0, top), egui::vec2(10.0, thumb_h));
    Some((thumb, span, travel))
}

fn tab_row(ui: &mut egui::Ui, selected: bool, open: bool, label: &str, mark: NavMark) -> (bool, egui::Rect) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), TAB_H), egui::Sense::CLICK | egui::Sense::DRAG);
    drag_scroll(ui, &response);
    let fill = if selected {
        SELECTED
    } else if response.hovered() {
        HOVER
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    let color = if selected { WHITE } else { DIM };
    let mark_pos = if open { rect.left_center() + egui::vec2(22.0, 0.0) } else { rect.center() };
    match mark {
        NavMark::Home => draw_home(ui.painter(), mark_pos, color),
        NavMark::Gear => draw_gear(ui.painter(), mark_pos, color),
        NavMark::Globe => draw_globe(ui.painter(), mark_pos, color),
        NavMark::Letter(letter) => {
            ui.painter().text(mark_pos, egui::Align2::CENTER_CENTER, letter, egui::FontId::proportional(15.0), color);
        }
        NavMark::Speaker => pc::draw_speaker(ui.painter(), mark_pos, color),
    }
    if open {
        ui.painter().text(
            rect.left_center() + egui::vec2(44.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            color,
        );
    }
    (response.clicked(), rect)
}

fn mix_item(ui: &mut egui::Ui, row: &MixRow, selected: bool, open: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::CLICK | egui::Sense::DRAG);
    drag_scroll(ui, &response);
    if selected {
        ui.painter().rect_filled(rect, 0.0, SELECTED);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, HOVER);
    }
    let bar = egui::Rect::from_center_size(rect.left_center() + egui::vec2(if open { 28.0 } else { rect.width() * 0.5 }, 0.0), egui::vec2(16.0, 3.0));
    ui.painter().rect_filled(bar, 1.0, row.color);
    if open {
        ui.painter().text(
            rect.left_center() + egui::vec2(44.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &row.label,
            egui::FontId::proportional(13.0),
            if selected { WHITE } else { egui::Color32::from_rgb(190, 190, 190) },
        );
    }
    response.clicked()
}

pub(crate) fn mix_rows(desk: &crate::desk::Desk) -> Vec<MixRow> {
    let mut rows = Vec::new();
    add_span(&mut rows, desk, 0, 2);
    add_span(&mut rows, desk, 10, 4);
    add_span(&mut rows, desk, 2, 8);
    add_span(&mut rows, desk, 14, 2);
    rows
}

fn add_span(rows: &mut Vec<MixRow>, desk: &crate::desk::Desk, start: u16, count: u16) {
    let mut bus = start;
    let end = start + count;
    while bus < end {
        if pair_linked(desk, bus) && bus + 1 < end {
            rows.push(MixRow { bus, label: output_mix_label(desk, bus, true), color: bus_color(bus) });
            bus += 2;
        } else {
            rows.push(MixRow { bus, label: output_mix_label(desk, bus, false), color: bus_color(bus) });
            bus += 1;
        }
    }
}

fn trimmed_name(names: &[String], index: usize) -> String {
    names.get(index).map(|text| text.trim_matches('\0').trim().to_string()).unwrap_or_default()
}

/// Custom output name for a strip. Empty means the fixed label still applies.
pub(crate) fn bus_custom(desk: &crate::desk::Desk, bus: u16, stereo: bool) -> String {
    let left = trimmed_name(&desk.gear.out_names, bus as usize);
    if !stereo {
        return left;
    }
    let right = trimmed_name(&desk.gear.out_names, bus as usize + 1);
    if left.is_empty() {
        right
    } else if right.is_empty() || left == right {
        left
    } else {
        format!("{left} / {right}")
    }
}

pub(crate) fn output_mix_label(desk: &crate::desk::Desk, bus: u16, linked: bool) -> String {
    let custom = bus_custom(desk, bus, linked);
    if !custom.is_empty() {
        format!("{custom} MIX")
    } else if linked {
        pair_label(bus)
    } else {
        format!("{} MIX", single_name(bus))
    }
}

pub(crate) fn input_caption(desk: &crate::desk::Desk, input: u16) -> String {
    let custom = trimmed_name(&desk.gear.in_names, input as usize);
    if custom.is_empty() {
        topology::input_name(input as usize).to_string()
    } else {
        custom
    }
}

pub(crate) fn paint_level(painter: &egui::Painter, rect: egui::Rect, db: Option<f32>) {
    painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
    let Some(db) = db.filter(|db| db.is_finite()) else { return };
    let t = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    if t <= 0.0 {
        return;
    }
    let height = (rect.height() * t).max(1.0);
    let fill = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom() - height), egui::vec2(rect.width(), height));
    let color = if db >= 0.0 {
        egui::Color32::from_rgb(210, 40, 40)
    } else if db >= -12.0 {
        egui::Color32::from_rgb(220, 180, 40)
    } else {
        egui::Color32::from_rgb(40, 180, 60)
    };
    painter.rect_filled(fill, 0.0, color);
}

pub(crate) fn meter_of(gear: &crate::gear::Gear, index: Option<usize>) -> Option<f32> {
    gear.meter_db(index?)
}

/// Level at a jack, taken from the source patched into it. The mix page keeps `bus_meter`.
pub(crate) fn jack_level(gear: &crate::gear::Gear, patch: &[u8], slot: u16) -> Option<f32> {
    let source = patch.get(slot as usize).copied().unwrap_or(topology::NOTHING);
    meter_of(gear, topology::jack_meter(source))
}

pub(crate) fn preamp_view(gear: &crate::gear::Gear, index: u16) -> (f32, String) {
    let max = crate::gear::preamp_max(index).unwrap_or(1).max(1);
    match gear.preamp.get(index as usize).copied().flatten() {
        Some(value) => (value as f32 / f32::from(max), format!("{value} dB")),
        None => (0.0, "—".into()),
    }
}

pub(crate) fn arc_louder(ui: &egui::Ui, center_y: f32) -> bool {
    ui.input(|input| input.pointer.interact_pos().is_none_or(|pos| pos.y < center_y))
}

pub(crate) fn ab_next(a: bool, b: bool, which: AbButton) -> (u8, u8, u8) {
    let (a, b) = match which {
        AbButton::On => {
            if a || b {
                (false, false)
            } else {
                (true, false)
            }
        }
        AbButton::A => (!(a && !b), false),
        AbButton::B => (false, !(b && !a)),
        AbButton::Both => {
            if a && b {
                (false, false)
            } else {
                (true, true)
            }
        }
    };
    (u8::from(a || b), u8::from(a), u8::from(b))
}

pub(crate) fn foot_label(stored: &str) -> String {
    let text = stored.trim_matches('\0').trim();
    if text.is_empty() {
        "None".into()
    } else {
        text.replace(':', "+")
    }
}

fn foot_token(key: egui::Key) -> Option<&'static str> {
    Some(match key {
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::Space => "Space",
        egui::Key::Enter => "Enter",
        egui::Key::Tab => "Tab",
        egui::Key::ArrowLeft => "ArrowLeft",
        egui::Key::ArrowRight => "ArrowRight",
        egui::Key::ArrowUp => "ArrowUp",
        egui::Key::ArrowDown => "ArrowDown",
        egui::Key::Minus => "Minus",
        egui::Key::Plus | egui::Key::Equals => "Equal",
        _ => return None,
    })
}

pub(crate) fn pair_linked(desk: &crate::desk::Desk, bus: u16) -> bool {
    bus % 2 == 0 && desk.bus_link.get(bus as usize).copied().unwrap_or(0) != 0
}

pub(crate) fn pair_label(bus: u16) -> String {
    match bus {
        0 => "MAIN 1-2 MIX".into(),
        2 => "LINE 3-4 MIX".into(),
        4 => "LINE 5-6 MIX".into(),
        6 => "LINE 7-8 MIX".into(),
        8 => "LINE 9-10 MIX".into(),
        10 => "PHONES 1 MIX".into(),
        12 => "PHONES 2 MIX".into(),
        14 => "REVERB MIX".into(),
        _ => format!("{} MIX", single_name(bus)),
    }
}

pub(crate) fn single_name(bus: u16) -> String {
    match bus {
        0 => "MAIN 1".into(),
        1 => "MAIN 2".into(),
        2..=9 => format!("LINE {}", bus + 1),
        10 => "PHONES 1 L".into(),
        11 => "PHONES 1 R".into(),
        12 => "PHONES 2 L".into(),
        13 => "PHONES 2 R".into(),
        14 => "REVERB L".into(),
        15 => "REVERB R".into(),
        _ => format!("BUS {bus}"),
    }
}

pub(crate) fn bus_color(bus: u16) -> egui::Color32 {
    const COLORS: [[u8; 3]; 16] = [
        [200, 200, 200],
        [200, 200, 200],
        [255, 122, 34],
        [255, 122, 34],
        [230, 200, 50],
        [230, 200, 50],
        [99, 210, 165],
        [99, 210, 165],
        [1, 146, 248],
        [1, 146, 248],
        [214, 23, 1],
        [214, 23, 1],
        [214, 23, 1],
        [214, 23, 1],
        [154, 109, 176],
        [154, 109, 176],
    ];
    let rgb = COLORS.get(bus as usize).copied().unwrap_or([180, 180, 180]);
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub(crate) fn link_line(link: &Link) -> String {
    match link {
        Link::Demo => "Demo. Nothing is sent to the 828.".into(),
        Link::Live => "Connected to the 828.".into(),
        Link::ReadOnly { reason } | Link::Offline { reason } => reason.clone(),
    }
}

pub(crate) fn banner(ui: &mut egui::Ui, color: egui::Color32, text: &str) {
    let frame = egui::Frame::NONE
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, color))
        .inner_margin(8.0)
        .corner_radius(6.0);
    frame.show(ui, |ui| {
        ui.label(egui::RichText::new(text).color(color));
    });
    ui.add_space(6.0);
}

fn paint_home_thumb(ui: &egui::Ui, panel: egui::Rect, scroll: &home::HomeScroll) {
    // The reference thumb is rgb(89,92,94), 10px wide, radius 8, and 98% of viewport/content.
    // The ratio was matched with 58px counted outside content_h (12px inset + 46px title).
    // The title and the 6px gap under it now scroll inside content_h, so 6px remains outside.
    let above = 6.0;
    let view_h = panel.height();
    let content_h = scroll.content_h + above;
    if content_h <= view_h + 1.0 {
        return;
    }
    let ratio = view_h / content_h;
    // 0.98 is the reference thumb. The last 7px keeps the painted pill on the measured bar.
    let thumb_h = (ratio * 0.98 * view_h - 7.0).max(24.0);
    let max_off = (scroll.content_h - scroll.view_h).max(1.0);
    let travel = (view_h - thumb_h).max(0.0);
    let top = panel.top() + (scroll.offset / max_off) * travel;
    // A 10px pill antialiases one pixel past the reference's left edge.
    let thumb = egui::Rect::from_min_size(egui::pos2(panel.right() - 8.8, top), egui::vec2(8.8, thumb_h));
    ui.painter().rect_filled(thumb, 8.0, egui::Color32::from_rgb(89, 92, 94));
}

pub(crate) fn unit_open(snap: &Snap) -> bool {
    matches!(snap.link, Link::Live | Link::ReadOnly { .. })
}

pub(crate) fn device_title(ui: &mut egui::Ui, wordmark: Option<&egui::TextureHandle>, live: bool) {
    // The product mark is always "828". The device name is a field on the Device page.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::hover());
    paint_device_rule(ui, wordmark, live, rect, rect.top() + 29.2);
}

/// The 828 rule. `rect` is the same horizontal span Home and Device already use.
pub(crate) fn paint_device_rule(ui: &egui::Ui, wordmark: Option<&egui::TextureHandle>, live: bool, rect: egui::Rect, rule_y: f32) {
    // The PNG is the tight ink. The reference ink box is about 128 wide,
    // centered on the rule, and inset from the right. Keep the PNG's aspect.
    // 232×72 picture, with 3px of empty fringe around the 226×66 ink.
    // 131.4 keeps that ink at the same 128-wide box as the reference.
    let mark_w = 131.4;
    let mark_h = wordmark.map(|texture| {
        let size = texture.size_vec2();
        mark_w * size.y / size.x.max(1.0)
    }).unwrap_or(mark_w * (72.0 / 232.0));
    let mark = egui::Rect::from_min_size(
        // The picture has a few pixels of empty fringe, so the ink sits 2.4px left of the reference until this nudge.
        egui::pos2(rect.right() - 15.0 - mark_w + 2.4, rule_y - mark_h * 0.5),
        egui::vec2(mark_w, mark_h),
    );
    if let Some(texture) = wordmark {
        paint_live_mark(ui, texture, mark, live);
    } else {
        ui.painter().text(mark.right_center(), egui::Align2::RIGHT_CENTER, "828", egui::FontId::proportional(28.0), WHITE);
    }
    // The reference rule stops at the logo canvas, which is wider than the ink, so the gap is large.
    ui.painter().line_segment(
        [egui::pos2(rect.left() + 16.0, rule_y), egui::pos2(mark.left() - 34.0, rule_y)],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(127, 127, 127)),
    );
}

fn paint_live_mark(ui: &egui::Ui, texture: &egui::TextureHandle, mark: egui::Rect, live: bool) {
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let painter = ui.painter();
    if !live {
        painter.image(texture.id(), mark, uv, egui::Color32::WHITE);
        return;
    }
    // A slow breath. The blue sits inside the strokes, with a faint edge of light.
    let wave = (ui.input(|input| input.time) * 1.5).sin() as f32 * 0.5 + 0.5;
    let minimized = ui.input(|input| input.viewport().minimized == Some(true));
    if !minimized {
        ui.ctx().request_repaint_after(Duration::from_millis(40));
    }
    let halo = egui::Color32::from_rgba_unmultiplied(70, 150, 220, (18.0 + 28.0 * wave) as u8);
    for offset in [
        egui::vec2(0.0, -1.3),
        egui::vec2(0.0, 1.3),
        egui::vec2(-1.3, 0.0),
        egui::vec2(1.3, 0.0),
    ] {
        painter.image(texture.id(), mark.translate(offset), uv, halo);
    }
    painter.image(texture.id(), mark, uv, egui::Color32::WHITE);
    let inner = mark.shrink(1.4 + wave * 0.7);
    let ink = egui::Color32::from_rgba_unmultiplied(80, 165, 235, (32.0 + 48.0 * wave) as u8);
    painter.image(texture.id(), inner, uv, ink);
}

pub(crate) fn card<R>(ui: &mut egui::Ui, title: &str, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, 12.0, CARD);
    ui.painter().rect_stroke(rect, 12.0, egui::Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
    let galley = ui.painter().layout_no_wrap(title.to_owned(), egui::FontId::proportional(14.0), egui::Color32::from_rgb(176, 182, 188));
    let title_pos = egui::pos2(rect.left() + 16.0, rect.top() - galley.size().y * 0.45);
    let bg = egui::Rect::from_min_size(title_pos - egui::vec2(4.0, 1.0), galley.size() + egui::vec2(8.0, 2.0));
    ui.painter().rect_filled(bg, 0.0, CONTENT);
    ui.painter().galley(title_pos, galley, egui::Color32::from_rgb(176, 182, 188));
    let inner = rect.shrink2(egui::vec2(12.0, 16.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    add(&mut child)
}

pub(crate) fn chip(ui: &mut egui::Ui, label: &str, on: bool, size: egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let stroke = if on { CYAN } else { egui::Color32::from_rgb(150, 156, 164) };
    let fill = if on { CYAN_DIM } else { egui::Color32::from_rgb(30, 32, 34) };
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.2, stroke), egui::StrokeKind::Inside);
    let text = if on { WHITE } else { egui::Color32::from_rgb(220, 220, 220) };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.5), text);
    response
}

pub(crate) fn pill(ui: &mut egui::Ui, label: &str, on: bool, size: egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if on {
        ui.painter().rect_filled(rect, 5.0, BLUE_PILL);
    } else {
        ui.painter().rect_stroke(rect, 5.0, egui::Stroke::new(1.2, egui::Color32::from_rgb(150, 156, 164)), egui::StrokeKind::Inside);
    }
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), WHITE);
    response
}

pub(crate) fn lock_button(ui: &mut egui::Ui, locked: bool, label: &str, size: egui::Vec2) -> bool {
    let response = chip(ui, label, locked, size);
    response.clicked()
}

pub(crate) fn padlock_button(ui: &mut egui::Ui, locked: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    paint_padlock(ui.painter(), rect, locked);
    response.clicked()
}

pub(crate) fn paint_padlock(painter: &egui::Painter, rect: egui::Rect, locked: bool) {
    // Open is green. Closed is red. Same chip the reference draws on a knob.
    let fill = if locked {
        egui::Color32::from_rgb(210, 31, 31)
    } else {
        egui::Color32::from_rgb(31, 157, 74)
    };
    painter.rect_filled(rect, 3.0, fill);
    let center = rect.center();
    let body = egui::Rect::from_center_size(center + egui::vec2(0.0, 2.4), egui::vec2(8.0, 6.0));
    painter.rect_stroke(body, 1.0, egui::Stroke::new(1.15, WHITE), egui::StrokeKind::Inside);
    let hinge = center + egui::vec2(if locked { 0.0 } else { 1.4 }, -1.2);
    let mut points = Vec::with_capacity(8);
    let steps = 7;
    for index in 0..=steps {
        let angle = std::f32::consts::PI + index as f32 * std::f32::consts::PI / steps as f32;
        points.push(hinge + egui::vec2(angle.cos(), angle.sin()) * 3.3);
    }
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.25, WHITE)));
}

pub(crate) fn meters(ui: &mut egui::Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, height), egui::Sense::hover());
    for index in 0..2 {
        let x = rect.left() + index as f32 * 8.0;
        let bar = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(5.0, height - 6.0));
        ui.painter().rect_filled(bar, 1.0, egui::Color32::from_rgb(8, 8, 8));
        let cap = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - 5.0), egui::vec2(5.0, 4.0));
        ui.painter().rect_filled(cap, 1.0, egui::Color32::from_rgb(120, 24, 24));
    }
}

pub(crate) fn arc_with_lock(ui: &mut egui::Ui, fraction: f32, locked: bool, size: egui::Vec2, radius: f32) -> bool {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    paint_arc(ui.painter(), rect.center(), radius, fraction.clamp(0.0, 1.0));
    let lock = egui::Rect::from_min_size(
        egui::pos2(rect.center().x + radius - 2.0, rect.center().y - radius - 2.0),
        egui::vec2(16.0, 14.0),
    );
    let response = ui.interact(lock, ui.scope_id().with(("arc-lock", rect.left() as i32, rect.top() as i32)), egui::Sense::click());
    paint_padlock(ui.painter(), lock, locked);
    response.clicked()
}

pub(crate) fn arc_box(ui: &mut egui::Ui, fraction: f32, size: egui::Vec2, radius: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    paint_arc(ui.painter(), rect.center(), radius, fraction.clamp(0.0, 1.0));
    response
}

pub(crate) fn paint_arc(painter: &egui::Painter, center: egui::Pos2, radius: f32, fraction: f32) {
    let start = 135.0_f32.to_radians();
    let sweep = 270.0_f32.to_radians();
    let steps = 32;
    let mut track = Vec::with_capacity(steps + 1);
    let mut hot = Vec::new();
    for index in 0..=steps {
        let t = index as f32 / steps as f32;
        let angle = start + sweep * t;
        let point = center + egui::vec2(angle.cos(), angle.sin()) * radius;
        track.push(point);
        if t <= fraction + f32::EPSILON {
            hot.push(point);
        }
    }
    painter.add(egui::Shape::line(track, egui::Stroke::new(5.0, egui::Color32::from_rgb(48, 52, 56))));
    if hot.len() >= 2 {
        painter.add(egui::Shape::line(hot, egui::Stroke::new(5.0, CYAN)));
    }
}

pub(crate) fn stored_fraction(stored: Option<u8>) -> f32 {
    match stored {
        Some(value) => 1.0 - (value as f32 / 100.0),
        None => 0.0,
    }
}

pub(crate) fn v_fader(ui: &mut egui::Ui, height: f32, shown_linear: f32, color: egui::Color32, hold: Hold) -> Bar {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(36.0, height), egui::Sense::click_and_drag());
    let follow = matches!(hold, Hold::Free);
    let mut t = (shown_linear / 4.0).clamp(0.0, 1.0);
    let mut event = Bar::Idle;
    if response.double_clicked() && follow {
        event = Bar::Double;
        t = 0.25;
    } else if !follow && (response.drag_started() || response.dragged() || response.double_clicked()) {
        event = Bar::Rejected;
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            t = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            event = Bar::Moved(t);
        }
    } else if response.drag_stopped() && follow {
        event = Bar::Released;
    }
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(6.0, rect.height() - 8.0));
    ui.painter().rect_filled(track, 3.0, TRACK);
    let fill_h = track.height() * t;
    let fill = egui::Rect::from_min_size(egui::pos2(track.left(), track.bottom() - fill_h), egui::vec2(track.width(), fill_h));
    ui.painter().rect_filled(fill, 3.0, color);
    let knob_y = track.bottom() - track.height() * t;
    let knob = egui::pos2(track.center().x, knob_y);
    ui.painter().circle_filled(knob, 8.0, egui::Color32::from_rgb(16, 18, 20));
    ui.painter().circle_stroke(knob, 8.0, egui::Stroke::new(2.0, WHITE));
    event
}

pub(crate) fn monitor_text(desk: &crate::desk::Desk) -> String {
    desk.monitor.map(topology::stored_text).unwrap_or_else(|| "—".into())
}

pub(crate) fn pair_text(left: Option<u8>, right: Option<u8>) -> String {
    match (left, right) {
        (Some(left), Some(right)) if left == right => topology::stored_text(left),
        (Some(left), Some(right)) => format!("{} / {}", topology::stored_text(left), topology::stored_text(right)),
        _ => "—".into(),
    }
}

pub(crate) fn shown_inputs(all: bool) -> Vec<u16> {
    if all {
        (0..topology::INPUTS as u16).collect()
    } else {
        let mut ids: Vec<u16> = (0..12).collect();
        ids.push(28);
        ids.push(29);
        ids
    }
}

pub(crate) fn cable_curve(from: egui::Pos2, to: egui::Pos2) -> [egui::Pos2; 4] {
    let mid = (from.x + to.x) * 0.5;
    let sag = 28.0 + (from.y - to.y).abs() * 0.05;
    [from, egui::pos2(mid, from.y + sag), egui::pos2(mid, to.y + sag), to]
}

pub(crate) fn cable(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32) {
    let points = cable_curve(from, to);
    let sheath = egui::Stroke::new(7.0, egui::Color32::from_rgb(12, 14, 16));
    let core = egui::Stroke::new(2.6, color);
    let highlight = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(90));
    for stroke in [sheath, core, highlight] {
        painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
            points,
            false,
            egui::Color32::TRANSPARENT,
            stroke,
        )));
    }
    for end in [from, to] {
        painter.circle_filled(end, 5.5, egui::Color32::from_rgb(186, 190, 194));
        painter.circle_filled(end, 3.2, egui::Color32::from_rgb(70, 74, 78));
        painter.circle_filled(end, 1.6, color);
    }
}

fn paint_tag(ui: &egui::Ui, origin: egui::Pos2) {
    let text = "RustMix Crimson Edition";
    let font = egui::FontId { size: 13.0, family: egui::FontFamily::Name(std::sync::Arc::from("NotoSansBlack")) };
    // The same tight tracking as the edition line under the MOTU mark.
    let mut job = egui::text::LayoutJob::default();
    for (index, ch) in text.chars().enumerate() {
        job.append(
            &ch.to_string(),
            if index == 0 { 0.0 } else { -0.45 },
            egui::TextFormat { font_id: font.clone(), color: CRIMSON, ..Default::default() },
        );
    }
    let galley = ui.painter().layout_job(job);
    let pos = origin + egui::vec2(120.0, 46.0) - galley.size() * 0.5;
    let angle = -2.4_f32.to_radians();
    let mut shadow = egui::epaint::TextShape::new(pos + egui::vec2(1.1, 0.8), galley.clone(), TAG_SHADOW);
    shadow.angle = angle;
    shadow.override_text_color = Some(TAG_SHADOW);
    ui.painter().add(shadow);
    let mut fill = egui::epaint::TextShape::new(pos, galley, CRIMSON);
    fill.angle = angle;
    fill.override_text_color = Some(CRIMSON);
    ui.painter().add(fill);
}

fn page_name(page: Page) -> &'static str {
    match page {
        Page::Home => "home",
        Page::Device => "device",
        Page::Control => "control",
        Page::Input => "input",
        Page::Output => "output",
        Page::Routing => "routing",
        Page::Discovery => "discovery",
        Page::Mix => "mixer",
        Page::Pc => "pc",
    }
}

/// Sits on the MOTU mark, left edge on the M, just overlapping the top of the letter.
fn paint_unofficial(ui: &egui::Ui, mark: egui::Rect) {
    let text = "Unofficial";
    let font = egui::FontId { size: 8.5, family: egui::FontFamily::Name(std::sync::Arc::from("NotoSansBold")) };
    let mut job = egui::text::LayoutJob::default();
    for (index, ch) in text.chars().enumerate() {
        job.append(
            &ch.to_string(),
            if index == 0 { 0.0 } else { 0.2 },
            egui::TextFormat { font_id: font.clone(), color: CRIMSON, ..Default::default() },
        );
    }
    let galley = ui.painter().layout_job(job);
    // Ink of the M in the 480×76 raster, mapped onto the drawn mark.
    let m_left = mark.left() + mark.width() * (8.0 / 480.0);
    let m_top = mark.top() + mark.height() * (2.0 / 76.0);
    // The galley box continues through the descent. These letters have none,
    // so the baseline is the bottom of the ink. Rest that on the M.
    let mut baseline = galley.size().y;
    if let Some(row) = galley.rows.first() {
        if let Some(glyph) = row.glyphs.first() {
            baseline = row.pos.y + glyph.pos.y;
        }
    }
    let pos = egui::pos2(m_left, m_top - baseline + 1.0);
    let shadow_color = egui::Color32::from_black_alpha(160);
    let mut shadow = egui::epaint::TextShape::new(pos + egui::vec2(0.6, 0.5), galley.clone(), shadow_color);
    shadow.override_text_color = Some(shadow_color);
    ui.painter().add(shadow);
    let mut fill = egui::epaint::TextShape::new(pos, galley, CRIMSON);
    fill.override_text_color = Some(CRIMSON);
    ui.painter().add(fill);
}

fn load_png(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

pub(crate) fn paint_image(painter: &egui::Painter, texture: &egui::TextureHandle, rect: egui::Rect) {
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
}

/// Two diagonal arrows. Open points inward, toward collapsing the sidebar.
fn paint_nav_arrows(painter: &egui::Painter, origin: egui::Pos2, open: bool) {
    let upper = origin + egui::vec2(28.0, 19.0);
    let lower = origin + egui::vec2(16.0, 31.0);
    if open {
        paint_diag_arrow(painter, upper, false);
        paint_diag_arrow(painter, lower, true);
    } else {
        paint_diag_arrow(painter, upper, true);
        paint_diag_arrow(painter, lower, false);
    }
}

fn paint_diag_arrow(painter: &egui::Painter, center: egui::Pos2, up_right: bool) {
    // The outline indents along the shaft, so it is drawn as two convex pieces.
    let raw = [
        egui::vec2(-4.0, 4.0),
        egui::vec2(-3.2, 4.8),
        egui::vec2(0.8, 0.8),
        egui::vec2(3.0, 3.0),
        egui::vec2(4.0, -4.0),
        egui::vec2(-3.0, -3.0),
        egui::vec2(-0.8, -0.8),
        egui::vec2(-4.8, 3.2),
    ];
    let point = |index: usize| {
        let offset = raw[index];
        center + if up_right { offset } else { -offset }
    };
    let white = egui::Color32::WHITE;
    for indices in [[2, 3, 4, 5, 6], [2, 6, 7, 0, 1]] {
        let points = indices.into_iter().map(point).collect();
        painter.add(egui::Shape::convex_polygon(points, white, egui::Stroke::NONE));
    }
}

/// Filled triangle. `tip` is the reference point the menu uses, not the centroid.
fn draw_mix_chevron(painter: &egui::Painter, tip: egui::Pos2, open: bool, color: egui::Color32) {
    let points = if open {
        vec![tip + egui::vec2(-5.0, -2.0), tip + egui::vec2(5.0, -2.0), tip + egui::vec2(0.0, 4.0)]
    } else {
        vec![tip + egui::vec2(-4.0, -5.0), tip + egui::vec2(2.0, 0.0), tip + egui::vec2(-4.0, 5.0)]
    };
    painter.add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
}

/// Soft black falloff. Offset 3, blur about 6, half-strength black.
pub(crate) fn paint_edge_shadow(painter: &egui::Painter, edge_x: f32, top: f32, bottom: f32, to_the_right: bool) {
    const ALPHA: [u8; 9] = [100, 86, 68, 52, 36, 24, 14, 6, 3];
    let height = (bottom - top).max(0.0);
    for (index, alpha) in ALPHA.iter().enumerate() {
        let x = if to_the_right { edge_x + index as f32 } else { edge_x - 1.0 - index as f32 };
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(1.0, height)),
            0.0,
            egui::Color32::from_black_alpha(*alpha),
        );
    }
}

fn draw_globe(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.4, color);
    painter.circle_stroke(center, 7.0, stroke);
    painter.line_segment([center + egui::vec2(-7.0, 0.0), center + egui::vec2(7.0, 0.0)], stroke);
    painter.line_segment([center + egui::vec2(0.0, -7.0), center + egui::vec2(0.0, 7.0)], stroke);
    painter.circle_stroke(center + egui::vec2(-3.2, 0.0), 2.2, stroke);
    painter.circle_stroke(center + egui::vec2(3.2, 0.0), 2.2, stroke);
}

fn draw_plug(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.4, color);
    let body = egui::Rect::from_center_size(center + egui::vec2(1.6, 0.0), egui::vec2(8.0, 10.0));
    painter.rect_stroke(body, 1.5, stroke, egui::StrokeKind::Middle);
    painter.line_segment([body.left_center() + egui::vec2(0.0, -2.6), body.left_center() + egui::vec2(-4.2, -2.6)], stroke);
    painter.line_segment([body.left_center() + egui::vec2(0.0, 2.6), body.left_center() + egui::vec2(-4.2, 2.6)], stroke);
}

fn draw_home(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.6, color);
    let roof = [
        center + egui::vec2(-8.0, 1.0),
        center + egui::vec2(0.0, -7.0),
        center + egui::vec2(8.0, 1.0),
    ];
    painter.add(egui::Shape::line(roof.to_vec(), stroke));
    painter.rect_stroke(
        egui::Rect::from_center_size(center + egui::vec2(0.0, 4.0), egui::vec2(10.0, 8.0)),
        0.0,
        stroke,
        egui::StrokeKind::Middle,
    );
}

fn draw_gear(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    painter.circle_stroke(center, 6.5, egui::Stroke::new(1.5, color));
    painter.circle_stroke(center, 2.4, egui::Stroke::new(1.4, color));
    for index in 0..6 {
        let angle = index as f32 * std::f32::consts::TAU / 6.0;
        let dir = egui::vec2(angle.cos(), angle.sin());
        painter.line_segment([center + dir * 5.0, center + dir * 8.0], egui::Stroke::new(1.6, color));
    }
}

fn apply_style(ctx: &egui::Context) {
    // Electron asks for Arial. On this machine that is Noto Sans.
    // Named faces only: the rest of the UI stays on the default proportional font.
    // Button labels use Light, because Regular stems stay heavier in egui than in Chromium.
    let mut fonts = egui::FontDefinitions::default();
    let mut any = false;
    for (name, path) in [
        ("NotoSans", "/usr/share/fonts/noto/NotoSans-Light.ttf"),
        ("NotoSansReg", "/usr/share/fonts/noto/NotoSans-Regular.ttf"),
        ("NotoSansBold", "/usr/share/fonts/noto/NotoSans-Bold.ttf"),
        ("NotoSansBlack", "/usr/share/fonts/noto/NotoSans-Black.ttf"),
    ] {
        let Ok(bytes) = std::fs::read(path) else { continue };
        fonts.font_data.insert(name.to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.insert(
            egui::FontFamily::Name(std::sync::Arc::from(name)),
            vec![name.to_owned()],
        );
        any = true;
    }
    if any {
        ctx.set_fonts(fonts);
    }
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CONTENT;
    visuals.window_fill = CONTENT;
    visuals.extreme_bg_color = egui::Color32::from_rgb(14, 16, 18);
    visuals.faint_bg_color = CARD;
    visuals.override_text_color = Some(WHITE);
    visuals.hyperlink_color = CYAN;
    visuals.selection.bg_fill = CYAN_DIM;
    visuals.selection.stroke.color = WHITE;
    visuals.warn_fg_color = WARN;
    visuals.error_fg_color = CRIMSON;
    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(30, 32, 34);
    visuals.widgets.inactive.fg_stroke.color = WHITE;
    visuals.widgets.inactive.bg_stroke.color = LINE;
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.fg_stroke.color = WHITE;
    visuals.widgets.hovered.bg_stroke.color = CYAN;
    visuals.widgets.active.bg_fill = CYAN_DIM;
    visuals.widgets.active.fg_stroke.color = WHITE;
    ctx.set_visuals(visuals);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = 22.0;
    });
}

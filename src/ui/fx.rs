//! EQ, gate, compressor, and reverb for the hardware mixer.
//! The plate edits a local board. `settle` copies the 828 in, and sends the difference out.

use eframe::egui;

use super::{bus_custom, input_caption, pair_label, pair_linked, single_name, DIM, WHITE};
use crate::engine::Link;
use crate::gear;
use crate::plan::Cmd;

pub(crate) const PLATE_BG: egui::Color32 = egui::Color32::from_rgb(18, 22, 25);

const SPAN_EDIT: f32 = 20.0;
const SPAN_THUMB: f32 = 12.0;
const BAND_COLOR: [egui::Color32; 4] = [
    egui::Color32::from_rgb(0, 255, 251),
    egui::Color32::from_rgb(187, 157, 255),
    egui::Color32::from_rgb(167, 44, 88),
    egui::Color32::from_rgb(1, 114, 241),
];
const EQ_LABEL: egui::Color32 = egui::Color32::from_rgb(116, 117, 135);
const EQ_VALUE: egui::Color32 = egui::Color32::from_rgb(185, 191, 217);
const GRID: egui::Color32 = egui::Color32::from_rgb(63, 71, 82);
const GATE: egui::Color32 = egui::Color32::from_rgb(0, 207, 213);
const COMP: egui::Color32 = egui::Color32::from_rgb(239, 183, 28);
const GR_METER: egui::Color32 = egui::Color32::from_rgb(225, 65, 65);
const L_METER: egui::Color32 = egui::Color32::from_rgb(78, 165, 255);
const TOGGLE_IDLE: egui::Color32 = egui::Color32::from_rgb(158, 159, 160);
const DYN_TRACK: egui::Color32 = egui::Color32::from_rgb(35, 38, 44);
const REVERB_COLOR: egui::Color32 = egui::Color32::from_rgb(154, 109, 176);
const PREDELAY_COLOR: egui::Color32 = egui::Color32::from_rgb(171, 91, 246);
const DAMP_COLOR: egui::Color32 = egui::Color32::from_rgb(209, 131, 39);
const DECAY_COLOR: egui::Color32 = egui::Color32::from_rgb(45, 130, 194);
const WIDTH_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 200, 50);
const BLACK: egui::Color32 = egui::Color32::BLACK;

const DYN_MIN: f32 = -72.0;
const DYN_MAX: f32 = 0.0;
const ATT_MIN: f32 = 1.0;
const ATT_MAX: f32 = 100.0;
const REL_MIN: f32 = 50.0;
const REL_MAX: f32 = 2000.0;
const GATE_THR: f32 = -46.0;
const GATE_ATT: f32 = 10.0;
const GATE_REL: f32 = 1000.0;
const COMP_THR: f32 = -10.0;
const COMP_RATIO: f32 = 2.0;
const COMP_ATT: f32 = 10.0;
const COMP_REL: f32 = 1000.0;
const COMP_GAIN: f32 = 0.0;
const RATIO_MAX: f32 = 10.0;
const GAIN_MIN: f32 = -20.0;
const GAIN_MAX: f32 = 20.0;
const Q_MIN: f32 = 0.45;
const Q_MAX: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plate {
    Eq,
    Dyn,
    Reverb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Peak,
    LowShelf,
    HighShelf,
    HighPass,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Band {
    pub on: bool,
    pub mode: Mode,
    pub hz: f32,
    pub gain: f32,
    pub q: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Dyn {
    pub gate_on: bool,
    pub gate_db: f32,
    pub gate_attack: f32,
    pub gate_release: f32,
    pub comp_on: bool,
    pub comp_db: f32,
    pub ratio: f32,
    pub comp_attack: f32,
    pub comp_release: f32,
    pub makeup: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Reverb {
    pub preset: u8,
    pub decay: f32,
    pub damping: f32,
    pub predelay: f32,
    pub width: f32,
}

pub(crate) struct Board {
    pub input_eq: Vec<[Band; 4]>,
    pub bus_eq: Vec<[Band; 3]>,
    pub dyns: Vec<Dyn>,
    pub reverb: Reverb,
    pub selected: u8,
    pub drag: Option<u8>,
    hold: bool,
    /// The plate changed since the last adopt. Late device reports must not be written back.
    user_edit: bool,
    /// Desk this board was copied from. A new connection adopts before it publishes.
    seen: String,
    /// `desk_gen` last copied in. Meter pictures do not change it.
    adopted: u64,
}

impl Board {
    /// Every band starts bypassed at 0 dB, so turning one on does not jump.
    pub(crate) fn demo() -> Self {
        Self {
            input_eq: (0..10).map(|_| input_bands()).collect(),
            bus_eq: (0..14).map(|_| bus_bands()).collect(),
            dyns: vec![Dyn::quiet(); 10],
            reverb: Reverb::medium(),
            selected: 0,
            drag: None,
            hold: false,
            user_edit: false,
            seen: String::new(),
            adopted: 0,
        }
    }
}

impl Dyn {
    fn quiet() -> Self {
        Self {
            gate_on: false,
            gate_db: GATE_THR,
            gate_attack: GATE_ATT,
            gate_release: GATE_REL,
            comp_on: false,
            comp_db: COMP_THR,
            ratio: COMP_RATIO,
            comp_attack: COMP_ATT,
            comp_release: COMP_REL,
            makeup: COMP_GAIN,
        }
    }
}

impl Reverb {
    fn medium() -> Self {
        Self { preset: 1, decay: 55.0, damping: 50.0, predelay: 25.0, width: 50.0 }
    }
}

fn input_bands() -> [Band; 4] {
    [
        Band { on: false, mode: Mode::Peak, hz: 80.0, gain: 0.0, q: 1.0 },
        Band { on: false, mode: Mode::Peak, hz: 400.0, gain: 0.0, q: 1.0 },
        Band { on: false, mode: Mode::Peak, hz: 2000.0, gain: 0.0, q: 1.0 },
        Band { on: false, mode: Mode::HighShelf, hz: 8000.0, gain: 0.0, q: 1.0 },
    ]
}

fn bus_bands() -> [Band; 3] {
    [
        Band { on: false, mode: Mode::HighPass, hz: 40.0, gain: 0.0, q: 1.0 },
        Band { on: false, mode: Mode::Peak, hz: 1000.0, gain: 0.0, q: 1.0 },
        Band { on: false, mode: Mode::HighShelf, hz: 8000.0, gain: 0.0, q: 1.0 },
    ]
}

pub(crate) fn editor(app: &mut super::DeskApp, ctx: &egui::Context, rect: egui::Rect) {
    if rect.width() < 280.0 || rect.height() < 200.0 || !plate_ok(app) {
        return;
    }
    let plate = app.plate;
    egui::Area::new(egui::Id::unique("rm-fx-plate"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.left_top())
        .default_size(rect.size())
        .constrain(false)
        .movable(false)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_clip_rect(rect);
            ui.visuals_mut().override_text_color = Some(WHITE);
            let _eat = ui.interact(rect, ui.make_persistent_id("fx-plate"), egui::Sense::click_and_drag());
            ui.painter().rect_filled(rect, 0.0, PLATE_BG);
            let header = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 40.0));
            if header_bar(ui, app, header) {
                return;
            }
            let body = egui::Rect::from_min_max(egui::pos2(rect.left() + 8.0, header.bottom()), egui::pos2(rect.right() - 8.0, rect.bottom() - 6.0));
            match plate {
                Plate::Eq => eq_body(ui, app, body),
                Plate::Dyn => dyn_body(ui, app, body),
                Plate::Reverb => reverb_body(ui, app, body),
            }
        });
}

fn plate_ok(app: &super::DeskApp) -> bool {
    match app.plate {
        Plate::Eq => {
            if let Some(input) = app.focus {
                (input as usize) < app.fx.input_eq.len()
            } else {
                app.focus_bus && (app.fx_bus as usize) < app.fx.bus_eq.len()
            }
        }
        Plate::Dyn => app.focus.is_some_and(|input| (input as usize) < app.fx.dyns.len()),
        Plate::Reverb => app.focus_bus,
    }
}

fn header_bar(ui: &mut egui::Ui, app: &mut super::DeskApp, rect: egui::Rect) -> bool {
    let title = title_of(app);
    let close = egui::Rect::from_min_size(egui::pos2(rect.right() - 40.0, rect.center().y - 16.0), egui::vec2(32.0, 32.0));
    let mut title_x = rect.left() + 8.0;
    if wants_arrows(app) {
        let prev = egui::Rect::from_min_size(egui::pos2(rect.left() + 4.0, rect.center().y - 16.0), egui::vec2(32.0, 32.0));
        let next = egui::Rect::from_min_size(egui::pos2(prev.right() + 8.0, prev.top()), egui::vec2(32.0, 32.0));
        if arrow_button(ui, prev, true, "fx-prev") {
            step_focus(app, -1);
        }
        if arrow_button(ui, next, false, "fx-next") {
            step_focus(app, 1);
        }
        title_x = next.right() + 12.0;
    }
    let painter = ui.painter();
    painter.text(egui::pos2(title_x, rect.center().y), egui::Align2::LEFT_CENTER, &title, face("NotoSansBold", 16.0), WHITE);
    let banner = banner_of(app);
    if !banner.is_empty() {
        painter.text(
            egui::pos2(close.left() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            banner,
            face("NotoSans", 11.0),
            EQ_LABEL,
        );
    }
    if close_button(ui, close) {
        app.focus = None;
        app.focus_bus = false;
        app.fx.drag = None;
        true
    } else {
        false
    }
}

fn wants_arrows(app: &super::DeskApp) -> bool {
    match app.plate {
        Plate::Reverb => false,
        Plate::Dyn => app.focus.is_some(),
        Plate::Eq => app.focus.is_some() || app.focus_bus,
    }
}

fn step_focus(app: &mut super::DeskApp, dir: i32) {
    app.fx.drag = None;
    if app.focus.is_some() && !app.focus_bus {
        let current = app.focus.unwrap_or(0) as i32;
        app.focus = Some((current + dir).rem_euclid(10) as u16);
        return;
    }
    if app.plate == Plate::Eq && app.focus_bus {
        step_bus(app, dir);
    }
}

fn step_bus(app: &mut super::DeskApp, dir: i32) {
    let link = app.hub.lock().desk.bus_link.clone();
    let mut bus = app.fx_bus as i32;
    for _ in 0..14 {
        bus = (bus + dir).rem_euclid(14);
        let next = bus as u16;
        let paired = next % 2 == 1 && link.get((next & !1) as usize).copied().unwrap_or(0) != 0;
        if paired || (next as usize) >= app.fx.bus_eq.len() {
            continue;
        }
        app.fx_bus = next;
        app.bus = next;
        app.focus = None;
        app.focus_bus = true;
        app.plate = Plate::Eq;
        return;
    }
}

fn title_of(app: &super::DeskApp) -> String {
    let effect = match app.plate {
        Plate::Eq => "EQ",
        Plate::Dyn => "Dynamics",
        Plate::Reverb => "Reverb",
    };
    if let Some(input) = app.focus {
        if !app.focus_bus {
            let name = input_caption(&app.hub.lock().desk, input);
            return format!("{name} {effect}");
        }
    }
    format!("{} {effect}", mix_name(app))
}

fn mix_name(app: &super::DeskApp) -> String {
    let bus = app.fx_bus;
    let even = bus & !1;
    let desk = app.hub.lock().desk.clone();
    let linked = pair_linked(&desk, even);
    let custom = bus_custom(&desk, if linked { even } else { bus }, linked);
    if !custom.is_empty() {
        return custom;
    }
    if linked {
        pair_label(even).trim_end_matches(" MIX").to_string()
    } else {
        single_name(bus)
    }
}

fn banner_of(app: &super::DeskApp) -> String {
    match app.hub.lock().link {
        Link::Demo => "Demo. Nothing is sent.".into(),
        Link::Live => String::new(),
        Link::ReadOnly { .. } => "Read-only on this unit.".into(),
        Link::Offline { .. } => "Demo. Nothing is sent.".into(),
    }
}

fn eq_body(ui: &mut egui::Ui, app: &mut super::DeskApp, rect: egui::Rect) {
    let bus_eq = app.focus.is_none();
    let mut bands = if let Some(input) = app.focus {
        app.fx.input_eq[input as usize].to_vec()
    } else {
        app.fx.bus_eq[app.fx_bus as usize].to_vec()
    };
    let count = bands.len();
    let mut selected = (app.fx.selected as usize).min(count.saturating_sub(1));
    let mut drag = app.fx.drag.filter(|index| (*index as usize) < count);
    let graph_h = (rect.height() - 176.0).clamp(72.0, 280.0);
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 36.0, rect.top() + 4.0),
        egui::pos2(rect.right() - 8.0, rect.top() + 4.0 + graph_h),
    );
    drag_plot(ui, &mut bands, &mut selected, &mut drag, plot);
    paint_eq_plot(ui.painter(), plot, &bands, selected, SPAN_EDIT);
    paint_eq_labels(ui.painter(), plot);

    let controls = egui::Rect::from_min_max(egui::pos2(rect.left(), plot.bottom() + 18.0), rect.right_bottom());
    let col_w = controls.width() / count as f32;
    let last = count - 1;
    for index in 0..count {
        let col = egui::Rect::from_min_size(egui::pos2(controls.left() + index as f32 * col_w, controls.top()), egui::vec2(col_w, controls.height()));
        let ctrl_w = 132.0_f32.min(col_w - 8.0);
        let ctrl_x = col.left() + (col_w - ctrl_w) * 0.5;
        let color = BAND_COLOR[index % 4];
        let power = egui::Rect::from_min_size(egui::pos2(ctrl_x, col.top()), egui::vec2(ctrl_w, 26.0));
        if power_line(ui, power, bands[index].on, color, ("fx-pwr", index)).clicked() {
            bands[index].on = !bands[index].on;
            selected = index;
            drag = None;
        }
        let freq = egui::Rect::from_min_size(egui::pos2(ctrl_x, col.top() + 30.0), egui::vec2(ctrl_w, 22.0));
        let gain = egui::Rect::from_min_size(egui::pos2(ctrl_x, col.top() + 54.0), egui::vec2(ctrl_w, 22.0));
        let q = egui::Rect::from_min_size(egui::pos2(ctrl_x, col.top() + 78.0), egui::vec2(ctrl_w, 22.0));
        let hz_default = band_default_hz(bus_eq, index);
        let moved = readout(ui, freq, "Freq", bands[index].hz, &hz_text(bands[index].hz), true, hz_default, 20.0, 20_000.0, true, ("fx-hz", index));
        if (moved - bands[index].hz).abs() > f32::EPSILON {
            bands[index].hz = moved.clamp(20.0, 20_000.0);
            selected = index;
        }
        let gain_on = bands[index].mode != Mode::HighPass;
        let gain_v = readout(
            ui,
            gain,
            "Gain",
            bands[index].gain,
            &format!("{:+.1} dB", bands[index].gain),
            gain_on,
            0.0,
            GAIN_MIN,
            GAIN_MAX,
            false,
            ("fx-gain", index),
        );
        if gain_on && (gain_v - bands[index].gain).abs() > f32::EPSILON {
            bands[index].gain = gain_v.clamp(GAIN_MIN, GAIN_MAX);
            selected = index;
        }
        let q_on = bands[index].mode == Mode::Peak;
        let q_v = readout(ui, q, "Q", bands[index].q, &format!("{:.2}", bands[index].q), q_on, 1.0, Q_MIN, Q_MAX, false, ("fx-q", index));
        if q_on && (q_v - bands[index].q).abs() > f32::EPSILON {
            bands[index].q = q_v.clamp(Q_MIN, Q_MAX);
            selected = index;
        }
        let modes = mode_choices(bus_eq, index, last);
        if !modes.is_empty() {
            let row = egui::Rect::from_min_size(egui::pos2(ctrl_x, col.top() + 108.0), egui::vec2(ctrl_w, 30.0));
            if let Some(mode) = mode_row(ui, row, modes, bands[index].mode, color, index) {
                bands[index].mode = mode;
                selected = index;
            }
        }
    }
    app.fx.selected = selected as u8;
    app.fx.drag = drag;
    store_eq(app, &bands);
}

fn dyn_body(ui: &mut egui::Ui, app: &mut super::DeskApp, rect: egui::Rect) {
    let Some(input) = app.focus else { return };
    let index = input as usize;
    if index >= app.fx.dyns.len() {
        return;
    }
    let gate = input < 2;
    let before = app.fx.dyns[index];
    let mut item = before;
    if gate {
        let gate_w = rect.width() * 0.465;
        let gate_rect = egui::Rect::from_min_size(rect.left_top(), egui::vec2(gate_w - 6.0, rect.height()));
        let comp_rect = egui::Rect::from_min_max(egui::pos2(rect.left() + gate_w + 6.0, rect.top()), rect.right_bottom());
        gate_pane(ui, gate_rect, &mut item, index);
        comp_pane(ui, comp_rect, &mut item, index);
    } else {
        let width = rect.width() * 0.62;
        let x = rect.left() + (rect.width() - width) * 0.5;
        let comp_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, rect.height()));
        comp_pane(ui, comp_rect, &mut item, index);
    }
    if item != before {
        app.fx.user_edit = true;
        app.fx.dyns[index] = item;
        mirror_dyn(app, index);
    }
}

fn gate_pane(ui: &mut egui::Ui, rect: egui::Rect, item: &mut Dyn, index: usize) {
    let geom = pane_geom(rect);
    if section_power(ui, geom.power, "Gate", item.gate_on, GATE, ("fx-gate-pwr", index)) {
        item.gate_on = !item.gate_on;
    }
    if defaults_button(ui, geom.defaults, GATE, ("fx-gate-dft", index)) {
        apply_gate_defaults(item);
    }
    if item.gate_on {
        drag_threshold(ui, geom.graph, &mut item.gate_db, ("fx-gate-graph", index));
    }
    paint_dyn_frame(ui.painter(), geom.graph, geom.red, geom.blue);
    let color = live(GATE, item.gate_on);
    paint_dyn_curve(ui.painter(), geom.graph, color, 2.4, |db| sample_gate(item, db));
    let xs = knob_origins(rect, 3, 34.0, 400.0);
    let y = geom.knobs;
    item.gate_db = knob(ui, egui::pos2(xs[0], y), item.gate_db, knob_spec("Threshold", "dB", DYN_MIN, DYN_MAX, GATE_THR, 1, GATE, item.gate_on, false), ("gk", index, 0));
    item.gate_attack = knob(ui, egui::pos2(xs[1], y), item.gate_attack, knob_spec("Attack", "ms", ATT_MIN, ATT_MAX, GATE_ATT, 0, GATE, item.gate_on, false), ("gk", index, 1));
    item.gate_release = knob(ui, egui::pos2(xs[2], y), item.gate_release, knob_spec("Release", "ms", REL_MIN, REL_MAX, GATE_REL, 0, GATE, item.gate_on, false), ("gk", index, 2));
}

fn comp_pane(ui: &mut egui::Ui, rect: egui::Rect, item: &mut Dyn, index: usize) {
    let geom = pane_geom(rect);
    if section_power(ui, geom.power, "Compression", item.comp_on, COMP, ("fx-comp-pwr", index)) {
        item.comp_on = !item.comp_on;
    }
    if defaults_button(ui, geom.defaults, COMP, ("fx-comp-dft", index)) {
        apply_comp_defaults(item);
    }
    if item.comp_on {
        drag_threshold(ui, geom.graph, &mut item.comp_db, ("fx-comp-graph", index));
    }
    paint_dyn_frame(ui.painter(), geom.graph, geom.red, geom.blue);
    let color = live(COMP, item.comp_on);
    paint_dyn_curve(ui.painter(), geom.graph, color, 2.4, |db| sample_comp(item, db));
    let xs = knob_origins(rect, 5, 28.0, 470.0);
    let y = geom.knobs;
    let on = item.comp_on;
    item.comp_db = knob(ui, egui::pos2(xs[0], y), item.comp_db, knob_spec("Threshold", "dB", DYN_MIN, DYN_MAX, COMP_THR, 1, COMP, on, true), ("ck", index, 0));
    item.ratio = knob(ui, egui::pos2(xs[1], y), item.ratio, knob_spec("Ratio", "", 1.0, RATIO_MAX, COMP_RATIO, 1, COMP, on, true), ("ck", index, 1));
    item.comp_attack = knob(ui, egui::pos2(xs[2], y), item.comp_attack, knob_spec("Attack", "ms", ATT_MIN, ATT_MAX, COMP_ATT, 0, COMP, on, true), ("ck", index, 2));
    item.comp_release = knob(ui, egui::pos2(xs[3], y), item.comp_release, knob_spec("Release", "ms", REL_MIN, REL_MAX, COMP_REL, 0, COMP, on, true), ("ck", index, 3));
    item.makeup = knob(ui, egui::pos2(xs[4], y), item.makeup, knob_spec("Gain", "dB", GAIN_MIN, GAIN_MAX, COMP_GAIN, 1, COMP, on, true), ("ck", index, 4));
}

struct PaneGeom {
    power: egui::Rect,
    defaults: egui::Rect,
    graph: egui::Rect,
    red: egui::Rect,
    blue: egui::Rect,
    knobs: f32,
}

fn pane_geom(rect: egui::Rect) -> PaneGeom {
    let knobs = rect.bottom() - 102.0;
    let graph_top = rect.top() + 36.0;
    let graph_bottom = (knobs - 22.0).max(graph_top + 64.0);
    let graph = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 36.0, graph_top),
        egui::pos2(rect.right() - 20.0, graph_bottom),
    );
    PaneGeom {
        power: egui::Rect::from_min_size(egui::pos2(rect.left() + 4.0, rect.top() + 2.0), egui::vec2(148.0, 28.0)),
        defaults: egui::Rect::from_min_size(egui::pos2(graph.right() - 70.0, graph.top() - 34.0), egui::vec2(70.0, 30.0)),
        red: egui::Rect::from_min_max(egui::pos2(graph.right() + 4.0, graph.top()), egui::pos2(graph.right() + 12.0, graph.bottom())),
        blue: egui::Rect::from_min_max(egui::pos2(graph.left(), graph.bottom() + 4.0), egui::pos2(graph.right(), graph.bottom() + 12.0)),
        graph,
        knobs,
    }
}

fn reverb_body(ui: &mut egui::Ui, app: &mut super::DeskApp, rect: egui::Rect) {
    let before = app.fx.reverb;
    let mut rev = before;
    let row_top = rect.bottom() - 118.0;
    let plot = egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), row_top));
    drag_reverb(ui, plot, &mut rev);
    paint_reverb_graph(ui.painter(), plot, &rev);
    let row = egui::Rect::from_min_max(egui::pos2(rect.left(), row_top + 8.0), rect.right_bottom());
    layout_reverb_row(ui, row, &mut rev);
    if rev != before {
        app.fx.user_edit = true;
        app.fx.reverb = rev;
    }
}

fn layout_reverb_row(ui: &mut egui::Ui, row: egui::Rect, rev: &mut Reverb) {
    let widths = [72.0_f32, 60.0, 60.0, 60.0, 60.0, 70.0];
    let fixed: f32 = widths.iter().sum();
    let gaps = 5.0;
    let gap = ((row.width() - 16.0 - fixed) / gaps).clamp(14.0, 40.0);
    let span = fixed + gap * gaps;
    let mut x = row.left() + ((row.width() - span).max(8.0) * 0.5);
    let y = row.top();
    size_block(ui, egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(72.0, 96.0)), rev);
    x += 72.0 + gap;
    let specs = [
        ("Pre-delay", "ms", 0.0_f32, 100.0, 50.0, PREDELAY_COLOR, 0u8),
        ("Damping", "", 0.0, 100.0, 90.0, DAMP_COLOR, 1),
        ("Decay", "", 30.0, 100.0, 86.0, DECAY_COLOR, 2),
        ("Width", "", 0.0, 100.0, 50.0, WIDTH_COLOR, 3),
    ];
    let mut values = [rev.predelay, rev.damping, rev.decay, rev.width];
    for (index, spec) in specs.iter().enumerate() {
        let before = values[index];
        values[index] = knob(
            ui,
            egui::pos2(x, y),
            before,
            knob_spec(spec.0, spec.1, spec.2, spec.3, spec.4, 0, spec.5, true, false),
            ("rv", spec.6),
        );
        if (values[index] - before).abs() > f32::EPSILON {
            rev.preset = 255;
        }
        x += 60.0 + gap;
    }
    rev.predelay = values[0].clamp(0.0, 100.0);
    rev.damping = values[1].clamp(0.0, 100.0);
    rev.decay = values[2].clamp(30.0, 100.0);
    rev.width = values[3].clamp(0.0, 100.0);
    let button = egui::Rect::from_min_size(egui::pos2(x, y + 16.0), egui::vec2(70.0, 30.0));
    if defaults_button(ui, button, REVERB_COLOR, "fx-rv-dft") {
        apply_reverb_defaults(rev);
    }
}

fn size_block(ui: &mut egui::Ui, rect: egui::Rect, rev: &mut Reverb) {
    let painter = ui.painter();
    painter.text(egui::pos2(rect.center().x, rect.top() + 8.0), egui::Align2::CENTER_CENTER, "Size", face("NotoSans", 12.0), EQ_LABEL);
    let colors = [BAND_COLOR[2], BAND_COLOR[3], BAND_COLOR[0]];
    let labels = ["Small", "Medium", "Large"];
    for index in 0..3u8 {
        let button = egui::Rect::from_min_size(egui::pos2(rect.left() + 4.0, rect.top() + 20.0 + index as f32 * 24.0), egui::vec2(rect.width() - 8.0, 22.0));
        let response = ui.interact(button, ui.make_persistent_id(("fx-size", index)), egui::Sense::click());
        let selected = rev.preset == index;
        let color = colors[index as usize];
        if selected {
            ui.painter().rect_filled(button, 3.0, color);
            ui.painter().text(button.center(), egui::Align2::CENTER_CENTER, labels[index as usize], face("NotoSans", 12.0), BLACK);
        } else {
            ui.painter().rect_stroke(button, 3.0, egui::Stroke::new(1.2, color), egui::StrokeKind::Inside);
            ui.painter().text(button.center(), egui::Align2::CENTER_CENTER, labels[index as usize], face("NotoSans", 12.0), color);
        }
        if response.clicked() {
            apply_preset(rev, index);
        }
    }
}

fn arrow_button(ui: &mut egui::Ui, rect: egui::Rect, left: bool, salt: &str) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click());
    let color = if response.hovered() { WHITE } else { DIM };
    let painter = ui.painter();
    painter.circle_stroke(rect.center(), 14.0, egui::Stroke::new(1.3, color));
    let dir = if left { -1.0 } else { 1.0 };
    let tip = rect.center() + egui::vec2(dir * 5.0, 0.0);
    let stroke = egui::Stroke::new(1.6, color);
    painter.line_segment([tip, rect.center() + egui::vec2(-dir * 4.0, -5.0)], stroke);
    painter.line_segment([tip, rect.center() + egui::vec2(-dir * 4.0, 5.0)], stroke);
    response.clicked()
}

fn close_button(ui: &mut egui::Ui, rect: egui::Rect) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id("fx-close"), egui::Sense::click());
    let color = if response.hovered() { WHITE } else { DIM };
    let painter = ui.painter();
    painter.circle_stroke(rect.center(), 14.0, egui::Stroke::new(1.3, color));
    let c = rect.center();
    let stroke = egui::Stroke::new(1.6, color);
    painter.line_segment([c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)], stroke);
    painter.line_segment([c + egui::vec2(-5.0, 5.0), c + egui::vec2(5.0, -5.0)], stroke);
    response.clicked()
}

fn power_line(ui: &mut egui::Ui, rect: egui::Rect, on: bool, color: egui::Color32, salt: impl std::hash::Hash + std::fmt::Debug) -> egui::Response {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click());
    let painter = ui.painter();
    let iy = rect.center().y;
    let icon = if on { color } else { EQ_LABEL };
    paint_power(painter, egui::pos2(rect.left() + 14.0, iy), icon);
    let line = egui::Rect::from_min_max(egui::pos2(rect.left() + 30.0, iy - 2.5), egui::pos2(rect.right() - 4.0, iy + 2.5));
    painter.rect_filled(line, 2.5, color);
    response
}

fn paint_power(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.7, color);
    painter.line_segment([center + egui::vec2(0.0, -5.5), center + egui::vec2(0.0, -1.2)], stroke);
    let mut points = Vec::with_capacity(19);
    for index in 0..=18 {
        let degrees = 40.0 + index as f32 * (280.0 / 18.0);
        let radians = degrees.to_radians();
        points.push(center + egui::vec2(radians.sin(), -radians.cos()) * 4.6);
    }
    painter.line(points, stroke);
}

fn section_power(ui: &mut egui::Ui, rect: egui::Rect, label: &str, on: bool, color: egui::Color32, salt: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click());
    let icon = if on { color } else { color.gamma_multiply(0.45) };
    paint_power(ui.painter(), egui::pos2(rect.left() + 12.0, rect.center().y), icon);
    let text = if on { WHITE } else { EQ_LABEL };
    ui.painter().text(egui::pos2(rect.left() + 26.0, rect.center().y), egui::Align2::LEFT_CENTER, label, face("NotoSansBold", 14.0), text);
    response.clicked()
}

fn defaults_button(ui: &mut egui::Ui, rect: egui::Rect, hot: egui::Color32, salt: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click());
    let painter = ui.painter();
    if response.is_pointer_button_down_on() {
        painter.rect_filled(rect, 3.0, hot);
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Defaults", face("NotoSans", 13.0), BLACK);
    } else {
        painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.5, TOGGLE_IDLE), egui::StrokeKind::Inside);
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Defaults", face("NotoSans", 13.0), TOGGLE_IDLE);
    }
    response.clicked()
}

fn readout(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    value: f32,
    text: &str,
    enabled: bool,
    default: f32,
    min: f32,
    max: f32,
    log: bool,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> f32 {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click_and_drag());
    let label_color = if enabled { EQ_LABEL } else { EQ_LABEL.gamma_multiply(0.4) };
    let value_color = if enabled { EQ_VALUE } else { EQ_VALUE.gamma_multiply(0.35) };
    let painter = ui.painter();
    painter.text(rect.left_center(), egui::Align2::LEFT_CENTER, label, face("NotoSans", 13.0), label_color);
    painter.text(rect.right_center(), egui::Align2::RIGHT_CENTER, text, face("NotoSans", 13.0), value_color);
    if !enabled {
        return value;
    }
    response.clone().on_hover_cursor(egui::CursorIcon::ResizeVertical);
    if response.double_clicked() {
        return default;
    }
    if response.dragged() {
        return apply_drag(value, min, max, response.drag_delta().y, log);
    }
    value
}

struct KnobSpec {
    label: &'static str,
    unit: &'static str,
    min: f32,
    max: f32,
    default: f32,
    decimals: usize,
    color: egui::Color32,
    enabled: bool,
    tight: bool,
}

fn knob_spec(label: &'static str, unit: &'static str, min: f32, max: f32, default: f32, decimals: usize, color: egui::Color32, enabled: bool, tight: bool) -> KnobSpec {
    KnobSpec { label, unit, min, max, default, decimals, color, enabled, tight }
}

fn knob(ui: &mut egui::Ui, origin: egui::Pos2, value: f32, spec: KnobSpec, salt: impl std::hash::Hash + std::fmt::Debug) -> f32 {
    let rect = egui::Rect::from_min_size(origin, egui::vec2(60.0, 96.0));
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click_and_drag());
    let fraction = unit(value, spec.min, spec.max);
    let center = origin + egui::vec2(30.0, 26.0);
    paint_knob_arc(ui.painter(), center, 20.0, fraction, live(spec.color, spec.enabled));
    let value_y = origin.y + 64.0 - if spec.tight { 2.0 } else { 0.0 };
    let label_y = origin.y + 84.0 - if spec.tight { 4.0 } else { 0.0 };
    let value_color = if spec.enabled { WHITE } else { EQ_LABEL };
    ui.painter().text(egui::pos2(center.x, value_y), egui::Align2::CENTER_CENTER, knob_text(value, spec.decimals, spec.unit), face("NotoSans", 12.0), value_color);
    ui.painter().text(egui::pos2(center.x, label_y), egui::Align2::CENTER_CENTER, spec.label, face("NotoSansBold", 12.0), value_color);
    if !spec.enabled {
        return value.clamp(spec.min, spec.max);
    }
    response.clone().on_hover_cursor(egui::CursorIcon::ResizeVertical);
    if response.double_clicked() {
        return spec.default;
    }
    if response.dragged() {
        return apply_drag(value, spec.min, spec.max, response.drag_delta().y, false);
    }
    value.clamp(spec.min, spec.max)
}

fn paint_knob_arc(painter: &egui::Painter, center: egui::Pos2, radius: f32, fraction: f32, color: egui::Color32) {
    let start = 135.0_f32.to_radians();
    let sweep = 270.0_f32.to_radians();
    let steps = 28;
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
    painter.line(track, egui::Stroke::new(6.0, DYN_TRACK));
    if hot.len() >= 2 {
        painter.line(hot, egui::Stroke::new(6.0, color));
    }
}

fn knob_origins(pane: egui::Rect, count: usize, design_gap: f32, design_pane: f32) -> Vec<f32> {
    let n = count as f32;
    let usable = (pane.width() - 8.0).max(60.0);
    let mut gap = design_gap * (pane.width() / design_pane).clamp(0.65, 1.15);
    let max_gap = ((usable - 60.0 * n) / (n - 1.0).max(1.0)).max(0.0);
    gap = gap.min(max_gap);
    let span = 60.0 * n + gap * (n - 1.0);
    let x0 = pane.left() + (pane.width() - span) * 0.5;
    (0..count).map(|index| x0 + index as f32 * (60.0 + gap)).collect()
}

fn mode_row(ui: &mut egui::Ui, rect: egui::Rect, modes: &[Mode], current: Mode, color: egui::Color32, index: usize) -> Option<Mode> {
    let n = modes.len();
    let step = 36.0;
    let span = step * (n as f32 - 1.0);
    let x0 = rect.center().x - span * 0.5;
    let mut picked = None;
    for (place, mode) in modes.iter().enumerate() {
        let center = egui::pos2(x0 + place as f32 * step, rect.center().y);
        let hit = egui::Rect::from_center_size(center, egui::vec2(32.0, 28.0));
        let response = ui.interact(hit, ui.make_persistent_id(("fx-mode", index, place)), egui::Sense::click());
        let mark = if *mode == current { color } else { EQ_LABEL };
        paint_mode_mark(ui.painter(), center, *mode, mark);
        if response.clicked() {
            picked = Some(*mode);
        }
    }
    picked
}

fn paint_mode_mark(painter: &egui::Painter, center: egui::Pos2, mode: Mode, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.6, color);
    let p = |x: f32, y: f32| center + egui::vec2(x, y);
    let points = match mode {
        Mode::Peak => vec![p(-12.0, 2.0), p(-6.0, 2.0), p(0.0, -8.0), p(6.0, 2.0), p(12.0, 2.0)],
        Mode::LowShelf => vec![p(-11.0, -6.0), p(-3.0, -6.0), p(3.0, 6.0), p(11.0, 6.0)],
        Mode::HighShelf => vec![p(-11.0, 6.0), p(-3.0, 6.0), p(3.0, -6.0), p(11.0, -6.0)],
        Mode::HighPass => vec![p(-11.0, 7.0), p(-3.0, 7.0), p(4.0, -6.0), p(11.0, -6.0)],
    };
    painter.line(points, stroke);
}

fn apply_drag(value: f32, min: f32, max: f32, delta_y: f32, log: bool) -> f32 {
    let travel = 150.0;
    if log && min > 0.0 && max > min {
        let lo = min.ln();
        let hi = max.ln();
        let t = ((value.max(min).ln() - lo) / (hi - lo) - delta_y / travel).clamp(0.0, 1.0);
        return (lo + t * (hi - lo)).exp().clamp(min, max);
    }
    (value - delta_y * (max - min) / travel).clamp(min, max)
}

fn knob_text(value: f32, decimals: usize, unit: &str) -> String {
    let number = match decimals {
        0 => format!("{value:.0}"),
        1 => format!("{value:.1}"),
        _ => format!("{value:.2}"),
    };
    if unit.is_empty() { number } else { format!("{number} {unit}") }
}

fn drag_threshold(ui: &mut egui::Ui, graph: egui::Rect, threshold: &mut f32, salt: impl std::hash::Hash + std::fmt::Debug) {
    let response = ui.interact(graph, ui.make_persistent_id(salt), egui::Sense::click_and_drag());
    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let t = ((pos.x - graph.left()) / graph.width().max(1.0)).clamp(0.0, 1.0);
            *threshold = DYN_MIN + t * (DYN_MAX - DYN_MIN);
        }
    }
}

fn drag_plot(ui: &mut egui::Ui, bands: &mut [Band], selected: &mut usize, drag: &mut Option<u8>, rect: egui::Rect) {
    let response = ui.interact(rect, ui.make_persistent_id("fx-eq-plot"), egui::Sense::click_and_drag());
    if response.drag_started() || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let index = nearest(bands, rect, pos, SPAN_EDIT);
            *selected = index;
            if response.drag_started() {
                *drag = Some(index as u8);
            }
        }
    }
    if response.dragged() {
        if let Some(index) = drag.map(|index| index as usize) {
            if let Some(pos) = response.interact_pointer_pos() {
                if index < bands.len() {
                    bands[index].hz = hz_of_x(rect, pos.x).clamp(20.0, 20_000.0);
                    if bands[index].mode != Mode::HighPass {
                        bands[index].gain = db_of_y(rect, pos.y, SPAN_EDIT).clamp(GAIN_MIN, GAIN_MAX);
                    }
                    *selected = index;
                }
            }
        }
    }
    if response.drag_stopped() {
        *drag = None;
    }
}

fn drag_reverb(ui: &mut egui::Ui, rect: egui::Rect, rev: &mut Reverb) {
    let response = ui.interact(rect, ui.make_persistent_id("fx-rv-plot"), egui::Sense::click_and_drag());
    let dots = reverb_dots(rect, rev);
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut best = None;
            let mut best_d = 18.0_f32;
            for (index, dot) in dots.iter().enumerate() {
                let distance = pos.distance(*dot);
                if distance < best_d {
                    best_d = distance;
                    best = Some(index as u8);
                }
            }
            ui.ctx().data_mut(|data| data.insert_temp(ui.make_persistent_id("fx-rv-which"), best.unwrap_or(255)));
        }
    }
    if response.dragged() {
        let which = ui.ctx().data(|data| data.get_temp::<u8>(ui.make_persistent_id("fx-rv-which"))).unwrap_or(255);
        if let Some(pos) = response.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            match which {
                0 => rev.predelay = (t / 0.30).clamp(0.0, 1.0) * 100.0,
                1 => {
                    let start = (rev.predelay / 100.0) * 0.30;
                    let u = ((t - start) / (1.0 - start).max(0.05)).clamp(0.0, 1.0);
                    rev.decay = 30.0 + u * 70.0;
                }
                2 => rev.damping = (t * 100.0).clamp(0.0, 100.0),
                _ => return,
            }
            rev.preset = 255;
        }
    }
}

fn store_eq(app: &mut super::DeskApp, bands: &[Band]) {
    if let Some(input) = app.focus {
        if !app.focus_bus {
            let row = &mut app.fx.input_eq[input as usize];
            if row.iter().zip(bands).any(|(have, want)| have != want) {
                for (slot, band) in row.iter_mut().zip(bands) {
                    *slot = *band;
                }
                app.fx.user_edit = true;
                mirror_input_eq(app, input);
            }
            return;
        }
    }
    if (app.fx_bus as usize) < app.fx.bus_eq.len() {
        let row = &mut app.fx.bus_eq[app.fx_bus as usize];
        if row.iter().zip(bands).any(|(have, want)| have != want) {
            for (slot, band) in row.iter_mut().zip(bands) {
                *slot = *band;
            }
            app.fx.user_edit = true;
            mirror_bus_eq(app, app.fx_bus);
        }
    }
}

fn mirror_input_eq(app: &mut super::DeskApp, input: u16) {
    let even = (input & !1) as usize;
    let linked = app.hub.lock().desk.in_link.get(even).copied().unwrap_or(0) != 0;
    if !linked || even + 1 >= app.fx.input_eq.len() {
        return;
    }
    if input as usize != even {
        app.fx.input_eq[even] = app.fx.input_eq[input as usize];
    }
    app.fx.input_eq[even + 1] = app.fx.input_eq[even];
}

fn mirror_bus_eq(app: &mut super::DeskApp, bus: u16) {
    let even = (bus & !1) as usize;
    let linked = app.hub.lock().desk.bus_link.get(even).copied().unwrap_or(0) != 0;
    if !linked || even + 1 >= app.fx.bus_eq.len() || (bus as usize) >= app.fx.bus_eq.len() {
        return;
    }
    if bus as usize != even {
        app.fx.bus_eq[even] = app.fx.bus_eq[bus as usize];
    }
    app.fx.bus_eq[even + 1] = app.fx.bus_eq[even];
}

fn mirror_dyn(app: &mut super::DeskApp, input: usize) {
    let even = input & !1;
    let linked = app.hub.lock().desk.in_link.get(even).copied().unwrap_or(0) != 0;
    if !linked || even + 1 >= app.fx.dyns.len() {
        return;
    }
    if input != even {
        app.fx.dyns[even] = app.fx.dyns[input];
    }
    app.fx.dyns[even + 1] = app.fx.dyns[even];
}

/// Copy the unit onto the plate, then send whatever the plate changed.
/// A fresh connection adopts once and does not publish the previous plate.
pub(crate) fn settle(app: &mut super::DeskApp, ctx: &egui::Context) {
    let (token, generation, writable) = {
        let engine = app.hub.lock();
        (desk_token(&engine.link, &engine.desk.uid), engine.desk_gen, engine.link.writable())
    };
    let down = ctx.input(|input| input.pointer.any_down());
    // The plate does not show meters. Copying the whole gear for every meter picture is what climbed.
    if app.fx.seen == token && app.fx.adopted == generation && !app.fx.user_edit && !down && app.fx.drag.is_none() {
        return;
    }
    let (gear, in_link, bus_link, rate) = {
        let engine = app.hub.lock();
        (
            engine.desk.gear.clone(),
            engine.desk.in_link.clone(),
            engine.desk.bus_link.clone(),
            engine.desk.gear.sample_rate,
        )
    };
    if app.fx.seen != token {
        if down || app.fx.drag.is_some() {
            app.fx.hold = true;
            return;
        }
        adopt(app, &gear, &in_link, &bus_link);
        app.fx.seen = token;
        app.fx.hold = false;
        app.fx.user_edit = false;
        app.fx.adopted = generation;
        return;
    }
    if down || app.fx.drag.is_some() {
        app.fx.hold = true;
        return;
    }
    app.fx.hold = false;
    // A report that arrives after the adopt must be copied in. Only a real plate edit goes out.
    if writable && app.fx.user_edit {
        let cmds = diff_board(app, &gear, &in_link, &bus_link, rate);
        if !cmds.is_empty() {
            app.act_all(&cmds);
        }
    }
    app.fx.user_edit = false;
    // A sent edit bumps the desk. Copy that desk. A meter frame never reaches here.
    let (gear, in_link, bus_link, copied) = {
        let engine = app.hub.lock();
        if engine.desk_gen == generation {
            (gear, in_link, bus_link, generation)
        } else {
            (
                engine.desk.gear.clone(),
                engine.desk.in_link.clone(),
                engine.desk.bus_link.clone(),
                engine.desk_gen,
            )
        }
    };
    adopt(app, &gear, &in_link, &bus_link);
    app.fx.adopted = copied;
}

fn desk_token(link: &Link, uid: &str) -> String {
    match link {
        Link::Live | Link::ReadOnly { .. } => format!("unit:{uid}"),
        Link::Demo => "demo".into(),
        Link::Offline { .. } => "offline".into(),
    }
}

fn adopt(app: &mut super::DeskApp, gear: &gear::Gear, in_link: &[u8], bus_link: &[u8]) {
    let selected = app.fx.selected;
    let drag = app.fx.drag;
    for ch in 0..10usize {
        let linked = in_link.get(ch & !1).copied().unwrap_or(0) != 0;
        let slot = if linked { ch & !1 } else { ch };
        for band in 0..4usize {
            app.fx.input_eq[ch][band] = band_of(false, gear, slot * 4 + band);
        }
        app.fx.dyns[ch] = dyn_of(gear, ch);
    }
    for bus in 0..14u16 {
        let even = bus & !1;
        let linked = bus_link.get(even as usize).copied().unwrap_or(0) != 0;
        let slot = gear::bus_eq_slot(bus, linked).unwrap_or(bus).min(13) as usize;
        for band in 0..3usize {
            app.fx.bus_eq[bus as usize][band] = band_of(true, gear, slot * 3 + band);
        }
    }
    if app.fx.reverb.preset != 255 {
        app.fx.reverb.preset = gear.rev_preset.min(2);
    }
    app.fx.reverb.decay = f32::from(gear.rev_decay);
    app.fx.reverb.damping = f32::from(gear.rev_damp);
    app.fx.reverb.predelay = f32::from(gear.rev_pre);
    app.fx.reverb.width = f32::from(gear.rev_width);
    app.fx.selected = selected;
    app.fx.drag = drag;
}

fn band_of(bus: bool, gear: &gear::Gear, index: usize) -> Band {
    let (mode, bypass, hz, gain, q) = if bus {
        (gear.bus_mode[index], gear.bus_bypass[index], gear.bus_hz[index], gear.bus_gain[index], gear.bus_q[index])
    } else {
        (gear.in_mode[index], gear.in_bypass[index], gear.in_hz[index], gear.in_gain[index], gear.in_q[index])
    };
    Band { on: bypass == 0, mode: mode_from(mode), hz: hz as f32, gain, q }
}

fn dyn_of(gear: &gear::Gear, index: usize) -> Dyn {
    Dyn {
        gate_on: index < 2 && gear.gate_byp[index] == 0,
        gate_db: if index < 2 { gear.gate_thr[index] } else { GATE_THR },
        gate_attack: if index < 2 { f32::from(gear.gate_att[index]) } else { GATE_ATT },
        gate_release: if index < 2 { f32::from(gear.gate_rel[index]) } else { GATE_REL },
        comp_on: gear.comp_byp[index] == 0,
        comp_db: gear.comp_thr[index],
        ratio: gear.comp_ratio[index],
        comp_attack: f32::from(gear.comp_att[index]),
        comp_release: f32::from(gear.comp_rel[index]),
        makeup: gear::db_of_linear(gear.comp_makeup[index]),
    }
}

fn diff_board(app: &super::DeskApp, gear: &gear::Gear, in_link: &[u8], bus_link: &[u8], rate: i32) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    for ch in 0..10u16 {
        let linked = in_link.get((ch & !1) as usize).copied().unwrap_or(0) != 0;
        if linked && ch % 2 == 1 {
            continue;
        }
        let slot = if linked { ch & !1 } else { ch };
        for band in 0..4u8 {
            let index = slot as usize * 4 + band as usize;
            push_band(&mut cmds, false, slot, band, app.fx.input_eq[ch as usize][band as usize], gear, index);
        }
        push_dyn(&mut cmds, slot, app.fx.dyns[ch as usize], gear);
    }
    for bus in 0..14u16 {
        let even = bus & !1;
        let linked = bus_link.get(even as usize).copied().unwrap_or(0) != 0;
        if linked && bus % 2 == 1 {
            continue;
        }
        let slot = gear::bus_eq_slot(bus, linked).unwrap_or(bus);
        if slot >= 14 {
            continue;
        }
        for band in 0..3u8 {
            let index = slot as usize * 3 + band as usize;
            push_band(&mut cmds, true, slot, band, app.fx.bus_eq[bus as usize][band as usize], gear, index);
        }
    }
    if rate <= 96_000 {
        push_reverb(&mut cmds, app.fx.reverb, gear);
    }
    cmds
}

fn push_band(cmds: &mut Vec<Cmd>, bus: bool, channel: u16, band: u8, have: Band, gear: &gear::Gear, index: usize) {
    let (mode, bypass, hz, gain, q) = if bus {
        (gear.bus_mode[index], gear.bus_bypass[index], gear.bus_hz[index], gear.bus_gain[index], gear.bus_q[index])
    } else {
        (gear.in_mode[index], gear.in_bypass[index], gear.in_hz[index], gear.in_gain[index], gear.in_q[index])
    };
    let on = if have.on != (bypass == 0) { Some(have.on) } else { None };
    let want = mode_code(have.mode);
    let mode = if want != mode { Some(want) } else { None };
    let hz_now = have.hz.round().clamp(20.0, 20_000.0) as i32;
    let hz = if (have.hz - hz as f32).abs() > 0.5 { Some(hz_now) } else { None };
    let gain = if (have.gain - gain).abs() > 0.05 { Some(have.gain.clamp(GAIN_MIN, GAIN_MAX)) } else { None };
    let q = if (have.q - q).abs() > 0.02 { Some(have.q.clamp(Q_MIN, Q_MAX)) } else { None };
    if on.is_some() || mode.is_some() || hz.is_some() || gain.is_some() || q.is_some() {
        cmds.push(Cmd::Eq { bus, channel, band, on, mode, hz, gain, q });
    }
}

fn push_dyn(cmds: &mut Vec<Cmd>, index: u16, item: Dyn, gear: &gear::Gear) {
    let at = index as usize;
    if index < 2 {
        let on = if item.gate_on != (gear.gate_byp[at] == 0) { Some(item.gate_on) } else { None };
        let db = if (item.gate_db - gear.gate_thr[at]).abs() > 0.05 { Some(item.gate_db.clamp(DYN_MIN, DYN_MAX)) } else { None };
        let attack = rounded_ms(item.gate_attack, gear.gate_att[at]);
        let release = rounded_ms(item.gate_release, gear.gate_rel[at]);
        if on.is_some() || db.is_some() || attack.is_some() || release.is_some() {
            cmds.push(Cmd::Gate { index, on, db, attack, release });
        }
    }
    let on = if item.comp_on != (gear.comp_byp[at] == 0) { Some(item.comp_on) } else { None };
    let db = if (item.comp_db - gear.comp_thr[at]).abs() > 0.05 { Some(item.comp_db.clamp(DYN_MIN, DYN_MAX)) } else { None };
    let ratio = if (item.ratio - gear.comp_ratio[at]).abs() > 0.05 { Some(item.ratio.clamp(1.0, RATIO_MAX)) } else { None };
    let attack = rounded_ms(item.comp_attack, gear.comp_att[at]);
    let release = rounded_ms(item.comp_release, gear.comp_rel[at]);
    let makeup_db = gear::db_of_linear(gear.comp_makeup[at]);
    let makeup = if (item.makeup - makeup_db).abs() > 0.05 { Some(item.makeup.clamp(GAIN_MIN, GAIN_MAX)) } else { None };
    if on.is_some() || db.is_some() || ratio.is_some() || attack.is_some() || release.is_some() || makeup.is_some() {
        cmds.push(Cmd::Comp { index, on, db, ratio, attack, release, makeup });
    }
}

fn rounded_ms(have: f32, stored: i16) -> Option<i16> {
    let next = have.round().clamp(1.0, 2000.0) as i16;
    if next != stored { Some(next) } else { None }
}

fn push_reverb(cmds: &mut Vec<Cmd>, rev: Reverb, gear: &gear::Gear) {
    let decay = byte_changed(rev.decay, gear.rev_decay);
    let damp = byte_changed(rev.damping, gear.rev_damp);
    let pre = byte_changed(rev.predelay, gear.rev_pre);
    let width = byte_changed(rev.width, gear.rev_width);
    let preset = if rev.preset <= 2 && rev.preset != gear.rev_preset { Some(rev.preset) } else { None };
    if decay.is_some() || damp.is_some() || pre.is_some() || width.is_some() || preset.is_some() {
        cmds.push(Cmd::Reverb { decay, damp, pre, width, preset });
    }
}

fn byte_changed(have: f32, stored: u8) -> Option<u8> {
    let next = have.round().clamp(0.0, 100.0) as u8;
    if next != stored { Some(next) } else { None }
}

fn mode_code(mode: Mode) -> u8 {
    match mode {
        Mode::Peak => 0,
        Mode::LowShelf => 1,
        Mode::HighShelf => 2,
        Mode::HighPass => 3,
    }
}

fn mode_from(byte: u8) -> Mode {
    match byte {
        1 => Mode::LowShelf,
        2 => Mode::HighShelf,
        3 => Mode::HighPass,
        _ => Mode::Peak,
    }
}

pub(crate) fn paint_eq_thumb(painter: &egui::Painter, rect: egui::Rect, bands: &[Band]) {
    painter.rect_filled(rect, 0.0, BLACK);
    let plot = rect.shrink2(egui::vec2(4.0, 6.0));
    paint_sum(painter, plot, bands, egui::Color32::from_white_alpha(170), 1.4, SPAN_THUMB);
    for (index, band) in bands.iter().enumerate() {
        if !band.on {
            continue;
        }
        painter.circle_filled(dot_at(plot, band, SPAN_THUMB), 2.4, BAND_COLOR[index % 4]);
    }
}

pub(crate) fn paint_dyn_thumb(painter: &egui::Painter, rect: egui::Rect, item: &Dyn, gate: bool) {
    painter.rect_filled(rect, 0.0, BLACK);
    let plot = rect.shrink2(egui::vec2(4.0, 6.0));
    if gate {
        paint_dyn_curve(painter, plot, live(GATE, item.gate_on), 1.3, |db| sample_gate(item, db));
    }
    paint_dyn_curve(painter, plot, live(COMP, item.comp_on), 1.3, |db| sample_comp(item, db));
}

pub(crate) fn paint_reverb_thumb(painter: &egui::Painter, rect: egui::Rect, rev: &Reverb) {
    painter.rect_filled(rect, 0.0, BLACK);
    let plot = rect.shrink2(egui::vec2(6.0, 8.0));
    paint_tail(painter, plot, rev, REVERB_COLOR, 1.5);
}

fn paint_eq_plot(painter: &egui::Painter, rect: egui::Rect, bands: &[Band], selected: usize, span: f32) {
    painter.rect_filled(rect, 0.0, BLACK);
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, GRID), egui::StrokeKind::Inside);
    for db in [-20.0, -10.0, 0.0, 10.0, 20.0] {
        let y = y_of_db(rect, db, span);
        painter.hline(rect.x_range(), y, egui::Stroke::new(1.0, GRID));
    }
    for hz in [100.0, 1000.0, 10_000.0] {
        painter.vline(x_of_hz(rect, hz), rect.y_range(), egui::Stroke::new(1.0, GRID));
    }
    for (index, band) in bands.iter().enumerate() {
        if band.on {
            paint_one(painter, rect, band, color_a(BAND_COLOR[index % 4], 90), 1.5, span);
        }
    }
    paint_sum(painter, rect, bands, egui::Color32::from_white_alpha(230), 1.8, span);
    for (index, band) in bands.iter().enumerate() {
        let color = if band.on { BAND_COLOR[index % 4] } else { EQ_LABEL };
        let radius = if index == selected { 5.0 } else { 3.5 };
        let at = dot_at(rect, band, span);
        painter.circle_filled(at, radius, color);
        if index == selected {
            painter.circle_stroke(at, radius + 2.0, egui::Stroke::new(1.2, WHITE));
        }
    }
}

fn paint_eq_labels(painter: &egui::Painter, rect: egui::Rect) {
    for db in [-20.0, 0.0, 20.0] {
        let y = y_of_db(rect, db, SPAN_EDIT);
        painter.text(egui::pos2(rect.left() - 4.0, y), egui::Align2::RIGHT_CENTER, &format!("{db:+.0}"), face("NotoSans", 10.0), EQ_LABEL);
    }
    for (hz, label) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        painter.text(egui::pos2(x_of_hz(rect, hz), rect.bottom() + 10.0), egui::Align2::CENTER_CENTER, label, face("NotoSans", 10.0), EQ_LABEL);
    }
}

fn paint_dyn_frame(painter: &egui::Painter, graph: egui::Rect, red: egui::Rect, blue: egui::Rect) {
    painter.rect_filled(graph, 0.0, BLACK);
    painter.rect_stroke(graph, 0.0, egui::Stroke::new(1.0, GRID), egui::StrokeKind::Inside);
    for step in 0..=6 {
        let db = DYN_MIN + step as f32 * 12.0;
        let x = graph.left() + unit(db, DYN_MIN, DYN_MAX) * graph.width();
        let y = graph.bottom() - unit(db, DYN_MIN, DYN_MAX) * graph.height();
        painter.vline(x, graph.y_range(), egui::Stroke::new(1.0, GRID));
        painter.hline(graph.x_range(), y, egui::Stroke::new(1.0, GRID));
        if step % 2 == 0 {
            painter.text(egui::pos2(graph.left() - 4.0, y), egui::Align2::RIGHT_CENTER, &format!("{db:.0}"), face("NotoSans", 10.0), EQ_LABEL);
        }
    }
    painter.line(vec![dyn_xy(graph, DYN_MIN, DYN_MIN), dyn_xy(graph, DYN_MAX, DYN_MAX)], egui::Stroke::new(1.0, EQ_VALUE));
    empty_well(painter, red, GR_METER);
    empty_well(painter, blue, L_METER);
}

fn empty_well(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(8, 8, 8));
    painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
}

fn paint_dyn_curve(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32, width: f32, mut sample: impl FnMut(f32) -> f32) {
    let clipped = painter.with_clip_rect(rect);
    let mut points = Vec::with_capacity(64);
    for index in 0..64 {
        let t = index as f32 / 63.0;
        let input = DYN_MIN + t * (DYN_MAX - DYN_MIN);
        points.push(dyn_xy(rect, input, sample(input)));
    }
    clipped.line(points, egui::Stroke::new(width, color));
}

fn paint_reverb_graph(painter: &egui::Painter, rect: egui::Rect, rev: &Reverb) {
    painter.rect_filled(rect, 0.0, BLACK);
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, GRID), egui::StrokeKind::Inside);
    for step in 1..4 {
        let x = rect.left() + rect.width() * step as f32 / 4.0;
        painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, GRID));
    }
    paint_tail(painter, rect, rev, preset_color(rev.preset), 2.0);
    let dots = reverb_dots(rect, rev);
    let colors = [PREDELAY_COLOR, DECAY_COLOR, DAMP_COLOR];
    for (dot, color) in dots.iter().zip(colors) {
        painter.circle_filled(*dot, 6.0, color);
        painter.circle_stroke(*dot, 8.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)));
    }
}

fn paint_sum(painter: &egui::Painter, rect: egui::Rect, bands: &[Band], color: egui::Color32, width: f32, span: f32) {
    let mut points = Vec::with_capacity(48);
    for index in 0..48 {
        let t = index as f32 / 47.0;
        let db: f32 = bands.iter().map(|band| band_db(band, hz_of_t(t))).sum();
        points.push(egui::pos2(rect.left() + t * rect.width(), y_of_db(rect, db, span)));
    }
    painter.line(points, egui::Stroke::new(width, color));
}

fn paint_one(painter: &egui::Painter, rect: egui::Rect, band: &Band, color: egui::Color32, width: f32, span: f32) {
    let mut points = Vec::with_capacity(48);
    for index in 0..48 {
        let t = index as f32 / 47.0;
        points.push(egui::pos2(rect.left() + t * rect.width(), y_of_db(rect, band_db(band, hz_of_t(t)), span)));
    }
    painter.line(points, egui::Stroke::new(width, color));
}

fn paint_tail(painter: &egui::Painter, rect: egui::Rect, rev: &Reverb, color: egui::Color32, width: f32) {
    let clipped = painter.with_clip_rect(rect);
    let mut points = Vec::with_capacity(64);
    for index in 0..64 {
        let t = index as f32 / 63.0;
        let amp = tail_amp(t, rev);
        let x = rect.left() + rect.width() * t;
        let y = rect.bottom() - 6.0 - amp * (rect.height() - 14.0);
        points.push(egui::pos2(x.clamp(rect.left(), rect.right()), y.clamp(rect.top(), rect.bottom())));
    }
    clipped.line(points, egui::Stroke::new(width, color));
}

fn reverb_dots(rect: egui::Rect, rev: &Reverb) -> [egui::Pos2; 3] {
    let y = rect.bottom() - 8.0;
    let start = (rev.predelay.clamp(0.0, 100.0) / 100.0) * 0.30;
    let len = ((rev.decay.clamp(30.0, 100.0) - 30.0) / 70.0).clamp(0.0, 1.0);
    let predelay = rect.left() + start * rect.width();
    let decay = rect.left() + (start + (1.0 - start) * len) * rect.width();
    let damping = rect.left() + (rev.damping.clamp(0.0, 100.0) / 100.0) * rect.width();
    [egui::pos2(predelay, y), egui::pos2(decay.clamp(rect.left(), rect.right()), y), egui::pos2(damping, y)]
}

fn tail_amp(t: f32, rev: &Reverb) -> f32 {
    let start = (rev.predelay.clamp(0.0, 100.0) / 100.0) * 0.30;
    if t <= start {
        return 0.0;
    }
    let u = ((t - start) / (1.0 - start).max(0.08)).clamp(0.0, 1.0);
    let len = ((rev.decay.clamp(30.0, 100.0) - 30.0) / 70.0).clamp(0.0, 1.0);
    let k = 7.5 - len * 6.2;
    let env = (-k * u).exp();
    let damp = (rev.damping.clamp(0.0, 100.0) / 100.0) * u;
    (env * (1.0 - 0.8 * damp)).clamp(0.0, 1.0)
}

fn preset_color(preset: u8) -> egui::Color32 {
    match preset {
        0 => BAND_COLOR[2],
        1 => BAND_COLOR[3],
        2 => BAND_COLOR[0],
        _ => DECAY_COLOR,
    }
}

fn sample_gate(item: &Dyn, db: f32) -> f32 {
    gate_out_db(db, item.gate_db, DYN_MIN, DYN_MAX, unit(item.gate_attack, ATT_MIN, ATT_MAX), unit(item.gate_release, REL_MIN, REL_MAX))
}

fn sample_comp(item: &Dyn, db: f32) -> f32 {
    comp_out_db(db, item.comp_db, item.ratio.max(1.0), item.makeup, unit(item.comp_attack, ATT_MIN, ATT_MAX), unit(item.comp_release, REL_MIN, REL_MAX))
}

/// Gate shape. Attack widens the open side. Release widens the close side.
fn gate_out_db(db: f32, thr: f32, min: f32, max: f32, att_n: f32, rel_n: f32) -> f32 {
    let open_w = 0.6 + att_n * 18.0;
    let close_w = 0.6 + rel_n * 26.0;
    let x_l = min.max(thr - close_w);
    let x_r = max.min(thr + open_w);
    let mut y_thr = thr - open_w * 0.65;
    y_thr = y_thr.clamp(min, thr);
    if db <= x_l {
        return min;
    }
    if db >= x_r {
        return db;
    }
    if db <= thr {
        let span = thr - x_l;
        let t = if span > 0.001 { (db - x_l) / span } else { 1.0 };
        return min + (y_thr - min) * smooth(t);
    }
    let span = x_r - thr;
    let t = if span > 0.001 { (db - thr) / span } else { 1.0 };
    y_thr + (x_r - y_thr) * smooth(t)
}

/// Compressor shape. Attack softens the knee, release sags under it, gain lifts the line.
fn comp_out_db(db: f32, thr: f32, ratio: f32, makeup: f32, att_n: f32, rel_n: f32) -> f32 {
    let ratio = ratio.max(1.0);
    let engaged = 1.0 - att_n * 0.85;
    let use_ratio = 1.0 + (ratio - 1.0) * engaged;
    let knee = 0.8 + att_n * 14.0;
    let half = knee * 0.5;
    let rel_w = 1.0 + rel_n * 24.0;
    let rel_edge = thr - half;
    let y = if db < rel_edge {
        let dist = rel_edge - db;
        if dist < rel_w {
            let bump = ((dist / rel_w) * std::f32::consts::PI).sin();
            db - bump * rel_n * (4.0 + (1.0 - 1.0 / ratio) * 10.0)
        } else {
            db
        }
    } else if db >= thr + half {
        thr + (db - thr) / use_ratio
    } else {
        let t = (db - rel_edge) / knee.max(0.001);
        let hard = thr + (db - thr) / use_ratio;
        db + (hard - db) * smooth(t)
    };
    y + makeup
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn dyn_xy(rect: egui::Rect, input: f32, output: f32) -> egui::Pos2 {
    egui::pos2(rect.left() + unit(input, DYN_MIN, DYN_MAX) * rect.width(), rect.bottom() - unit(output, DYN_MIN, DYN_MAX) * rect.height())
}

fn apply_gate_defaults(item: &mut Dyn) {
    item.gate_db = GATE_THR;
    item.gate_attack = GATE_ATT;
    item.gate_release = GATE_REL;
}

fn apply_comp_defaults(item: &mut Dyn) {
    item.comp_db = COMP_THR;
    item.ratio = COMP_RATIO;
    item.comp_attack = COMP_ATT;
    item.comp_release = COMP_REL;
    item.makeup = COMP_GAIN;
}

fn apply_reverb_defaults(rev: &mut Reverb) {
    rev.decay = 86.0;
    rev.damping = 90.0;
    rev.predelay = 50.0;
    rev.width = 50.0;
    rev.preset = 1;
}

fn dot_at(rect: egui::Rect, band: &Band, span: f32) -> egui::Pos2 {
    let db = if band.mode == Mode::HighPass { 0.0 } else { band.gain };
    egui::pos2(x_of_hz(rect, band.hz), y_of_db(rect, db, span))
}

fn nearest(bands: &[Band], rect: egui::Rect, pos: egui::Pos2, span: f32) -> usize {
    let mut best = 0usize;
    let mut best_d = f32::MAX;
    for (index, band) in bands.iter().enumerate() {
        let distance = pos.distance(dot_at(rect, band, span));
        if distance < best_d {
            best_d = distance;
            best = index;
        }
    }
    best
}

fn band_db(band: &Band, hz: f32) -> f32 {
    if !band.on {
        return 0.0;
    }
    let corner = band.hz.max(1.0);
    let x = (hz.max(1.0) / corner).ln();
    match band.mode {
        Mode::Peak => {
            let width = (1.15 / band.q.clamp(0.1, 16.0)).max(0.08);
            let weight = (-0.5 * (x / width).powi(2)).exp();
            band.gain * weight
        }
        Mode::LowShelf => {
            let k = (1.4 * band.q.clamp(0.2, 8.0)).max(0.4);
            let weight = 1.0 / (1.0 + (k * x).exp());
            band.gain * weight
        }
        Mode::HighShelf => {
            let k = (1.4 * band.q.clamp(0.2, 8.0)).max(0.4);
            let weight = 1.0 / (1.0 + (-k * x).exp());
            band.gain * weight
        }
        Mode::HighPass => {
            if hz >= corner * 4.0 {
                return 0.0;
            }
            let octaves = (corner / hz.max(1.0)).log2().max(0.0);
            (-18.0 * octaves).max(-60.0)
        }
    }
}

fn mode_choices(bus: bool, index: usize, last: usize) -> &'static [Mode] {
    if index == 0 {
        if bus { &[Mode::HighPass, Mode::LowShelf, Mode::Peak] } else { &[Mode::Peak, Mode::LowShelf, Mode::HighPass] }
    } else if index == last {
        &[Mode::Peak, Mode::HighShelf]
    } else {
        &[]
    }
}

fn band_default_hz(bus: bool, index: usize) -> f32 {
    if bus {
        match index {
            0 => 40.0,
            1 => 1000.0,
            _ => 8000.0,
        }
    } else {
        match index {
            0 => 80.0,
            1 => 400.0,
            2 => 2000.0,
            _ => 8000.0,
        }
    }
}

fn apply_preset(rev: &mut Reverb, which: u8) {
    rev.preset = which;
    match which {
        0 => {
            rev.decay = 30.0;
            rev.damping = 70.0;
            rev.predelay = 10.0;
            rev.width = 30.0;
        }
        2 => {
            rev.decay = 85.0;
            rev.damping = 30.0;
            rev.predelay = 45.0;
            rev.width = 80.0;
        }
        _ => {
            rev.decay = 55.0;
            rev.damping = 50.0;
            rev.predelay = 25.0;
            rev.width = 50.0;
        }
    }
}

fn hz_text(hz: f32) -> String {
    if hz >= 1000.0 {
        let kilo = hz / 1000.0;
        if (kilo - kilo.round()).abs() < 0.05 {
            format!("{kilo:.0} kHz")
        } else {
            format!("{kilo:.2} kHz")
        }
    } else {
        format!("{hz:.0} Hz")
    }
}

fn color_a(color: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn live(color: egui::Color32, on: bool) -> egui::Color32 {
    if on { color } else { color.gamma_multiply(0.2) }
}

fn unit(value: f32, min: f32, max: f32) -> f32 {
    if max <= min { 0.0 } else { ((value - min) / (max - min)).clamp(0.0, 1.0) }
}

fn hz_of_t(t: f32) -> f32 {
    20.0 * 1000.0_f32.powf(t.clamp(0.0, 1.0))
}

fn t_of_hz(hz: f32) -> f32 {
    ((hz.max(20.0) / 20.0).ln() / 1000.0_f32.ln()).clamp(0.0, 1.0)
}

fn x_of_hz(rect: egui::Rect, hz: f32) -> f32 {
    rect.left() + t_of_hz(hz) * rect.width()
}

fn hz_of_x(rect: egui::Rect, x: f32) -> f32 {
    let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    hz_of_t(t)
}

fn y_of_db(rect: egui::Rect, db: f32, span: f32) -> f32 {
    let t = ((db.clamp(-span, span) + span) / (span * 2.0)).clamp(0.0, 1.0);
    rect.bottom() - t * rect.height()
}

fn db_of_y(rect: egui::Rect, y: f32, span: f32) -> f32 {
    let t = ((rect.bottom() - y) / rect.height().max(1.0)).clamp(0.0, 1.0);
    t * span * 2.0 - span
}

fn face(name: &str, size: f32) -> egui::FontId {
    egui::FontId { size, family: egui::FontFamily::Name(name.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bypassed_band_is_silent_and_a_peak_centers_on_its_gain() {
        let silent = Band { on: false, mode: Mode::Peak, hz: 1000.0, gain: 9.0, q: 1.0 };
        assert_eq!(band_db(&silent, 1000.0), 0.0);
        let peak = Band { on: true, mode: Mode::Peak, hz: 400.0, gain: -6.0, q: 1.0 };
        let at = band_db(&peak, 400.0);
        assert!((at + 6.0).abs() < 1.0e-3, "{at}");
        let summed = band_db(&silent, 400.0) + band_db(&peak, 400.0);
        assert!((summed + 6.0).abs() < 1.0e-3, "{summed}");
        let pass = Band { on: true, mode: Mode::HighPass, hz: 80.0, gain: 20.0, q: 4.0 };
        let above = band_db(&pass, 2000.0);
        assert!(above.abs() < 0.05, "{above}");
    }

    #[test]
    fn demo_starts_bypassed_so_enabling_does_not_jump() {
        let board = Board::demo();
        assert_eq!(board.input_eq.len(), 10);
        assert_eq!(board.bus_eq.len(), 14);
        assert_eq!(board.dyns.len(), 10);
        for row in &board.input_eq {
            assert!(row.iter().all(|band| !band.on && band.gain == 0.0 && (band.q - 1.0).abs() < 1.0e-6));
        }
        for row in &board.bus_eq {
            assert!(row.iter().all(|band| !band.on && band.gain == 0.0));
            assert_eq!(row[0].mode, Mode::HighPass);
        }
        assert!(board.dyns.iter().all(|item| !item.gate_on && !item.comp_on && item.makeup == 0.0));
        assert!((board.reverb.decay - 55.0).abs() < 1.0e-6);
    }

    #[test]
    fn mode_choices_follow_the_band() {
        assert_eq!(mode_choices(false, 0, 3), &[Mode::Peak, Mode::LowShelf, Mode::HighPass]);
        assert!(mode_choices(false, 1, 3).is_empty());
        assert_eq!(mode_choices(false, 3, 3), &[Mode::Peak, Mode::HighShelf]);
        assert_eq!(mode_choices(true, 0, 2), &[Mode::HighPass, Mode::LowShelf, Mode::Peak]);
        assert!(mode_choices(true, 1, 2).is_empty());
        assert_eq!(mode_choices(true, 2, 2), &[Mode::Peak, Mode::HighShelf]);
    }

    #[test]
    fn quiet_uses_factory_numbers_and_defaults_restore_them() {
        let item = Dyn::quiet();
        assert!((item.gate_db + 46.0).abs() < 1.0e-6);
        assert!((item.gate_attack - 10.0).abs() < 1.0e-6);
        assert!((item.gate_release - 1000.0).abs() < 1.0e-6);
        assert!((item.comp_db + 10.0).abs() < 1.0e-6);
        assert!((item.ratio - 2.0).abs() < 1.0e-6);
        assert!((item.comp_release - 1000.0).abs() < 1.0e-6);
        let mut moved = item;
        moved.gate_on = true;
        moved.comp_on = true;
        moved.gate_db = -12.0;
        moved.gate_attack = 80.0;
        moved.gate_release = 200.0;
        moved.comp_db = -30.0;
        moved.ratio = 8.0;
        moved.comp_attack = 40.0;
        moved.comp_release = 300.0;
        moved.makeup = 6.0;
        apply_gate_defaults(&mut moved);
        assert!(moved.gate_on);
        assert!((moved.gate_db + 46.0).abs() < 1.0e-6);
        assert!((moved.gate_attack - 10.0).abs() < 1.0e-6);
        assert!((moved.gate_release - 1000.0).abs() < 1.0e-6);
        assert!((moved.comp_db + 30.0).abs() < 1.0e-6);
        apply_comp_defaults(&mut moved);
        assert!(moved.comp_on);
        assert!((moved.comp_db + 10.0).abs() < 1.0e-6);
        assert!((moved.ratio - 2.0).abs() < 1.0e-6);
        assert!(moved.makeup.abs() < 1.0e-6);
        assert!((moved.comp_attack - 10.0).abs() < 1.0e-6);
        assert!((moved.comp_release - 1000.0).abs() < 1.0e-6);
        let mut rev = Reverb::medium();
        apply_reverb_defaults(&mut rev);
        assert_eq!(rev.preset, 1);
        assert!((rev.decay - 86.0).abs() < 1.0e-6);
        assert!((rev.damping - 90.0).abs() < 1.0e-6);
        assert!((rev.predelay - 50.0).abs() < 1.0e-6);
        assert!((rev.width - 50.0).abs() < 1.0e-6);
    }

    #[test]
    fn attack_release_and_gain_move_the_curves_inside_the_map() {
        let fast = gate_out_db(-46.0, -46.0, DYN_MIN, DYN_MAX, 0.0, 0.0);
        let slow = gate_out_db(-46.0, -46.0, DYN_MIN, DYN_MAX, 1.0, 0.0);
        assert!(fast - slow > 5.0, "{fast} {slow}");
        for step in 0..=72 {
            let db = DYN_MIN + step as f32;
            let y = gate_out_db(db, -46.0, DYN_MIN, DYN_MAX, 0.4, 0.5);
            assert!((DYN_MIN..=DYN_MAX).contains(&y), "{y}");
        }
        let loose = comp_out_db(0.0, -10.0, 4.0, 0.0, 1.0, 0.0);
        let tight = comp_out_db(0.0, -10.0, 4.0, 0.0, 0.0, 0.0);
        assert!(loose - tight > 2.0, "{loose} {tight}");
        let held = comp_out_db(-20.0, -10.0, 4.0, 0.0, 0.0, 0.0);
        let sag = comp_out_db(-20.0, -10.0, 4.0, 0.0, 0.0, 1.0);
        assert!(held - sag > 2.0, "{held} {sag}");
        let lifted = comp_out_db(-20.0, -10.0, 4.0, 3.0, 0.0, 1.0);
        assert!((lifted - sag - 3.0).abs() < 1.0e-3, "{lifted} {sag}");
        let over = comp_out_db(0.0, -10.0, 2.0, 20.0, 0.0, 0.0);
        assert!(over > 0.0, "{over}");
        assert_eq!(unit(over, DYN_MIN, DYN_MAX), 1.0);
        assert_eq!(unit(-80.0, DYN_MIN, DYN_MAX), 0.0);
        assert_eq!(knob_text(1.4, 1, ""), "1.4");
    }
}

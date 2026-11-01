//! One mix, drawn as channel strips and one bus strip.
//! A linked pair such as Main 1-2 is one stereo strip, not a left strip and a right strip.
//! The fader uses the +12 dB warp, so unity gain sits on the 0 tick.
//! Drag the cap. The track does not move it. Double-click is 0 dB.
//! A locked fader does not follow the pointer. The value is sent on release.
//! Pan is shown only on a stereo mix, and only for a channel that is not linked.
//! EQ, gate, compressor, and reverb are sent when the plate is released.

use eframe::egui;

use super::{
    bus_color, pair_label, pair_linked, paint_padlock, shown_inputs, single_name, Bar, DeskApp,
    Hold, Snap, WHITE,
};
use crate::engine::LockChange;
use crate::plan::Cmd;
use crate::topology::{self, INPUTS};

const MIX_BG: egui::Color32 = egui::Color32::from_rgb(35, 38, 44);
const BUS_BG: egui::Color32 = egui::Color32::from_rgb(47, 52, 59);
const TICK: egui::Color32 = egui::Color32::from_rgb(108, 109, 110);
const TOGGLE: egui::Color32 = egui::Color32::from_rgb(158, 159, 160);
const MUTE_RED: egui::Color32 = egui::Color32::from_rgb(207, 31, 37);
const SOLO_YELLOW: egui::Color32 = egui::Color32::from_rgb(234, 226, 23);
const TOGGLE_BLUE: egui::Color32 = egui::Color32::from_rgb(52, 141, 221);
const BLACK: egui::Color32 = egui::Color32::BLACK;

const CH_W: f32 = 105.0;
/// Mixer scrollbar track. The pill is the 35px stroke inside it.
const SCROLL_H: f32 = 40.0;
const THUMB_MIN: f32 = 46.5;
const THUMB_INK: egui::Color32 = egui::Color32::from_rgb(89, 92, 94);
const BUS_W: f32 = 130.0;
const BUS_NARROW: f32 = 50.0;
const THUMB_H: f32 = 49.0;
const NAME_H: f32 = 35.0;
const TOGGLE_H: f32 = 30.0;
/// Electron shows the EQ plate once a strip is taller than this.
const EQ_STRIP_H: f32 = 394.0;
/// The gate plate joins once the strip clears the second slot.
const DYN_STRIP_H: f32 = 448.0;

pub(crate) fn show(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, MIX_BG);
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    // The reference mixer has no title row. The mix name lives in the sidebar.
    let (body, _) = ui.allocate_exact_size(egui::vec2(full.width(), full.height()), egui::Sense::hover());
    strips(app, ui, snap, body);

    if app.focus.is_some() || app.focus_bus {
        // The thumbs stay above this plate, so the same thumb can close it.
        let skip = 10.0 + (THUMB_H + 5.0) * 2.0;
        let panel = egui::Rect::from_min_max(body.min + egui::vec2(0.0, skip), body.max);
        let col_w = editor_col_w(app);
        if panel.height() >= 200.0 && panel.width() >= col_w + 300.0 {
            ui.painter().rect_filled(panel, 0.0, super::fx::PLATE_BG);
            paint_plate_shadow(ui.painter(), panel);
            let column = egui::Rect::from_min_size(panel.left_top(), egui::vec2(col_w, panel.height()));
            let content = egui::Rect::from_min_max(egui::pos2(column.right(), panel.top()), panel.right_bottom());
            editor_column(app, ui, snap, column);
            super::fx::editor(app, ui.ctx(), content);
        }
    }
    super::fx::settle(app, ui.ctx());
}

fn editor_col_w(app: &DeskApp) -> f32 {
    if app.focus.is_some() && !app.focus_bus {
        CH_W
    } else if app.bus_strip_open {
        BUS_W
    } else {
        BUS_NARROW
    }
}

fn editor_column(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, column: egui::Rect) {
    let _cover = ui.interact(column, ui.make_persistent_id("fx-column"), egui::Sense::click_and_drag());
    let mut col = ui.new_child(
        egui::UiBuilder::new().max_rect(column).layout(egui::Layout::left_to_right(egui::Align::Min)),
    );
    col.spacing_mut().item_spacing.x = 0.0;
    col.set_clip_rect(column.intersect(ui.clip_rect()));
    if let Some(input) = app.focus {
        if !app.focus_bus && (input as usize) < app.fx.input_eq.len() {
            channel_strip(app, &mut col, snap, app.bus, input, column.height(), true, true);
            return;
        }
    }
    if app.focus_bus {
        let bus = app.fx_bus;
        let linked = pair_linked(&snap.desk, bus & !1);
        let shown = if linked { bus & !1 } else { bus };
        let plan = ChannelPlan { eq: true, dynamics: false, usb: false, pan: false };
        let rows = column_y(column.top(), column.height(), &plan);
        bus_strip(
            app,
            &mut col,
            snap,
            shown,
            column.height(),
            true,
            true,
            rows.fader_top,
            rows.fader_bot,
            rows.name,
            linked,
            true,
        );
    }
}

/// Soft black fade above the plate. Same ink as the sidebar shadow, cast upward.
fn paint_plate_shadow(painter: &egui::Painter, panel: egui::Rect) {
    for index in 0..12 {
        let distance = index as f32 + 0.5;
        let weight = (-(distance / 6.0).powi(2)).exp();
        let alpha = (weight * 110.0) as u8;
        if alpha < 3 {
            continue;
        }
        painter.hline(panel.x_range(), panel.top() - distance, egui::Stroke::new(1.0, egui::Color32::from_black_alpha(alpha)));
    }
}

fn strips(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, body: egui::Rect) {
    let linked = pair_linked(&snap.desk, app.bus);
    // Channel faders belong to this mix. A stereo pair is the even bus, one set of faders.
    let bus = app.bus;
    let wide = if app.bus_strip_open { BUS_W } else { BUS_NARROW };
    let master_w = wide;
    let channel_w = (body.width() - master_w).max(CH_W);
    let channels = egui::Rect::from_min_size(body.left_top(), egui::vec2(channel_w, body.height()));
    let masters = egui::Rect::from_min_max(egui::pos2(body.left() + channel_w, body.top()), body.right_bottom());
    ui.painter().rect_filled(masters, 0.0, BUS_BG);

    // The track is 40px and the same color as the mixer, so only the pill shows.
    // Channel faders stop above it. The master column runs through it, so the caps stay level.
    let view = egui::Rect::from_min_max(
        channels.left_top(),
        egui::pos2(channels.right(), (channels.bottom() - SCROLL_H).max(channels.top() + 120.0)),
    );
    let track = egui::Rect::from_min_max(egui::pos2(view.left(), view.bottom()), channels.right_bottom());
    let strip_h = view.height().max(120.0);
    let bar_id = ui.make_persistent_id("mix-scroll-bar");
    let mut bar = ui.ctx().data(|data| data.get_temp::<MixBar>(bar_id)).unwrap_or_default();
    let (pointer, pressed, down, wheel) = ui.ctx().input(|input| {
        (input.pointer.latest_pos(), input.pointer.primary_pressed(), input.pointer.primary_down(), input.smooth_scroll_delta)
    });
    let forced = bar.pointer(pointer, pressed, down, wheel);
    let lead = shown_inputs(app.all_inputs).first().copied().unwrap_or(0);
    let lead_rows = column_y(view.top(), strip_h, &plan_channel(strip_h, pan_visible(snap, app.bus, lead), false));
    let mut channel_ui = ui.new_child(egui::UiBuilder::new().max_rect(view));
    channel_ui.set_clip_rect(view.intersect(ui.clip_rect()));
    let mut area = egui::ScrollArea::horizontal()
        .id_salt("strips")
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = forced {
        area = area.horizontal_scroll_offset(offset);
    }
    let output = area.show(&mut channel_ui, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            ui.set_min_height(strip_h);
            for (place, input) in shown_inputs(app.all_inputs).into_iter().enumerate() {
                channel_strip(app, ui, snap, bus, input, strip_h, place == 0, false);
            }
        });
    });
    let offset = forced.unwrap_or_else(|| output.state.clamped_offset().x);
    bar.remember(track, offset, output.content_size.x, output.inner_rect.width());
    if bar.can_scroll() {
        paint_mix_thumb(ui.painter(), track, bar.thumb_left(), bar.thumb_w);
        if bar.dragging {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if pointer.is_some_and(|pos| {
            egui::Rect::from_min_size(egui::pos2(bar.thumb_left(), track.top()), egui::vec2(bar.thumb_w, track.height())).contains(pos)
        }) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    ui.ctx().data_mut(|data| data.insert_temp(bar_id, bar));

    let mut master_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(masters)
            .layout(egui::Layout::left_to_right(egui::Align::Min)),
    );
    master_ui.spacing_mut().item_spacing.x = 0.0;
    master_ui.set_clip_rect(masters.intersect(ui.clip_rect()));
    // One column. Main 1-2 is a stereo mix, so it does not get a second master.
    bus_strip(
        app,
        &mut master_ui,
        snap,
        bus,
        body.height(),
        true,
        true,
        lead_rows.fader_top,
        lead_rows.fader_bot,
        lead_rows.name,
        linked,
        false,
    );
    super::paint_edge_shadow(ui.painter(), masters.left(), masters.top(), masters.bottom(), false);
}

#[derive(Clone, Copy)]
struct MixBar {
    ready: bool,
    dragging: bool,
    anchor: f32,
    track_min_x: f32,
    track_min_y: f32,
    track_max_x: f32,
    track_max_y: f32,
    offset: f32,
    content_w: f32,
    view_w: f32,
    thumb_w: f32,
}

impl Default for MixBar {
    fn default() -> Self {
        Self {
            ready: false,
            dragging: false,
            anchor: 0.0,
            track_min_x: 0.0,
            track_min_y: 0.0,
            track_max_x: 0.0,
            track_max_y: 0.0,
            offset: 0.0,
            content_w: 0.0,
            view_w: 0.0,
            thumb_w: 0.0,
        }
    }
}

impl MixBar {
    fn track(&self) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(self.track_min_x, self.track_min_y),
            egui::pos2(self.track_max_x, self.track_max_y),
        )
    }

    fn max_off(&self) -> f32 {
        (self.content_w - self.view_w).max(0.0)
    }

    fn can_scroll(&self) -> bool {
        self.view_w > THUMB_MIN && self.thumb_w + 0.5 < self.view_w && self.max_off() > 1.0
    }

    fn travel(&self) -> f32 {
        (self.view_w - self.thumb_w).max(1.0)
    }

    fn thumb_left(&self) -> f32 {
        let max_off = self.max_off();
        let along = if max_off <= 1.0 { 0.0 } else { (self.offset / max_off).clamp(0.0, 1.0) };
        self.track_min_x + along * self.travel()
    }

    fn offset_at(&self, pointer_x: f32) -> f32 {
        let room = (self.view_w - self.thumb_w).max(0.0);
        let left = (pointer_x - self.anchor).clamp(self.track_min_x, self.track_min_x + room);
        let along = if room <= 1.0 { 0.0 } else { (left - self.track_min_x) / room };
        along.clamp(0.0, 1.0) * self.max_off()
    }

    /// Drag moves the channels. A click on the track jumps. Neither writes a level.
    fn pointer(&mut self, pos: Option<egui::Pos2>, pressed: bool, down: bool, wheel: egui::Vec2) -> Option<f32> {
        if !self.ready {
            return None;
        }
        let track = self.track();
        if pressed {
            if let Some(pos) = pos {
                if track.contains(pos) && self.can_scroll() {
                    self.dragging = true;
                    let thumb_left = self.thumb_left();
                    if pos.x >= thumb_left && pos.x <= thumb_left + self.thumb_w {
                        self.anchor = pos.x - thumb_left;
                    } else {
                        self.anchor = self.thumb_w * 0.5;
                    }
                }
            }
        }
        if self.dragging {
            if down {
                return pos.map(|pos| self.offset_at(pos.x));
            }
            self.dragging = false;
        }
        if self.can_scroll() {
            if let Some(pos) = pos {
                let delta = wheel.x + wheel.y;
                if track.contains(pos) && delta != 0.0 {
                    return Some((self.offset - delta).clamp(0.0, self.max_off()));
                }
            }
        }
        None
    }

    fn remember(&mut self, track: egui::Rect, offset: f32, content_w: f32, view_w: f32) {
        self.ready = view_w > 1.0;
        self.track_min_x = track.left();
        self.track_min_y = track.top();
        self.track_max_x = track.right();
        self.track_max_y = track.bottom();
        self.offset = offset.max(0.0);
        self.content_w = content_w;
        self.view_w = view_w;
        self.thumb_w = if content_w <= view_w + 0.5 {
            view_w
        } else {
            (view_w * view_w / content_w).max(THUMB_MIN).min(view_w)
        };
    }
}

fn paint_mix_thumb(painter: &egui::Painter, track: egui::Rect, thumb_x: f32, thumb_w: f32) {
    let visual_h = SCROLL_H - 5.0;
    let inset = 5.5;
    let width = (thumb_w - inset * 2.0).max(visual_h);
    let rect = egui::Rect::from_min_size(egui::pos2(thumb_x + inset, track.top() + 2.5), egui::vec2(width, visual_h));
    painter.rect_filled(rect, visual_h * 0.5, THUMB_INK);
}

fn channel_strip(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, bus: u16, input: u16, height: f32, first: bool, editor: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(CH_W, height), egui::Sense::hover());
    let mut col = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    col.set_clip_rect(rect.intersect(ui.clip_rect()));
    let painter = col.painter();
    painter.rect_filled(rect, 0.0, MIX_BG);
    // One hairline at the boundary. A second stroke on the same edge reads as a border.
    if !first {
        painter.vline(rect.left(), rect.y_range(), egui::Stroke::new(1.0, egui::Color32::from_rgb(62, 62, 62)));
    }
    // The in-plate strip has no bottom rule. The closed mixer keeps the hairline.
    if !editor {
        painter.hline(rect.x_range(), rect.bottom(), egui::Stroke::new(1.0, BLACK));
    }

    let geom = if editor && input < 10 {
        // Same column the plate scales from a 600px design. Full-size thumbs would eat the fader.
        editor_geom(rect.top(), height, pan_visible(snap, app.bus, input), app.bus % 2 == 0)
    } else {
        let mut plan = plan_channel(height, pan_visible(snap, app.bus, input), (28..42).contains(&input));
        if input >= 10 {
            plan.eq = false;
            plan.dynamics = false;
            plan.usb = (28..42).contains(&input);
        }
        mixer_geom(rect.top(), height, &plan)
    };
    let item_l = rect.left() + 7.0;
    let item_w = CH_W - 14.0;
    if let Some(y) = geom.stereo {
        let on = snap.desk.bus_link.get(app.bus as usize).copied().unwrap_or(0) != 0;
        let stereo = egui::Rect::from_min_size(egui::pos2(item_l, y), egui::vec2(item_w, geom.toggle_h));
        if toggle_at(&mut col, stereo, "Stereo", on, TOGGLE_BLUE, geom.font, ("ed-stereo", app.bus, input)).clicked() {
            app.act(Cmd::BusLink { bus: app.bus, linked: !on });
        }
    }
    if let Some(y) = geom.eq {
        if (input as usize) < app.fx.input_eq.len() {
            let thumb = egui::Rect::from_min_size(egui::pos2(item_l, y), egui::vec2(item_w, geom.thumb_h));
            let clicked = {
                super::fx::paint_eq_thumb(col.painter(), thumb, &app.fx.input_eq[input as usize]);
                thumb_click(&mut col, thumb, if editor { "ed-eq" } else { "eq" }, input)
            };
            if clicked {
                open_plate(app, Some(input), bus, super::fx::Plate::Eq);
            }
        }
    }
    if let Some(y) = geom.dyn_y {
        if (input as usize) < app.fx.dyns.len() {
            let thumb = egui::Rect::from_min_size(egui::pos2(item_l, y), egui::vec2(item_w, geom.thumb_h));
            let clicked = {
                super::fx::paint_dyn_thumb(col.painter(), thumb, &app.fx.dyns[input as usize], input < 2);
                thumb_click(&mut col, thumb, if editor { "ed-dyn" } else { "dyn" }, input)
            };
            if clicked {
                open_plate(app, Some(input), bus, super::fx::Plate::Dyn);
            }
        }
    }
    if let Some(y) = geom.usb {
        let painter = col.painter();
        painter.text(egui::pos2(rect.center().x, y + 8.0), egui::Align2::CENTER_CENTER, "Computer", face("NotoSans", 13.0), TICK);
        painter.text(egui::pos2(rect.center().x, y + 24.0), egui::Align2::CENTER_CENTER, "USB", face("NotoSans", 13.0), TICK);
    }
    if let Some(y) = geom.name {
        let name = egui::Rect::from_min_size(egui::pos2(item_l, y), egui::vec2(item_w, NAME_H));
        let caption = super::input_caption(&snap.desk, input);
        if app.rename == Some(super::Rename::Input(input)) {
            app.edit_name(&mut col, name);
        } else {
            paint_plate(&col, name, &caption, None, true);
            let response = col.interact(name, col.make_persistent_id(("ch-name", bus, input, editor)), egui::Sense::click());
            if response.double_clicked() {
                app.begin_rename(super::Rename::Input(input), &caption);
            } else if response.clicked() && input < 10 {
                open_plate(app, Some(input), bus, super::fx::Plate::Eq);
            }
        }
    }

    if let Some(y) = geom.pan {
        let pan_rect = egui::Rect::from_min_size(egui::pos2(item_l, y), egui::vec2(item_w, geom.pan_h));
        let index = topology::cell_index(bus, input);
        let resting = index.and_then(|index| snap.desk.pans.get(index).copied()).unwrap_or(0.5);
        let shown = app.preview_pan(bus, input).unwrap_or(resting);
        let event = pan_slider(&mut col, pan_rect, shown, bus_color(bus), app.hold(snap, false), ("pan", bus, input, editor));
        app.take_pan(event, bus, input);
    }

    let mute_rect = egui::Rect::from_min_size(egui::pos2(item_l, geom.mute), egui::vec2(item_w, geom.toggle_h));
    let index = topology::cell_index(bus, input);
    let edit = app.hold(snap, false);
    if let Some(index) = index {
        let muted = snap.desk.mutes.get(index).copied().unwrap_or(0) != 0;
        if toggle_at(&mut col, mute_rect, "MUTE", muted, MUTE_RED, geom.font, ("mute", bus, input, editor)).clicked() {
            act_switch(app, edit, Cmd::Mix { bus, input, gain: None, db: None, pan: None, mute: Some(!muted), solo: None });
        }
    }
    let solo_top = geom.solo;
    let fader_rect = egui::Rect::from_min_max(egui::pos2(rect.left() + 50.0, geom.fader_top), egui::pos2(rect.left() + 100.0, geom.fader_bot));
    let (meter_y, meter_h) = meter_band(fader_rect, geom.radius);
    paint_ticks(&col, rect.left(), fader_rect, geom.radius);
    paint_meter(
        &col,
        egui::Rect::from_min_size(egui::pos2(rect.left() + 26.0, meter_y), egui::vec2(19.0, meter_h)),
        channel_meter(app, snap, bus, input),
        input < 10,
        geom.slot_h,
        geom.slot_gap,
    );

    let resting = snap.desk.gain_at(bus, input);
    let shown = app.preview_gain(bus, Some(input)).unwrap_or(resting.unwrap_or(0.0));
    let locked = snap.locks.blocks_cell(bus, input);
    let level_hold = if resting.is_none() { Hold::Frozen } else { app.hold(snap, locked) };
    let (event, lock_click) = cap_fader(&mut col, fader_rect, shown, bus_color(bus), level_hold, locked, geom.radius, ("fader", bus, input, editor));
    if lock_click {
        app.lock(LockChange::Cell(bus, input, !locked));
    }
    if resting.is_none() {
        if matches!(event, Bar::Rejected | Bar::Double | Bar::Moved(_)) {
            app.drag = None;
            app.note("That mix has not arrived yet.");
        }
    } else {
        app.take_gain(event, bus, Some(input), locked);
    }

    if let Some(index) = index {
        let solo = snap.desk.solos.get(index).copied().unwrap_or(0) != 0;
        let solo_rect = egui::Rect::from_min_size(egui::pos2(item_l, solo_top), egui::vec2(item_w, geom.toggle_h));
        if toggle_at(&mut col, solo_rect, "SOLO", solo, SOLO_YELLOW, geom.font, ("solo", bus, input, editor)).clicked() {
            act_switch(app, edit, Cmd::Mix { bus, input, gain: None, db: None, pan: None, mute: None, solo: Some(!solo) });
        }
    }
}

fn bus_strip(
    app: &mut DeskApp,
    ui: &mut egui::Ui,
    snap: &Snap,
    bus: u16,
    height: f32,
    active: bool,
    collapse_here: bool,
    fader_top: f32,
    fader_bot: f32,
    name_y: f32,
    stereo: bool,
    editor: bool,
) {
    let width = if app.bus_strip_open { BUS_W } else { BUS_NARROW };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let mut col = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    col.set_clip_rect(rect.intersect(ui.clip_rect()));
    col.painter().rect_filled(rect, 0.0, BUS_BG);
    if active {
        col.painter().rect_filled(
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 3.0)),
            0.0,
            bus_color(bus),
        );
    }
    if !app.bus_strip_open {
        narrow_bus(&mut col, app, snap, bus, rect, collapse_here, editor);
        return;
    }

    let solo = any_solo(snap, bus);
    // Clear Solo sits under the cap only while a solo is up, so the idle fader matches the channels.
    let fader_bot = if solo { fader_bot - 40.0 } else { fader_bot };
    let mute_top = fader_top - 40.0;
    // The name shares the channel-name row, including the pan gap on a stereo mix.
    let name_top = name_y;
    let (mut show_eq, show_rvb) = bus_thumbs(rect.top() + 10.0, name_top - 5.0, (14..16).contains(&bus));
    if (bus as usize) >= app.fx.bus_eq.len() {
        show_eq = false;
    }
    let mut y = rect.top() + 10.0;
    let thumb_l = rect.left() + 20.0;
    if show_eq {
        let thumb = egui::Rect::from_min_size(egui::pos2(thumb_l, y), egui::vec2(91.0, THUMB_H));
        let clicked = {
            super::fx::paint_eq_thumb(col.painter(), thumb, &app.fx.bus_eq[bus as usize]);
            thumb_click(&mut col, thumb, if editor { "ed-bus-eq" } else { "bus-eq" }, bus)
        };
        if clicked {
            open_plate(app, None, bus, super::fx::Plate::Eq);
        }
        y += THUMB_H + 5.0;
    }
    if show_rvb {
        let thumb = egui::Rect::from_min_size(egui::pos2(thumb_l, y), egui::vec2(91.0, THUMB_H));
        let clicked = {
            super::fx::paint_reverb_thumb(col.painter(), thumb, &app.fx.reverb);
            thumb_click(&mut col, thumb, if editor { "ed-bus-rvb" } else { "bus-rvb" }, bus)
        };
        if clicked {
            open_plate(app, None, bus, super::fx::Plate::Reverb);
        }
    }

    let collapse = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 23.0), egui::vec2(30.0, 30.0));
    let clear = if solo {
        Some(egui::Rect::from_min_size(egui::pos2(rect.left() + 14.0, fader_bot + 8.0), egui::vec2(rect.width() - 28.0, TOGGLE_H)))
    } else {
        None
    };
    let name = egui::Rect::from_min_size(egui::pos2(rect.left() + 12.0, name_top), egui::vec2(BUS_W - 24.0, NAME_H));
    let mute = egui::Rect::from_min_size(egui::pos2(rect.left() + 7.0, mute_top), egui::vec2(BUS_W - 14.0, TOGGLE_H));
    let caption = shown_bus_title(&snap.desk, bus, stereo);
    if app.rename == Some(super::Rename::Output(bus)) {
        app.edit_name(&mut col, name);
    } else {
        paint_plate(&col, name, &caption, Some(bus_color(bus)), false);
        let response = col.interact(name, col.make_persistent_id(("bus-name", bus, editor)), egui::Sense::click());
        if response.double_clicked() && bus < 32 {
            app.begin_rename(super::Rename::Output(bus), &caption);
        } else if response.clicked() {
            let plate = if (bus as usize) < app.fx.bus_eq.len() { super::fx::Plate::Eq } else { super::fx::Plate::Reverb };
            open_plate(app, None, bus, plate);
        }
    }

    let muted = snap.desk.bus_mute.get(bus as usize).copied().unwrap_or(0) != 0;
    let edit = app.hold(snap, false);
    if toggle_at(&mut col, mute, "MUTE", muted, MUTE_RED, 14.0, ("bus-mute", bus, editor)).clicked() {
        act_switch(app, edit, Cmd::Bus { bus, gain: None, db: None, mute: Some(!muted) });
    }

    let fader = egui::Rect::from_min_max(egui::pos2(rect.left() + 67.0, fader_top), egui::pos2(rect.left() + 117.0, fader_bot));
    let radius = cap_radius(fader.height());
    paint_ticks(&col, rect.left() + 4.0, fader, radius);
    let (meter_y, meter_h) = meter_band(fader, radius);
    let meter = egui::Rect::from_min_size(egui::pos2(rect.left() + 26.0, meter_y), egui::vec2(30.0, meter_h));
    if stereo {
        // One well, split into L and R, the way a stereo bus meter is drawn.
        let gap = 4.0;
        let half = ((meter.width() - gap) * 0.5).max(2.0);
        paint_meter(&col, egui::Rect::from_min_size(meter.left_top(), egui::vec2(half, meter.height())), bus_level(snap, bus), false, 19.0, 6.0);
        paint_meter(
            &col,
            egui::Rect::from_min_size(egui::pos2(meter.right() - half, meter.top()), egui::vec2(half, meter.height())),
            bus_level(snap, bus + 1),
            false,
            19.0,
            6.0,
        );
    } else {
        paint_meter(&col, meter, bus_level(snap, bus), false, 19.0, 6.0);
    }
    let linear = snap.desk.bus_fader.get(bus as usize).copied().unwrap_or(1.0);
    let shown = app.preview_gain(bus, None).unwrap_or(linear);
    let locked = snap.locks.blocks_bus(bus);
    let (event, lock_click) = cap_fader(&mut col, fader, shown, bus_color(bus), app.hold(snap, locked), locked, radius, ("bus-fader", bus, editor));
    if lock_click {
        app.lock(LockChange::Bus(bus, !locked));
    }
    app.take_gain(event, bus, None, locked);

    if let Some(clear) = clear {
        if toggle_at(&mut col, clear, "CLEAR SOLO", false, bus_color(bus), 14.0, ("clear", bus, editor)).clicked() {
            clear_solos(app, snap, bus);
        }
    }
    if collapse_here && collapse_button(&mut col, collapse, true, editor).clicked() {
        app.bus_strip_open = false;
    }
}

fn narrow_bus(ui: &mut egui::Ui, app: &mut DeskApp, snap: &Snap, bus: u16, rect: egui::Rect, collapse_here: bool, editor: bool) {
    let fader = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top() + 8.0),
        egui::pos2(rect.right() - 4.0, rect.bottom() - 40.0),
    );
    let linear = snap.desk.bus_fader.get(bus as usize).copied().unwrap_or(1.0);
    let shown = app.preview_gain(bus, None).unwrap_or(linear);
    let locked = snap.locks.blocks_bus(bus);
    let (event, lock_click) = cap_fader(ui, fader, shown, bus_color(bus), app.hold(snap, locked), locked, cap_radius(fader.height()), ("bus-fader", bus, editor));
    if lock_click {
        app.lock(LockChange::Bus(bus, !locked));
    }
    app.take_gain(event, bus, None, locked);
    if collapse_here {
        let button = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 18.0), egui::vec2(30.0, 28.0));
        if collapse_button(ui, button, false, editor).clicked() {
            app.bus_strip_open = true;
        }
    }
}

struct ChannelPlan {
    eq: bool,
    dynamics: bool,
    usb: bool,
    pan: bool,
}

/// One thumb fits in `room`. A reverb bus keeps the decay plot when only one fits.
fn bus_thumbs(top: f32, limit: f32, reverb: bool) -> (bool, bool) {
    let room = limit - top;
    let pair = THUMB_H + 5.0 + THUMB_H;
    if reverb {
        if room >= pair {
            return (true, true);
        }
        if room >= THUMB_H {
            return (false, true);
        }
        return (false, false);
    }
    (room >= THUMB_H, false)
}

fn fader_room(height: f32, eq: bool, dynamics: bool, pan: bool, usb: bool) -> f32 {
    let mut used = 10.0 + NAME_H + 5.0 + TOGGLE_H + 10.0 + TOGGLE_H + 10.0 + 10.0;
    if eq {
        used += THUMB_H + 5.0;
    }
    if dynamics {
        used += THUMB_H + 5.0;
    }
    if usb {
        used += 32.0;
    }
    if pan {
        used += TOGGLE_H + 10.0;
    }
    height - used
}

fn plan_channel(height: f32, pan: bool, usb: bool) -> ChannelPlan {
    // Match the Electron gates, then drop a plate if the fader would have nowhere to travel.
    let mut eq = height > EQ_STRIP_H;
    let mut dynamics = height > DYN_STRIP_H;
    loop {
        let show_usb = usb && !eq && !dynamics;
        if fader_room(height, eq, dynamics, pan, show_usb) >= 72.0 || (!eq && !dynamics) {
            return ChannelPlan { eq, dynamics, usb: show_usb, pan };
        }
        if dynamics {
            dynamics = false;
        } else {
            eq = false;
        }
    }
}

struct StripRows {
    eq: Option<f32>,
    dynamics: Option<f32>,
    usb: Option<f32>,
    name: f32,
    pan: Option<f32>,
    mute: f32,
    fader_top: f32,
    fader_bot: f32,
    solo: f32,
}

fn column_y(top: f32, height: f32, plan: &ChannelPlan) -> StripRows {
    let mut y = top + 10.0;
    let eq = plan.eq.then(|| {
        let at = y;
        y += THUMB_H + 5.0;
        at
    });
    let dynamics = plan.dynamics.then(|| {
        let at = y;
        y += THUMB_H + 5.0;
        at
    });
    let usb = plan.usb.then(|| {
        let at = y;
        y += 32.0;
        at
    });
    let name = y;
    y += NAME_H + 5.0;
    let pan = plan.pan.then(|| {
        let at = y;
        y += TOGGLE_H + 10.0;
        at
    });
    let mute = y;
    let solo = top + height - 10.0 - TOGGLE_H;
    StripRows {
        eq,
        dynamics,
        usb,
        name,
        pan,
        mute,
        fader_top: mute + TOGGLE_H + 10.0,
        fader_bot: solo - 10.0,
        solo,
    }
}

struct StripGeom {
    eq: Option<f32>,
    dyn_y: Option<f32>,
    usb: Option<f32>,
    thumb_h: f32,
    stereo: Option<f32>,
    name: Option<f32>,
    pan: Option<f32>,
    pan_h: f32,
    mute: f32,
    toggle_h: f32,
    fader_top: f32,
    fader_bot: f32,
    solo: f32,
    radius: f32,
    font: f32,
    slot_h: f32,
    slot_gap: f32,
}

fn mixer_geom(top: f32, height: f32, plan: &ChannelPlan) -> StripGeom {
    let rows = column_y(top, height, plan);
    StripGeom {
        eq: rows.eq,
        dyn_y: rows.dynamics,
        usb: rows.usb,
        thumb_h: THUMB_H,
        stereo: None,
        name: Some(rows.name),
        pan: rows.pan,
        pan_h: TOGGLE_H,
        mute: rows.mute,
        toggle_h: TOGGLE_H,
        fader_top: rows.fader_top,
        fader_bot: rows.fader_bot,
        solo: rows.solo,
        radius: cap_radius(rows.fader_bot - rows.fader_top),
        font: 14.0,
        slot_h: 19.0,
        slot_gap: 6.0,
    }
}

/// In-plate strip. Electron lays this out at 600px and scales the whole plate into the dock,
/// so the buttons shrink and the fader stays the long part of the column.
fn editor_geom(top: f32, height: f32, pan: bool, stereo: bool) -> StripGeom {
    let scale = (height / 600.0).clamp(0.4, 1.15);
    let thumb_h = 76.5 * scale;
    let toggle_h = 30.0 * scale;
    let pan_h = 60.0 * scale;
    let gap = 5.0 * scale;
    let radius = 20.0 * scale;
    let mut y = top + 10.0 * scale;
    let stereo_y = if stereo {
        let at = y;
        y += toggle_h + gap;
        Some(at)
    } else {
        None
    };
    let eq = y;
    y += thumb_h + gap;
    let dyn_y = y;
    y += thumb_h + gap;
    let pan_y = if pan {
        let at = y;
        y += pan_h + 10.0 * scale;
        Some(at)
    } else {
        None
    };
    let mute = y;
    let solo = top + height - 10.0 * scale - toggle_h;
    let fader_top = mute + toggle_h + 10.0 * scale;
    let fader_bot = (solo - 10.0 * scale).max(fader_top + 48.0);
    StripGeom {
        eq: Some(eq),
        dyn_y: Some(dyn_y),
        usb: None,
        thumb_h,
        stereo: stereo_y,
        name: None,
        pan: pan_y,
        pan_h,
        mute,
        toggle_h,
        fader_top,
        fader_bot,
        solo,
        radius,
        font: (14.0 * scale).clamp(10.0, 14.0),
        slot_h: 19.0 * scale,
        slot_gap: 6.0 * scale,
    }
}

/// Plate under the fader. A stereo pair reads "MAIN 1-2", not "MAIN 1" beside "MAIN 2".
fn strip_title(bus: u16, stereo: bool) -> String {
    if stereo {
        pair_label(bus).trim_end_matches(" MIX").to_string()
    } else {
        single_name(bus)
    }
}

fn pan_visible(snap: &Snap, bus: u16, input: u16) -> bool {
    pair_linked(&snap.desk, bus & !1) && !channel_linked(&snap.desk, input)
}

fn channel_linked(desk: &crate::desk::Desk, input: u16) -> bool {
    let even = input & !1;
    desk.in_link.get(even as usize).copied().unwrap_or(0) != 0
}

fn open_plate(app: &mut DeskApp, input: Option<u16>, bus: u16, plate: super::fx::Plate) {
    let same = match input {
        Some(id) => app.focus == Some(id) && !app.focus_bus,
        None => app.focus.is_none() && app.focus_bus && app.fx_bus == bus,
    };
    if same && app.plate == plate {
        app.focus = None;
        app.focus_bus = false;
        app.fx.drag = None;
        return;
    }
    app.focus = input;
    app.focus_bus = input.is_none();
    app.fx_bus = bus;
    app.plate = plate;
    app.fx.selected = 0;
    app.fx.drag = None;
}

fn thumb_click(ui: &mut egui::Ui, rect: egui::Rect, salt: &str, which: u16) -> bool {
    ui.interact(rect, ui.make_persistent_id((salt, which)), egui::Sense::click()).clicked()
}

fn act_switch(app: &mut DeskApp, edit: Hold, cmd: Cmd) {
    if matches!(edit, Hold::Free) {
        app.act(cmd);
    } else {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
    }
}

fn clear_solos(app: &mut DeskApp, snap: &Snap, bus: u16) {
    if !matches!(app.hold(snap, false), Hold::Free) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    let mut cmds = Vec::new();
    for input in 0..INPUTS as u16 {
        let Some(index) = topology::cell_index(bus, input) else { continue };
        if snap.desk.solos.get(index).copied().unwrap_or(0) == 0 {
            continue;
        }
        cmds.push(Cmd::Mix { bus, input, gain: None, db: None, pan: None, mute: None, solo: Some(false) });
    }
    if !cmds.is_empty() {
        app.act_all(&cmds);
    }
}

fn any_solo(snap: &Snap, bus: u16) -> bool {
    let start = bus as usize * INPUTS;
    snap.desk.solos.get(start..start + INPUTS).is_some_and(|band| band.iter().any(|value| *value != 0))
}

fn paint_plate(ui: &egui::Ui, rect: egui::Rect, text: &str, stroke: Option<egui::Color32>, underline: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, BLACK);
    if let Some(color) = stroke {
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.1, color), egui::StrokeKind::Inside);
    }
    let font = face("NotoSansReg", 16.0);
    let shown = fit_text(painter, text, font.clone(), rect.width() - 8.0);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, &shown, font.clone(), WHITE);
    if underline {
        let width = painter.layout_no_wrap(shown, font, WHITE).size().x;
        let y = rect.center().y + 10.0;
        painter.hline((rect.center().x - width * 0.5)..=(rect.center().x + width * 0.5), y, egui::Stroke::new(1.0, WHITE));
    }
}

fn fit_text(painter: &egui::Painter, text: &str, font: egui::FontId, max_w: f32) -> String {
    if painter.layout_no_wrap(text.to_owned(), font.clone(), WHITE).size().x <= max_w {
        return text.to_owned();
    }
    let mut cut: String = text.into();
    while !cut.is_empty() {
        cut.pop();
        let trial = format!("{cut}…");
        if painter.layout_no_wrap(trial.clone(), font.clone(), WHITE).size().x <= max_w {
            return trial;
        }
    }
    "…".into()
}

fn toggle_at(ui: &mut egui::Ui, rect: egui::Rect, label: &str, on: bool, on_color: egui::Color32, font_px: f32, salt: impl std::hash::Hash + std::fmt::Debug) -> egui::Response {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click());
    paint_toggle(ui.painter(), rect, label, on, on_color, font_px);
    response
}

fn paint_toggle(painter: &egui::Painter, rect: egui::Rect, label: &str, on: bool, on_color: egui::Color32, font_px: f32) {
    let visual = rect.shrink(2.0);
    let font = face("NotoSans", font_px);
    if on {
        painter.rect_filled(visual, 6.0, on_color);
        painter.text(visual.center(), egui::Align2::CENTER_CENTER, label, font, BLACK);
    } else {
        painter.rect_stroke(visual, 6.0, egui::Stroke::new(1.0, TOGGLE), egui::StrokeKind::Inside);
        painter.text(visual.center(), egui::Align2::CENTER_CENTER, label, font, TOGGLE);
    }
}

fn collapse_button(ui: &mut egui::Ui, rect: egui::Rect, open: bool, editor: bool) -> egui::Response {
    let id = if editor { "mix-bus-collapse-plate" } else { "mix-bus-collapse" };
    let response = ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click());
    let painter = ui.painter();
    let c = rect.center();
    // Two arrows. Open points inward. Collapsed points outward.
    let (left_dir, right_dir) = if open { (-1.0, 1.0) } else { (1.0, -1.0) };
    chevron(painter, c + egui::vec2(-7.0, 0.0), left_dir);
    chevron(painter, c + egui::vec2(7.0, 0.0), right_dir);
    response
}

fn chevron(painter: &egui::Painter, center: egui::Pos2, dir: f32) {
    let stroke = egui::Stroke::new(1.4, TOGGLE);
    let tip = center + egui::vec2(dir * 4.0, 0.0);
    painter.line_segment([tip, center + egui::vec2(-dir * 3.0, -4.0)], stroke);
    painter.line_segment([tip, center + egui::vec2(-dir * 3.0, 4.0)], stroke);
}

fn pan_slider(ui: &mut egui::Ui, rect: egui::Rect, pan: f32, color: egui::Color32, hold: Hold, salt: impl std::hash::Hash + std::fmt::Debug) -> Bar {
    let response = ui.interact(rect, ui.make_persistent_id(salt), egui::Sense::click_and_drag());
    let follow = matches!(hold, Hold::Free);
    let mut t = pan.clamp(0.0, 1.0);
    let event = if response.double_clicked() {
        if follow { Bar::Double } else { Bar::Rejected }
    } else if !follow && (response.drag_started() || response.dragged()) {
        Bar::Rejected
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            t = ((pos.x - (rect.left() + 2.0)) / (rect.width() - 4.0).max(1.0)).clamp(0.0, 1.0);
            Bar::Moved(t)
        } else {
            Bar::Idle
        }
    } else if response.drag_stopped() && follow {
        Bar::Released
    } else {
        Bar::Idle
    };
    let painter = ui.painter();
    let y = rect.center().y;
    painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 3.0)), 0.0, BLACK);
    let x = rect.left() + 2.0 + t * (rect.width() - 4.0);
    let mid = rect.center().x;
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(x.min(mid), y - 0.4), egui::pos2(x.max(mid), y + 0.4)),
        0.0,
        WHITE,
    );
    painter.rect_filled(egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(3.0, 10.0)), 0.0, color);
    event
}

fn cap_fader(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    shown: f32,
    color: egui::Color32,
    hold: Hold,
    locked: bool,
    radius: f32,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> (Bar, bool) {
    if rect.height() < 24.0 || rect.width() < 8.0 {
        return (Bar::Idle, false);
    }
    let drag_top = rect.top() + radius + 2.0;
    let drag_bot = rect.bottom() - radius - 2.0;
    let drag_h = (drag_bot - drag_top).max(1.0);
    let travel = travel_of(shown);
    let cap_y = drag_top + (1.0 - travel) * drag_h;
    let cx = rect.left() + rect.width() * 0.5;
    let lock_rect = egui::Rect::from_min_size(rect.left_top() + egui::vec2(2.0, 2.0), egui::vec2(18.0, 16.0));
    let cap_rect = egui::Rect::from_center_size(egui::pos2(cx, cap_y), egui::vec2(radius * 2.0, radius * 2.0));
    let id = ui.make_persistent_id(salt);
    let response = ui.interact(cap_rect, id, egui::Sense::click_and_drag());
    let lock_response = ui.interact(lock_rect, id.with("lock"), egui::Sense::click());
    let pressed_lock = ui.input(|input| input.pointer.press_origin().is_some_and(|pos| lock_rect.contains(pos)));
    let follow = matches!(hold, Hold::Free) && !pressed_lock;
    let event = if response.double_clicked() && !pressed_lock {
        if follow { Bar::Double } else { Bar::Rejected }
    } else if !follow && !pressed_lock && (response.drag_started() || response.dragged() || response.double_clicked()) {
        Bar::Rejected
    } else if follow && (response.drag_started() || response.dragged()) {
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let anchor = cap_y - pos.y;
                ui.ctx().data_mut(|data| data.insert_temp(id, anchor));
            }
        }
        if response.dragged() {
            let anchor = ui.ctx().data(|data| data.get_temp::<f32>(id)).unwrap_or(0.0);
            if let Some(pos) = response.interact_pointer_pos() {
                let center = pos.y + anchor;
                let next = (1.0 - (center - drag_top) / drag_h).clamp(0.0, 1.0);
                Bar::Moved(linear_of(next) / 4.0)
            } else {
                Bar::Idle
            }
        } else {
            Bar::Idle
        }
    } else if response.drag_stopped() && follow {
        Bar::Released
    } else {
        Bar::Idle
    };

    let painter = ui.painter();
    let track_top = rect.top() + radius;
    let track_bot = rect.bottom() - radius;
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(cx - 1.5, track_top), egui::pos2(cx + 1.5, track_bot)),
        0.0,
        BLACK,
    );
    painter.circle_filled(egui::pos2(cx, cap_y), radius, BLACK);
    painter.circle_stroke(egui::pos2(cx, cap_y), (radius - 6.0).max(4.0), egui::Stroke::new(2.0, color));
    paint_padlock(painter, lock_rect, locked);
    (event, lock_response.clicked())
}

fn cap_radius(height: f32) -> f32 {
    if height < 96.0 { 12.0 } else { 20.0 }
}

fn meter_band(fader: egui::Rect, radius: f32) -> (f32, f32) {
    let top = fader.top() + radius + 2.0;
    let height = (fader.height() - 2.0 * (radius + 2.0)).max(8.0);
    (top, height)
}

fn shown_bus_title(desk: &crate::desk::Desk, bus: u16, stereo: bool) -> String {
    let custom = super::bus_custom(desk, bus, stereo);
    if custom.is_empty() {
        strip_title(bus, stereo)
    } else {
        custom
    }
}

fn channel_meter(app: &DeskApp, snap: &Snap, bus: u16, input: u16) -> Option<f32> {
    let mut db = super::meter_of(&snap.desk.gear, topology::input_meter(input as usize))?;
    if app.meter_post {
        let linear = snap.desk.gain_at(bus, input).unwrap_or(0.0);
        if linear <= 1.0e-6 {
            return None;
        }
        db += topology::db_of(linear);
    }
    Some(db)
}

fn bus_level(snap: &Snap, bus: u16) -> Option<f32> {
    super::meter_of(&snap.desk.gear, topology::bus_meter(bus as usize))
}

fn paint_meter(ui: &egui::Ui, rect: egui::Rect, db: Option<f32>, clip_slot: bool, slot_h: f32, slot_gap: f32) {
    let painter = ui.painter();
    // The black well is the travel from silence up to 0 dB. The +12 headroom stays empty.
    let zero = ctrl_of_db(0.0);
    let well_top = rect.top() + rect.height() * (1.0 - zero);
    let well = egui::Rect::from_min_max(egui::pos2(rect.left(), well_top), rect.right_bottom());
    // The clip slot sits just above 0 dB on the analog inputs. The well itself stays one column.
    if clip_slot {
        let slot = egui::Rect::from_min_size(egui::pos2(rect.left(), well.top() - slot_gap - slot_h), egui::vec2(rect.width(), slot_h));
        let hot = db.is_some_and(|db| db >= -0.05);
        painter.rect_filled(slot, 0.0, if hot { egui::Color32::from_rgb(210, 40, 40) } else { BLACK });
    }
    super::paint_level(painter, well, db);
}

fn paint_ticks(ui: &egui::Ui, text_left: f32, fader: egui::Rect, radius: f32) {
    let top = fader.top() + radius + 2.0;
    let height = (fader.height() - 2.0 * (radius + 2.0)).max(1.0);
    let painter = ui.painter();
    let font = egui::FontId::proportional(9.0);
    for (db, label) in [(12.0, "+12"), (0.0, "0"), (-12.0, "−12"), (-24.0, "−24"), (-48.0, "−48")] {
        let y = top + height * (1.0 - ctrl_of_db(db));
        painter.text(egui::pos2(text_left + 16.0, y), egui::Align2::RIGHT_CENTER, label, font.clone(), TICK);
        painter.hline((text_left + 18.0)..=(fader.left() - 2.0), y, egui::Stroke::new(1.0, TICK));
    }
    let y = top + height;
    painter.text(egui::pos2(text_left + 16.0, y), egui::Align2::RIGHT_CENTER, "−∞", font, TICK);
    painter.hline((text_left + 18.0)..=(fader.left() - 2.0), y, egui::Stroke::new(1.0, TICK));
}

fn exp12() -> f32 {
    (1.0_f32 - 24.0 / 84.0).ln() / 0.66_f32.ln()
}

/// Travel of a dB value on the +12 dB fader. −12 dB sits at 0.66. Silence is 0.
fn ctrl_of_db(db: f32) -> f32 {
    if !db.is_finite() || db < -71.0 {
        return 0.0;
    }
    if db > 12.0 {
        return 1.0;
    }
    let scaled = (db - 12.0) / 84.0 + 1.0;
    if scaled <= 0.0 {
        return 0.0;
    }
    scaled.powf(1.0 / exp12())
}

fn db_of_ctrl(travel: f32) -> f32 {
    if travel <= 0.0 {
        return f32::NEG_INFINITY;
    }
    84.0 * (travel.clamp(0.0, 1.0).powf(exp12()) - 1.0) + 12.0
}

fn travel_of(linear: f32) -> f32 {
    if linear <= 0.001 {
        return 0.0;
    }
    ctrl_of_db(20.0 * linear.log10())
}

fn linear_of(travel: f32) -> f32 {
    if travel <= 0.001 {
        return 0.0;
    }
    let db = db_of_ctrl(travel);
    if !db.is_finite() {
        return 0.0;
    }
    10f32.powf(db / 20.0).clamp(0.0, 4.0)
}

fn face(name: &str, size: f32) -> egui::FontId {
    egui::FontId { size, family: egui::FontFamily::Name(name.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_sits_on_the_zero_tick() {
        let zero = ctrl_of_db(0.0);
        let travel = travel_of(1.0);
        assert!((travel - zero).abs() < 1.0e-4, "{travel} vs {zero}");
        assert!((linear_of(zero) - 1.0).abs() < 1.0e-3, "{}", linear_of(zero));
        assert!((db_of_ctrl(zero)).abs() < 1.0e-3, "{}", db_of_ctrl(zero));
        assert_eq!(travel_of(0.0), 0.0);
        assert_eq!(ctrl_of_db(f32::NEG_INFINITY), 0.0);
        assert!((ctrl_of_db(12.0) - 1.0).abs() < 1.0e-6);
        // 0.66 is 24 dB down from +12, which is −12 dB.
        assert!((ctrl_of_db(-12.0) - 0.66).abs() < 1.0e-3, "{}", ctrl_of_db(-12.0));
        assert!((db_of_ctrl(ctrl_of_db(-24.0)) + 24.0).abs() < 1.0e-2);
        assert!(ctrl_of_db(-48.0) > 0.0 && ctrl_of_db(-48.0) < ctrl_of_db(-24.0));
        assert!(ctrl_of_db(-24.0) < ctrl_of_db(-12.0) && ctrl_of_db(-12.0) < zero);
    }

    #[test]
    fn channel_and_bus_faders_share_a_row() {
        let height = 430.0;
        let plan = plan_channel(height, true, false);
        assert!(plan.eq);
        assert!(!plan.dynamics);
        let rows = column_y(0.0, height, &plan);
        let travel = rows.fader_bot - rows.fader_top;
        assert!(travel > 160.0, "{travel}");
        assert!((rows.fader_top - (rows.mute + TOGGLE_H + 10.0)).abs() < 0.1);
        let (eq, rvb) = bus_thumbs(10.0, rows.name - 5.0, false);
        assert!(eq && !rvb);
        let (eq, rvb) = bus_thumbs(10.0, rows.name - 5.0, true);
        assert!(!eq && rvb);
        let mono = plan_channel(height, false, false);
        assert!(!mono.pan);
        assert!(mono.eq && !mono.dynamics);
    }

    #[test]
    fn a_linked_main_is_one_strip_name() {
        assert_eq!(strip_title(0, true), "MAIN 1-2");
        assert_eq!(strip_title(0, false), "MAIN 1");
        assert_eq!(strip_title(1, false), "MAIN 2");
        assert_eq!(strip_title(2, true), "LINE 3-4");
        assert_eq!(strip_title(10, true), "PHONES 1");
        assert_eq!(strip_title(14, true), "REVERB");
    }

    #[test]
    fn plate_strip_keeps_a_long_fader() {
        let geom = editor_geom(0.0, 390.0, true, true);
        let travel = geom.fader_bot - geom.fader_top;
        assert!(travel > 120.0, "{travel}");
        let meter = travel - 2.0 * (geom.radius + 2.0);
        assert!(meter > 90.0, "{meter}");
        assert!(geom.name.is_none());
        assert!(geom.stereo.is_some());
        assert!(geom.radius < 16.0, "{}", geom.radius);
        // Unity sits far enough down the travel that the lock, in the top 16px, stays clear of the cap.
        let drop = (1.0 - ctrl_of_db(0.0)) * meter;
        assert!(drop > 18.0, "{drop}");
        let closed = mixer_geom(0.0, 490.0, &plan_channel(490.0, true, false));
        assert!(closed.name.is_some());
        assert!(closed.stereo.is_none());
        assert!((closed.radius - 20.0).abs() < 0.1);
    }
}

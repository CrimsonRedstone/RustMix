//! Home. The five channel cards, the monitor, and the phones.
//! The arc does not drag. −1 and +1 are the only level changes.

use eframe::egui;

use super::{device_title, monitor_text, pair_text, stored_fraction, DeskApp, Snap, CYAN, DIM, UNAVAILABLE, WHITE};
use crate::plan::Cmd;
use crate::topology::{self, NOTHING};

pub(crate) struct HomeScroll {
    pub offset: f32,
    pub view_h: f32,
    pub content_h: f32,
}

pub(crate) fn show(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) -> HomeScroll {
    let scroll = egui::ScrollArea::vertical()
        .id_salt("home")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false])
        .show(ui, |ui| {
        // The rule and the 828 scroll with the page. At the top, 46px of title,
        // the 6px item gap, and this 32px put the rack on the same row as before.
        header(ui, app.wordmark.as_ref(), super::unit_open(snap));
        ui.add_space(32.0);
        face(ui, app.face.as_ref());
        // The reference starts the cards 180px under the photo top. The extra pull-up
        // cancels item spacing so the card stroke lands on the reference row.
        ui.add_space(-17.5);
        // The reference row is 760 wide and starts 38px from the content edge.
        // This page is already inset 18px, so the rest of that inset is 20px.
        // Five cards and four 21px gaps fill the 760.
        let cards_h = 224.0;
        let row_w = 760.0;
        let gap = 21.0;
        let card_w = (row_w - gap * 4.0) / 5.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.add_space(20.0);
            mic_card(app, ui, snap, card_w, cards_h);
            mic2_card(app, ui, snap, card_w, cards_h);
            monitor_card(app, ui, snap, card_w, cards_h);
            phones_card(app, ui, snap, 0, card_w, cards_h);
            phones_card(app, ui, snap, 1, card_w, cards_h);
        });
        // item_spacing.y (6) is already between the rows. 14 more is the reference's 20.
        ui.add_space(14.0);
        let ab_w = 184.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.add_space(20.0);
            ab_card(app, ui, snap, ab_w);
            talk_card(app, ui, snap, row_w - ab_w - gap);
        });
        ui.add_space(14.0);
        input_monitor(app, ui, snap);
    });
    HomeScroll {
        offset: scroll.state.clamped_offset().y,
        view_h: scroll.inner_rect.height(),
        content_h: scroll.content_size.y,
    }
}

fn header(ui: &mut egui::Ui, wordmark: Option<&egui::TextureHandle>, live: bool) {
    device_title(ui, wordmark, live);
}

pub(crate) fn face(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) {
    // 1702×388 at 0.48 is 817×186. The reference centers (width + 20) and never squashes the height.
    const SCALE: f32 = 0.48;
    const PAD: f32 = 20.0;
    let natural_w = 1702.0 * SCALE;
    let natural_h = 388.0 * SCALE;
    let avail = ui.available_width();
    let fit = if avail.is_finite() {
        (avail / (natural_w + PAD)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let width = natural_w * fit;
    let height = natural_h * fit;
    let slot_w = if avail.is_finite() { avail.max(1.0) } else { natural_w + PAD };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(slot_w, height.max(1.0)), egui::Sense::hover());
    if let Some(texture) = texture {
        let slot = width + PAD * fit;
        let left = rect.left() + (slot_w - slot) * 0.5;
        let image = egui::Rect::from_min_size(egui::pos2(left, rect.top()), egui::vec2(width, height));
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(texture.id(), image, uv, egui::Color32::WHITE);
    }
}

fn mic_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32, height: f32) {
    let insert = app.mic_insert;
    let preamp = if insert { 10 } else { 0 };
    let (fraction, value) = super::preamp_view(&snap.desk.gear, preamp);
    let phantom = crate::gear::flag(snap.desk.gear.phantom[0]);
    let pad = crate::gear::flag(snap.desk.gear.pad[0]);
    home_card(ui, "Mic", egui::vec2(width, height), |ui| {
        let area = ui.max_rect();
        let knob = knob_rect(area, 12.0, false);
        paint_meters(ui, meter_left(area, knob, 1, true), &[super::meter_of(&snap.desk.gear, topology::input_meter(0))]);
        let row_w = 73.0;
        let top = area.top() + 27.0;
        let left = knob.center().x - row_w * 0.5;
        let half = (row_w - 5.0) * 0.5;
        if toggle(ui, "mic-48", egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(half, 25.0)), "48v", phantom, BLUE, 10.0) && app.writable(snap) {
            app.act(Cmd::Phantom { index: 0, on: !phantom });
        }
        if toggle(ui, "mic-pad", egui::Rect::from_min_size(egui::pos2(left + half + 5.0, top), egui::vec2(half, 25.0)), "Pad", pad, BLUE, 10.0) && app.writable(snap) {
            app.act(Cmd::Pad { index: 0, on: !pad });
        }
        if toggle(ui, "mic-pre", egui::Rect::from_min_size(egui::pos2(left, top + 30.0), egui::vec2(row_w, 25.0)), "Preamp Gain", !insert, BLUE, 10.0) {
            app.mic_insert = false;
        }
        if toggle(ui, "mic-ins", egui::Rect::from_min_size(egui::pos2(left, top + 60.0), egui::vec2(row_w, 25.0)), "Insert Gain", insert, BLUE, 10.0) {
            app.mic_insert = true;
        }
        let title = if insert { ("Insert", "Gain") } else { ("Preamp", "Gain") };
        let hit = knob_face(ui, "mic", knob, KnobFace { fraction, locked: false, all: false, all_on: false, steps: false, quiet: true, title: Some(title), value: &value });
        if hit.lock {
            app.note(UNAVAILABLE);
        } else if hit.arc && app.writable(snap) {
            app.act(Cmd::StepPreamp { index: preamp, louder: super::arc_louder(ui, knob.top() + 35.0) });
        }
    });
}

fn mic2_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32, height: f32) {
    let (fraction, value) = super::preamp_view(&snap.desk.gear, 1);
    let phantom = crate::gear::flag(snap.desk.gear.phantom[1]);
    let pad = crate::gear::flag(snap.desk.gear.pad[1]);
    home_card(ui, "Mic/Line/Inst 2", egui::vec2(width, height), |ui| {
        let area = ui.max_rect();
        let knob = knob_rect(area, 12.0, false);
        paint_meters(ui, meter_left(area, knob, 1, false), &[super::meter_of(&snap.desk.gear, topology::input_meter(1))]);
        let cx = knob.center().x;
        if toggle(ui, "mic2-48", egui::Rect::from_min_size(egui::pos2(cx - 26.0, area.top() + 27.0), egui::vec2(52.0, 30.0)), "48V", phantom, BLUE, 14.0) && app.writable(snap) {
            app.act(Cmd::Phantom { index: 1, on: !phantom });
        }
        if toggle(ui, "mic2-pad", egui::Rect::from_min_size(egui::pos2(cx - 26.0, area.top() + 73.0), egui::vec2(52.0, 30.0)), "Pad", pad, BLUE, 14.0) && app.writable(snap) {
            app.act(Cmd::Pad { index: 1, on: !pad });
        }
        let hit = knob_face(ui, "mic2", knob, KnobFace { fraction, locked: false, all: false, all_on: false, steps: false, quiet: true, title: None, value: &value });
        if hit.lock {
            app.note(UNAVAILABLE);
        } else if hit.arc && app.writable(snap) {
            app.act(Cmd::StepPreamp { index: 1, louder: super::arc_louder(ui, knob.top() + 35.0) });
        }
    });
}

fn monitor_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32, height: f32) {
    let locked = snap.locks.blocks_monitor();
    let all = snap.locks.all;
    let quiet = locked || !app.writable(snap);
    let value = monitor_text(&snap.desk);
    home_card(ui, "Monitor", egui::vec2(width, height), |ui| {
        let area = ui.max_rect();
        let knob = knob_rect(area, 18.0, true);
        paint_meters(ui, meter_left(area, knob, 2, false), &[
            super::jack_level(&snap.desk.gear, &snap.desk.patch, topology::line_slot(0)),
            super::jack_level(&snap.desk.gear, &snap.desk.patch, topology::line_slot(1)),
        ]);
        let muted = crate::gear::flag(snap.desk.gear.mon_mute);
        let mono = crate::gear::flag(snap.desk.gear.mono);
        if toggle(ui, "mon-mute", egui::Rect::from_min_size(egui::pos2(knob.left(), area.top() + 27.0), egui::vec2(68.0, 30.0)), "MUTE", muted, egui::Color32::from_rgb(210, 40, 40), 14.0) && app.writable(snap) {
            app.act(Cmd::MonMute { on: !muted });
        }
        if toggle(ui, "mon-mono", egui::Rect::from_min_size(egui::pos2(knob.left(), area.top() + 73.0), egui::vec2(68.0, 30.0)), "MONO", mono, BLUE, 14.0) && app.writable(snap) {
            app.act(Cmd::MonMono { on: !mono });
        }
        let hit = knob_face(ui, "mon", knob, KnobFace {
            fraction: stored_fraction(snap.desk.monitor),
            locked,
            all: true,
            all_on: all,
            steps: true,
            quiet,
            title: None,
            value: &value,
        });
        if hit.all {
            app.lock(crate::engine::LockChange::All(!all));
        }
        if hit.lock {
            app.lock(crate::engine::LockChange::Monitor(!locked));
        }
        if hit.down {
            app.act(Cmd::StepMonitor { louder: false });
        }
        if hit.up {
            app.act(Cmd::StepMonitor { louder: true });
        }
    });
}

fn phones_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, pair: u8, width: f32, height: f32) {
    let title = if pair == 0 { "Phones 1" } else { "Phones 2" };
    let locked = snap.locks.blocks_phones(pair);
    let quiet = locked || !app.writable(snap);
    let left = snap.desk.phones.get(pair as usize * 2).copied().flatten();
    let right = snap.desk.phones.get(pair as usize * 2 + 1).copied().flatten();
    let value = pair_text(left, right);
    let id = if pair == 0 { "ph1" } else { "ph2" };
    home_card(ui, title, egui::vec2(width, height), |ui| {
        let area = ui.max_rect();
        let knob = knob_rect(area, 18.0, true);
        paint_meters(ui, meter_left(area, knob, 2, false), &[
            super::jack_level(&snap.desk.gear, &snap.desk.patch, topology::phone_slot(pair as usize * 2)),
            super::jack_level(&snap.desk.gear, &snap.desk.patch, topology::phone_slot(pair as usize * 2 + 1)),
        ]);
        let hit = knob_face(ui, id, knob, KnobFace {
            fraction: stored_fraction(left.or(right)),
            locked,
            all: false,
            all_on: false,
            steps: true,
            quiet,
            title: None,
            value: &value,
        });
        if hit.lock {
            app.lock(crate::engine::LockChange::Phones(pair, !locked));
        }
        if hit.down {
            app.act(Cmd::StepPhones { pair, louder: false });
        }
        if hit.up {
            app.act(Cmd::StepPhones { pair, louder: true });
        }
    });
}

fn home_card<R>(ui: &mut egui::Ui, title: &str, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let label = egui::Color32::from_rgb(116, 117, 135);
    // The reference stroke starts 12px down so the 16px title sits on that line, not on the view top.
    let stroke = egui::Rect::from_min_max(rect.min + egui::vec2(1.0, 12.0), rect.max - egui::vec2(1.0, 1.0));
    ui.painter().rect_stroke(stroke, 6.0, egui::Stroke::new(2.0, label), egui::StrokeKind::Inside);
    let galley = ui.painter().layout_no_wrap(title.to_owned(), egui::FontId::proportional(16.0), label);
    let title_pos = egui::pos2(rect.left() + 10.0, stroke.top() - galley.size().y * 0.42);
    let cover = egui::Rect::from_min_size(title_pos - egui::vec2(3.0, 0.0), galley.size() + egui::vec2(8.0, 2.0));
    ui.painter().rect_filled(cover, 0.0, super::CONTENT);
    ui.painter().galley(title_pos, galley, label);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
    add(&mut child)
}

fn knob_rect(card: egui::Rect, dx: f32, hearing: bool) -> egui::Rect {
    let height = if hearing { 100.0 } else { 75.0 };
    let cx = card.center().x + dx;
    egui::Rect::from_min_size(egui::pos2(cx - 34.0, card.top() + 115.0), egui::vec2(68.0, height))
}

fn meter_left(card: egui::Rect, knob: egui::Rect, count: i32, insert: bool) -> egui::Rect {
    let span = if count <= 1 { 10.0 } else { 22.0 };
    let left = if count <= 1 && insert {
        card.left() + ((knob.left() - card.left()) - 8.5) * 0.5
    } else if count <= 1 {
        card.left() + (knob.left() - card.left()) * 0.5
    } else {
        card.left() + ((knob.left() - card.left()) - span) * 0.5 + 3.0
    };
    egui::Rect::from_min_size(egui::pos2(left, card.top() + 25.0), egui::vec2(span, 150.0))
}

fn thick_arc(painter: &egui::Painter, center: egui::Pos2, radius: f32, fraction: f32) {
    let start = 135.0_f32.to_radians();
    let sweep = 270.0_f32.to_radians();
    let steps = 36;
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
    painter.add(egui::Shape::line(track, egui::Stroke::new(7.0, egui::Color32::from_rgb(35, 38, 44))));
    if hot.len() >= 2 {
        painter.add(egui::Shape::line(hot, egui::Stroke::new(7.0, BLUE)));
    }
}

const BLUE: egui::Color32 = egui::Color32::from_rgb(52, 141, 221);
const EQ: egui::Color32 = egui::Color32::from_rgb(116, 117, 135);
const VALUE: egui::Color32 = egui::Color32::from_rgb(185, 191, 217);
const GATE: egui::Color32 = egui::Color32::from_rgb(0, 207, 213);

fn paint_meters(ui: &mut egui::Ui, rect: egui::Rect, levels: &[Option<f32>]) {
    let mut x = rect.left();
    for level in levels {
        let bar = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(10.0, rect.height()));
        super::paint_level(ui.painter(), bar, *level);
        x += 12.0;
    }
}

fn named_font(name: &str, size: f32) -> egui::FontId {
    // Missing face falls back to the proportional font.
    egui::FontId {
        size,
        family: egui::FontFamily::Name(std::sync::Arc::from(name)),
    }
}

fn ui_font(size: f32) -> egui::FontId {
    // Light is the photographed weight of Electron's "14px Arial" buttons.
    named_font("NotoSans", size)
}

fn toggle(ui: &mut egui::Ui, id: &str, rect: egui::Rect, label: &str, on: bool, on_color: egui::Color32, font: f32) -> bool {
    let visual = rect.shrink(2.0);
    let edge = egui::Color32::from_rgb(158, 159, 160);
    if on {
        ui.painter().rect_filled(visual, 6.0, on_color);
        ui.painter().text(visual.center(), egui::Align2::CENTER_CENTER, label, ui_font(font), egui::Color32::BLACK);
    } else {
        // Canvas centers a 1px stroke on the path. Middle matches that outer edge.
        ui.painter().rect_stroke(visual, 6.0, egui::Stroke::new(1.0, edge), egui::StrokeKind::Middle);
        ui.painter().text(visual.center(), egui::Align2::CENTER_CENTER, label, ui_font(font), edge);
    }
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click()).clicked()
}

fn hearing_chip(ui: &mut egui::Ui, id: &str, rect: egui::Rect, label: &str, on: bool, dim: bool) -> bool {
    let amber = egui::Color32::from_rgb(239, 183, 28);
    let dim_c = egui::Color32::from_rgb(48, 48, 48);
    if dim {
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, dim_c), egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(9.0), dim_c);
    } else if on {
        ui.painter().rect_filled(rect, 2.0, amber);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(9.0), egui::Color32::BLACK);
    } else {
        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140));
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, GATE), egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(9.0), VALUE);
    }
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click()).clicked()
}

struct KnobFace<'a> {
    fraction: f32,
    locked: bool,
    all: bool,
    all_on: bool,
    steps: bool,
    quiet: bool,
    title: Option<(&'a str, &'a str)>,
    value: &'a str,
}

struct KnobHit {
    all: bool,
    lock: bool,
    down: bool,
    up: bool,
    arc: bool,
}

fn knob_face(ui: &mut egui::Ui, id: &str, rect: egui::Rect, face: KnobFace<'_>) -> KnobHit {
    let arc_center = egui::pos2(rect.left() + 34.0, rect.top() + 35.0);
    thick_arc(ui.painter(), arc_center, 27.0, face.fraction.clamp(0.0, 1.0));
    if let Some((a, b)) = face.title {
        ui.painter().text(arc_center + egui::vec2(0.0, -6.0), egui::Align2::CENTER_CENTER, a, egui::FontId::proportional(10.0), BLUE);
        ui.painter().text(arc_center + egui::vec2(0.0, 6.0), egui::Align2::CENTER_CENTER, b, egui::FontId::proportional(10.0), BLUE);
    }
    let mut hit = KnobHit { all: false, lock: false, down: false, up: false, arc: false };
    if face.all {
        let all_rect = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, rect.top() + 1.0), egui::vec2(26.0, 14.0));
        hit.all = hearing_chip(ui, &format!("{id}-all"), all_rect, "ALL", face.all_on, false);
    }
    let lock_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 20.0, rect.top() + 1.0), egui::vec2(18.0, 16.0));
    super::paint_padlock(ui.painter(), lock_rect, face.locked);
    hit.lock = ui.interact(lock_rect, ui.make_persistent_id((id, "lock")), egui::Sense::click()).clicked();
    let hearing = rect.height() > 90.0;
    let text_top = if hearing { rect.bottom() - 36.0 } else { rect.bottom() - 18.0 };
    ui.painter().text(
        egui::pos2(rect.center().x, text_top + 9.0),
        egui::Align2::CENTER_CENTER,
        face.value,
        egui::FontId::proportional(12.0),
        EQ,
    );
    if face.steps {
        let y = rect.bottom() - 18.0;
        let down = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, y), egui::vec2(30.0, 16.0));
        let up = egui::Rect::from_min_size(egui::pos2(rect.left() + 36.0, y), egui::vec2(30.0, 16.0));
        hit.down = hearing_chip(ui, &format!("{id}-down"), down, "−1", false, face.quiet);
        hit.up = hearing_chip(ui, &format!("{id}-up"), up, "+1", false, face.quiet);
    }
    let arc_rect = egui::Rect::from_center_size(arc_center, egui::vec2(44.0, 40.0));
    hit.arc = ui.interact(arc_rect, ui.make_persistent_id((id, "arc")), egui::Sense::click()).clicked();
    if hit.lock || hit.all || hit.down || hit.up {
        hit.arc = false;
    }
    hit
}

fn paint_step(painter: &egui::Painter, rect: egui::Rect, label: &str, dim: bool) {
    let stroke = if dim { egui::Color32::from_rgb(48, 52, 56) } else { CYAN };
    let text = if dim { DIM } else { CYAN };
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(16, 18, 20));
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.1, stroke), egui::StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), text);
}

pub(super) fn phones_source(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, pair: u8) {
    let slot_l = topology::phone_slot(pair as usize * 2);
    let slot_r = topology::phone_slot(pair as usize * 2 + 1);
    let have_l = snap.desk.patch.get(slot_l as usize).copied().unwrap_or(NOTHING);
    let have_r = snap.desk.patch.get(slot_r as usize).copied().unwrap_or(NOTHING);
    let current = source_name(have_l, have_r);
    let width = ui.available_width().clamp(80.0, 200.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::click());
    let edge = egui::Color32::from_rgb(20, 100, 155);
    let value = egui::Color32::from_rgb(158, 159, 160);
    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    ui.painter().text(rect.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, &current, egui::FontId::proportional(11.0), value);
    let tip = rect.right_center() + egui::vec2(-12.0, 1.0);
    let mark = egui::Stroke::new(1.2, value);
    ui.painter().line_segment([tip + egui::vec2(-4.0, -2.0), tip + egui::vec2(0.0, 2.0)], mark);
    ui.painter().line_segment([tip + egui::vec2(0.0, 2.0), tip + egui::vec2(4.0, -2.0)], mark);
    egui::Popup::menu(&response).show(|ui| {
        for (label, left, right) in phone_sources() {
            if ui.selectable_label(false, label).clicked() && (have_l, have_r) != (left, right) && app.writable(snap) {
                app.act_all(&[
                    Cmd::Patch { slot: slot_l, source: left },
                    Cmd::Patch { slot: slot_r, source: right },
                ]);
            }
        }
    });
}

fn phone_sources() -> [(&'static str, u8, u8); 9] {
    [
        ("Nothing", NOTHING, NOTHING),
        ("Main 1-2 Mix (L-R)", 76, 77),
        ("Line 3-4 Mix (L-R)", 78, 79),
        ("Line 5-6 Mix (L-R)", 80, 81),
        ("Line 7-8 Mix (L-R)", 82, 83),
        ("Line 9-10 Mix (L-R)", 84, 85),
        ("Phones 1 Mix (L-R)", 86, 87),
        ("Phones 2 Mix (L-R)", 88, 89),
        ("Reverb (L-R)", 90, 91),
    ]
}

fn source_name(left: u8, right: u8) -> String {
    for (label, l, r) in phone_sources() {
        if left == l && right == r {
            return label.to_string();
        }
    }
    if left >= NOTHING && right >= NOTHING {
        return "Nothing".into();
    }
    topology::source_label(left)
}

pub(super) fn step_pair(app: &mut DeskApp, ui: &mut egui::Ui, quiet: bool, step: impl Fn(bool) -> Cmd) {
    ui.horizontal(|ui| {
        for (label, louder) in [("−1", false), ("+1", true)] {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 22.0), egui::Sense::click());
            paint_step(ui.painter(), rect, label, quiet);
            if response.clicked() {
                app.act(step(louder));
            }
        }
    });
}

fn ab_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32) {
    let a = crate::gear::flag(snap.desk.gear.a_on);
    let b = crate::gear::flag(snap.desk.gear.b_on);
    let blocked = crate::gear::flag(snap.desk.gear.expander);
    home_card(ui, "A/B Monitoring", egui::vec2(width, 215.0), |ui| {
        let area = ui.max_rect();
        let inner = egui::Rect::from_min_max(egui::pos2(area.left(), area.top() + 15.0), egui::pos2(area.right() - 2.0, area.bottom()));
        let place = |cx: f32, cy: f32| egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(60.0, 30.0));
        let cy1 = inner.top() + 40.0;
        let cy2 = inner.top() + inner.height() * 0.5;
        let cy3 = inner.bottom() - 40.0;
        let inset = 55.0;
        let buttons = [
            ("ab-on", place(inner.center().x, cy1), "AB ON", a || b, super::AbButton::On),
            ("ab-a", place(inner.left() + inset, cy2), "A", a && !b, super::AbButton::A),
            ("ab-b", place(inner.right() - inset, cy2), "B", b && !a, super::AbButton::B),
            ("ab-both", place(inner.center().x, cy3), "BOTH", a && b, super::AbButton::Both),
        ];
        for (id, rect, label, on, which) in buttons {
            if toggle(ui, id, rect, label, on, BLUE, 14.0) {
                if blocked {
                    app.note("The optical expander turns A/B off.");
                } else if app.writable(snap) {
                    let (enable, a, b) = super::ab_next(a, b, which);
                    app.act(Cmd::Ab { enable, a, b });
                }
            }
        }
    });
}

fn talk_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32) {
    let gear = &snap.desk.gear;
    let enabled = crate::gear::flag(gear.talk_en);
    let latch = crate::gear::flag(gear.talk_latch);
    let source = gear.talk_src.unwrap_or(255);
    let level_db = crate::gear::db_of_linear(gear.talk_level.unwrap_or(0.0));
    let dim_db = crate::gear::db_of_linear(gear.talk_dim.unwrap_or(1.0));
    home_card(ui, "Talkback", egui::vec2(width, 215.0), |ui| {
        let area = ui.max_rect();
        if toggle(ui, "talk", egui::Rect::from_min_size(egui::pos2(area.left() + 30.0, area.top() + 40.0), egui::vec2(90.0, 30.0)), "TALK", enabled, BLUE, 14.0) && app.writable(snap) {
            app.act(Cmd::Talk { latch: None, enable: Some(!enabled), source: None, dest: None, level: None, dim: None });
        }
        if check_box(ui, "talk-latch", egui::pos2(area.left() + 50.0, area.top() + 75.0), "Latch", latch) && app.writable(snap) {
            app.act(Cmd::Talk { latch: Some(!latch), enable: None, source: None, dest: None, level: None, dim: None });
        }

        let menu = egui::Rect::from_min_size(egui::pos2(area.left() + 155.0, area.top() + 54.0), egui::vec2(172.0, 27.0));
        // The reference border is rgb(20,100,155). One screenshot pixel of that
        // stroke lands near rgb(22,80,118), which is the color drawn here.
        let edge = egui::Color32::from_rgb(22, 80, 118);
        let muted = egui::Color32::from_rgb(158, 159, 160);
        ui.painter().text(egui::pos2(menu.left(), menu.top() - 2.0), egui::Align2::LEFT_BOTTOM, "Source:", ui_font(14.0), EQ);
        ui.painter().rect_stroke(menu, 2.0, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
        ui.painter().text(menu.left_center() + egui::vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, talk_source(source), named_font("NotoSansBold", 11.0), muted);
        let response = ui.interact(menu, ui.make_persistent_id("talk-source"), egui::Sense::click());
        egui::Popup::menu(&response).show(|ui| {
            if ui.selectable_label(false, "None").clicked() && source != 255 && app.writable(snap) {
                app.act(Cmd::Talk { latch: None, enable: None, source: Some(255), dest: None, level: None, dim: None });
            }
            for index in 0..10u8 {
                let label = topology::input_name(index as usize);
                if ui.selectable_label(false, label).clicked() && source != index && app.writable(snap) {
                    app.act(Cmd::Talk { latch: None, enable: None, source: Some(index), dest: None, level: None, dim: None });
                }
            }
        });

        let level_rect = egui::Rect::from_min_size(egui::pos2(area.left() + 165.0, area.top() + 100.0), egui::vec2(68.0, 75.0));
        let dim_rect = egui::Rect::from_min_size(egui::pos2(area.left() + 255.0, area.top() + 100.0), egui::vec2(68.0, 75.0));
        let level_hit = talk_knob(ui, "talk-level", level_rect, (level_db + 48.0) / 60.0, level_db <= -47.5, level_db, "Level");
        let dim_hit = talk_knob(ui, "talk-dim", dim_rect, (dim_db + 48.0) / 48.0, dim_db <= -47.5, dim_db, "Dim");
        if level_hit.1 || dim_hit.1 {
            app.note(UNAVAILABLE);
        } else if app.writable(snap) {
            if level_hit.0 {
                let next = if super::arc_louder(ui, level_rect.top() + 35.0) { level_db + 1.0 } else { level_db - 1.0 };
                app.act(Cmd::Talk { latch: None, enable: None, source: None, dest: None, level: Some(next.clamp(-48.0, 12.0)), dim: None });
            } else if dim_hit.0 {
                let next = if super::arc_louder(ui, dim_rect.top() + 35.0) { dim_db + 1.0 } else { dim_db - 1.0 };
                app.act(Cmd::Talk { latch: None, enable: None, source: None, dest: None, level: None, dim: Some(next.clamp(-48.0, 0.0)) });
            }
        }

        let dest = egui::Rect::from_min_size(egui::pos2(area.left() + 360.0, area.top() + 54.0), egui::vec2(172.0, 140.0));
        ui.painter().text(egui::pos2(dest.left() + 3.0, dest.top() - 2.0), egui::Align2::LEFT_BOTTOM, "Destinations:", ui_font(14.0), EQ);
        ui.painter().rect_stroke(dest, 2.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(22, 80, 118)), egui::StrokeKind::Inside);
        let names = [
            ("Main Out 1-2", 0i32),
            ("Phones Out 1", 10),
            ("Phones Out 2", 12),
            ("Line Out 3-4", 2),
            ("Line Out 5-6", 4),
            ("Line Out 7-8", 6),
            ("Line Out 9-10", 8),
        ];
        let mask = gear.talk_dest.unwrap_or(0);
        let mut y = dest.top() + 15.0;
        for (index, (name, bit)) in names.iter().enumerate() {
            if y + 12.0 > dest.bottom() - 4.0 {
                break;
            }
            let flag = 1i32 << (*bit as u32);
            let on = (mask & flag) != 0;
            if check_box(ui, &format!("talk-dest-{index}"), egui::pos2(dest.left() + 10.0, y), name, on) && app.writable(snap) {
                let pair = flag | (1i32 << (*bit as u32 + 1));
                let next = if on { mask & !pair } else { mask | pair };
                app.act(Cmd::Talk { latch: None, enable: None, source: None, dest: Some(next), level: None, dim: None });
            }
            y += 33.0;
        }
    });
}

fn talk_source(source: u8) -> &'static str {
    if source == 255 {
        "None"
    } else {
        topology::input_name(source as usize)
    }
}

fn talk_knob(ui: &mut egui::Ui, id: &str, rect: egui::Rect, fraction: f32, silent: bool, db: f32, label: &str) -> (bool, bool) {
    let center = egui::pos2(rect.left() + 34.0, rect.top() + 35.0);
    thick_arc(ui.painter(), center, 27.0, fraction.clamp(0.0, 1.0));
    let lock = egui::Rect::from_min_size(egui::pos2(rect.right() - 20.0, rect.top() + 1.0), egui::vec2(18.0, 16.0));
    ui.painter().rect_filled(lock, 3.0, egui::Color32::from_rgb(29, 78, 52));
    let body = egui::Rect::from_center_size(lock.center() + egui::vec2(0.0, 2.2), egui::vec2(7.0, 5.0));
    let ink = egui::Color32::from_rgb(210, 214, 208);
    ui.painter().rect_filled(body, 1.0, ink);
    let hinge = lock.center() + egui::vec2(1.2, -2.4);
    let mut points = Vec::with_capacity(6);
    for index in 0..=5 {
        let angle = 230.0_f32.to_radians() + index as f32 * (170.0_f32.to_radians() / 5.0);
        points.push(hinge + egui::vec2(angle.cos(), angle.sin()) * 2.8);
    }
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.2, ink)));
    let readout = egui::pos2(rect.center().x, rect.bottom() - 9.0);
    if silent {
        paint_silent_db(ui.painter(), readout);
    } else {
        ui.painter().text(readout, egui::Align2::CENTER_CENTER, format!("{db:.0} dB"), named_font("NotoSansReg", 12.0), EQ);
    }
    // "800 13px Arial" resolves to Noto Sans Black on this machine.
    ui.painter().text(egui::pos2(rect.center().x, rect.bottom() + 14.0), egui::Align2::CENTER_TOP, label, named_font("NotoSansBlack", 13.0), EQ);
    let lock_hit = ui.interact(lock, ui.make_persistent_id((id, "lock")), egui::Sense::click()).clicked();
    let arc_hit = ui.interact(egui::Rect::from_center_size(center, egui::vec2(44.0, 40.0)), ui.make_persistent_id((id, "arc")), egui::Sense::click()).clicked();
    (arc_hit && !lock_hit, lock_hit)
}

fn paint_silent_db(painter: &egui::Painter, center: egui::Pos2) {
    // Noto Sans Regular has no U+221E, so a text infinity paints as a square.
    // The hyphen and the unit stay on the same face as the dim readout's "0 dB".
    let font = named_font("NotoSansReg", 12.0);
    let minus = painter.layout_no_wrap("-".to_owned(), font.clone(), EQ);
    let unit = painter.layout_no_wrap(" dB".to_owned(), font, EQ);
    // Advance of that infinity at 12px, side bearings included.
    let mark_w = 16.0;
    let minus_w = minus.size().x;
    let width = minus_w + mark_w + unit.size().x;
    let left = center.x - width * 0.5;
    let top = center.y - minus.size().y * 0.5;
    let axis_y = minus.rows.first().map(|row| {
        let bounds = row.visuals.mesh_bounds;
        if bounds.width() > 0.0 {
            top + row.pos.y + bounds.center().y
        } else {
            center.y
        }
    }).unwrap_or(center.y);
    painter.galley(egui::pos2(left, top), minus, EQ);
    paint_infinity(painter, egui::pos2(left + minus_w + mark_w * 0.5, axis_y));
    painter.galley(egui::pos2(left + minus_w + mark_w, top), unit, EQ);
}

fn paint_infinity(painter: &egui::Painter, center: egui::Pos2) {
    // Ink of the reference infinity at 12px is 15.0 by 6.4. The stroke is
    // centered on the path, so the path is one stroke-width smaller.
    let stroke = 1.12;
    let x_scale = (15.0 - stroke) * 0.5;
    let y_scale = (6.37 - stroke) * 0.5 / 0.35355339;
    let mut points = Vec::with_capacity(64);
    for index in 0..64 {
        let t = index as f32 * std::f32::consts::TAU / 64.0;
        let s = t.sin();
        let denom = 1.0 + s * s;
        points.push(center + egui::vec2(t.cos() / denom * x_scale, s * t.cos() / denom * y_scale));
    }
    painter.add(egui::Shape::closed_line(points, egui::Stroke::new(stroke, EQ)));
}

fn check_box(ui: &mut egui::Ui, id: &str, at: egui::Pos2, label: &str, on: bool) -> bool {
    let box_rect = egui::Rect::from_min_size(at, egui::vec2(12.0, 12.0));
    let ink = egui::Color32::from_rgb(116, 118, 134);
    if on {
        ui.painter().rect_filled(box_rect, 2.0, egui::Color32::from_rgb(33, 150, 243));
        let a = box_rect.min + egui::vec2(2.4, 6.0);
        let b = box_rect.min + egui::vec2(5.0, 9.2);
        let c = box_rect.min + egui::vec2(9.6, 3.2);
        let mark = egui::Stroke::new(1.6, WHITE);
        ui.painter().line_segment([a, b], mark);
        ui.painter().line_segment([b, c], mark);
    } else {
        ui.painter().rect_stroke(box_rect, 2.0, egui::Stroke::new(1.0, ink), egui::StrokeKind::Inside);
    }
    let text = ui.painter().layout_no_wrap(label.to_owned(), named_font("NotoSansReg", 13.0), ink);
    let text_pos = egui::pos2(box_rect.right() + 5.0, box_rect.center().y - text.size().y * 0.5);
    ui.painter().galley(text_pos, text.clone(), ink);
    let hit = egui::Rect::from_min_size(box_rect.min, egui::vec2(12.0 + 5.0 + text.size().x, 16.0));
    ui.interact(hit, ui.make_persistent_id(id), egui::Sense::click()).clicked()
}

fn input_monitor(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    // Two phones rows make this block 282 tall, which sets the scroll thumb.
    // Buttons stay off. A click does not write a mix fader.
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        home_card(ui, "Input Monitoring", egui::vec2(760.0, 282.0), |ui| {
            let area = ui.max_rect();
            paint_speaker(ui.painter(), egui::pos2(area.left() + 45.0, area.top() + 48.0));
            paint_phones(ui.painter(), egui::pos2(area.left() + 38.0, area.top() + 112.0));
            paint_phones(ui.painter(), egui::pos2(area.left() + 38.0, area.top() + 167.0));

            let btn = 25.0;
            let pitch = 49.0;
            let origin = area.left() + 220.0;
            let lines_left = origin + 2.0 * pitch;
            let lines_right = origin + 9.0 * pitch + btn;
            let rule = egui::Stroke::new(1.0, EQ);
            ui.painter().line_segment([egui::pos2(lines_left + 1.0, area.top() + 44.0), egui::pos2(lines_right, area.top() + 44.0)], rule);
            ui.painter().line_segment([egui::pos2(area.left() + 40.0, area.top() + 104.0), egui::pos2(lines_right, area.top() + 104.0)], rule);
            ui.painter().line_segment([egui::pos2(area.left() + 40.0, area.top() + 159.0), egui::pos2(lines_right, area.top() + 159.0)], rule);

            let mic_center = origin + (pitch + btn) * 0.5;
            let line_center = (lines_left + lines_right) * 0.5;
            cover_label(ui.painter(), "Mic/Line/Inst", egui::pos2(mic_center, area.top() + 50.0));
            cover_label(ui.painter(), "Line In", egui::pos2(line_center, area.top() + 50.0));

            for (row, title, baseline, top, bus) in [
                (0_u8, "Main Out 1-2", 81.0, 64.0, 0_u16),
                (1, "Phones 1", 137.0, 119.0, 10),
                (2, "Phones 2", 192.0, 174.0, 12),
            ] {
                ui.painter().text(
                    egui::pos2(area.left() + 94.0, area.top() + baseline),
                    egui::Align2::LEFT_BOTTOM,
                    title,
                    ui_font(14.0),
                    EQ,
                );
                for number in 1..=10u16 {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(origin + (number - 1) as f32 * pitch, area.top() + top),
                        egui::vec2(btn, btn),
                    );
                    let state = monitor_state(snap, bus, number - 1);
                    let (on, color) = match state {
                        Monitor::On => (true, BLUE),
                        Monitor::Edited => (true, EQ),
                        Monitor::Off | Monitor::Disabled => (false, BLUE),
                    };
                    if toggle(ui, &format!("im-{row}-{number}"), rect, &number.to_string(), on, color, 14.0) {
                        if state == Monitor::Disabled {
                            app.note("That input is the talkback source.");
                        } else if app.writable(snap) {
                            set_monitor(app, bus, number - 1, state != Monitor::On);
                        }
                    }
                }
            }

            let info = egui::pos2(area.right() - 20.0, area.top() + 35.0);
            ui.painter().circle_stroke(info, 10.0, egui::Stroke::new(2.2, EQ));
            ui.painter().text(info + egui::vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(15.0), EQ);
            let info_hit = egui::Rect::from_center_size(info, egui::vec2(24.0, 24.0));
            if ui.interact(info_hit, ui.make_persistent_id("im-info"), egui::Sense::click()).clicked() {
                app.im_info = !app.im_info;
            }

            let alert = egui::pos2(area.left() + 238.0, area.bottom() - 68.0);
            paint_alert(ui.painter(), alert);
            let note = egui::Color32::from_rgb(116, 117, 135);
            ui.painter().text(
                egui::pos2(alert.x + 52.0, area.bottom() - 54.0),
                egui::Align2::LEFT_BOTTOM,
                "Click an input to hear it directly on the Main Mix or Phones Mix.",
                named_font("NotoSansBold", 11.0),
                note,
            );
            ui.painter().text(
                egui::pos2(alert.x + 52.0, area.bottom() - 37.0),
                egui::Align2::LEFT_BOTTOM,
                "When doing so, turn off input monitoring in your host software.",
                named_font("NotoSansBold", 11.0),
                note,
            );

            if app.im_info {
                let panel = egui::Rect::from_min_max(area.min + egui::vec2(14.0, 24.0), area.max - egui::vec2(10.0, 8.0));
                ui.painter().rect_filled(panel, 6.0, egui::Color32::BLACK);
                ui.painter().rect_stroke(panel, 6.0, egui::Stroke::new(1.5, EQ), egui::StrokeKind::Inside);
                let close = egui::Rect::from_min_size(egui::pos2(area.right() - 40.0, area.top() + 30.0), egui::vec2(16.0, 16.0));
                ui.painter().line_segment([close.left_top(), close.right_bottom()], egui::Stroke::new(1.4, EQ));
                ui.painter().line_segment([close.right_top(), close.left_bottom()], egui::Stroke::new(1.4, EQ));
                if ui.interact(close, ui.make_persistent_id("im-info-x"), egui::Sense::click()).clicked() {
                    app.im_info = false;
                }
            }
        });
    });
}

fn cover_label(painter: &egui::Painter, text: &str, baseline: egui::Pos2) {
    let galley = painter.layout_no_wrap(text.to_owned(), ui_font(14.0), EQ);
    let pos = egui::pos2(baseline.x - galley.size().x * 0.5, baseline.y - galley.size().y);
    let cover = egui::Rect::from_min_size(pos - egui::vec2(6.0, 0.0), galley.size() + egui::vec2(12.0, 0.0));
    painter.rect_filled(cover, 0.0, super::CONTENT);
    painter.galley(pos, galley, EQ);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Monitor {
    Off,
    On,
    Edited,
    Disabled,
}

fn monitor_state(snap: &Snap, bus: u16, channel: u16) -> Monitor {
    if snap.desk.gear.talk_src == Some(channel as u8) {
        return Monitor::Disabled;
    }
    let Some(index) = topology::cell_index(bus, channel) else { return Monitor::Off };
    let fader = snap.desk.faders.as_ref().and_then(|faders| faders.get(index).copied()).unwrap_or(0.0);
    if fader == 0.0 {
        return Monitor::Off;
    }
    let stereo = snap.desk.in_link.get((channel & !1) as usize).copied().unwrap_or(0) != 0;
    let mute = snap.desk.mutes.get(index).copied().unwrap_or(0);
    let pan = snap.desk.pans.get(index).copied().unwrap_or(0.5);
    let rounded = (fader * 100.0).round() / 100.0;
    if !stereo && mute == 0 && (pan - 0.5).abs() < 0.001 && (rounded - 1.0).abs() < 0.001 {
        Monitor::On
    } else {
        Monitor::Edited
    }
}

fn set_monitor(app: &mut DeskApp, bus: u16, channel: u16, enable: bool) {
    if !enable {
        app.act(Cmd::Mix { bus, input: channel, gain: Some(0.0), db: None, pan: None, mute: None, solo: None });
        return;
    }
    app.act_all(&[
        Cmd::Mix { bus, input: channel, gain: Some(1.0), db: None, pan: Some(0.5), mute: Some(false), solo: Some(false) },
        Cmd::InputLink { input: channel & !1, linked: false },
    ]);
}

fn paint_speaker(painter: &egui::Painter, at: egui::Pos2) {
    let color = egui::Color32::from_rgb(91, 93, 107);
    let body = egui::Rect::from_min_size(at + egui::vec2(4.0, 3.0), egui::vec2(20.0, 36.0));
    painter.rect_filled(body, 2.5, color);
    painter.circle_filled(body.center() + egui::vec2(0.0, -8.0), 4.0, super::CONTENT);
    painter.circle_filled(body.center() + egui::vec2(0.0, 8.0), 6.2, super::CONTENT);
}

fn paint_phones(painter: &egui::Painter, at: egui::Pos2) {
    let color = egui::Color32::from_rgb(91, 93, 107);
    let center = at + egui::vec2(21.0, 16.0);
    let mut band = Vec::with_capacity(13);
    for index in 0..=12 {
        let angle = std::f32::consts::PI * (1.0 + index as f32 / 12.0);
        band.push(center + egui::vec2(angle.cos(), angle.sin()) * 12.0);
    }
    painter.add(egui::Shape::line(band, egui::Stroke::new(3.0, color)));
    painter.rect_filled(egui::Rect::from_center_size(at + egui::vec2(9.0, 18.0), egui::vec2(8.0, 14.0)), 2.0, color);
    painter.rect_filled(egui::Rect::from_center_size(at + egui::vec2(33.0, 18.0), egui::vec2(8.0, 14.0)), 2.0, color);
}

fn paint_alert(painter: &egui::Painter, at: egui::Pos2) {
    let color = egui::Color32::from_rgb(91, 93, 107);
    let top = at + egui::vec2(21.0, 6.0);
    let left = at + egui::vec2(6.0, 32.0);
    let right = at + egui::vec2(36.0, 32.0);
    painter.add(egui::Shape::closed_line(vec![top, right, left], egui::Stroke::new(1.6, color)));
    painter.line_segment([at + egui::vec2(21.0, 14.0), at + egui::vec2(21.0, 23.0)], egui::Stroke::new(1.8, color));
    painter.circle_filled(at + egui::vec2(21.0, 27.5), 1.3, color);
}

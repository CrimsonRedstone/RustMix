//! Desktop volume. Horizontal rows for outputs, programs, inputs, and recordings.
//! A change is pactl volume or mute. Nothing here moves a stream or writes an 828 trim.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use super::{DeskApp, Page, CARD, CYAN, DIM, WARN, WHITE};
use crate::pipewire::{self, Level, LevelKind, Levels, PactlRunner, Runner};

const MUTED: egui::Color32 = egui::Color32::from_rgb(207, 31, 37);
const SLIDER: egui::Color32 = egui::Color32::from_rgb(52, 141, 221);
const TRACK: egui::Color32 = egui::Color32::from_rgb(18, 20, 22);

pub(crate) struct Audio {
    levels: Option<Levels>,
    error: String,
    fetch_rx: Option<Receiver<(u64, Result<Levels, String>)>>,
    fetching: bool,
    next: Instant,
    epoch: u64,
    pending: u32,
    drag: Option<Drag>,
    write_tx: Sender<Result<(), String>>,
    write_rx: Receiver<Result<(), String>>,
}

#[derive(Clone)]
struct Drag {
    key: String,
    percent: u8,
}

enum Act {
    Preview(String, u8),
    Volume(Level, u8),
    Mute(Level),
}

impl Audio {
    pub(crate) fn new() -> Self {
        let (write_tx, write_rx) = mpsc::channel();
        Self {
            levels: None,
            error: String::new(),
            fetch_rx: None,
            fetching: false,
            next: Instant::now(),
            epoch: 0,
            pending: 0,
            drag: None,
            write_tx,
            write_rx,
        }
    }

    fn take_results(&mut self) {
        while let Ok(result) = self.write_rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            if let Err(err) = result {
                self.error = err;
            }
            if self.pending == 0 {
                self.next = Instant::now() + Duration::from_millis(250);
            }
        }
        let Some(rx) = &self.fetch_rx else { return };
        let Ok((epoch, result)) = rx.try_recv() else { return };
        self.fetching = false;
        self.fetch_rx = None;
        if epoch != self.epoch || self.drag.is_some() || self.pending > 0 {
            return;
        }
        match result {
            Ok(levels) => {
                self.levels = Some(levels);
                self.error.clear();
            }
            Err(err) => self.error = err,
        }
    }

    fn start_fetch(&mut self) {
        let epoch = self.epoch;
        let (tx, rx) = mpsc::channel();
        self.fetch_rx = Some(rx);
        self.fetching = true;
        self.next = Instant::now() + Duration::from_millis(700);
        thread::spawn(move || {
            let result = pipewire::read_levels(&PactlRunner);
            let _ = tx.send((epoch, result));
        });
    }

    fn preview(&mut self, key: &str, percent: u8) {
        self.drag = Some(Drag { key: key.to_string(), percent });
    }

    fn commit_volume(&mut self, node: &Level, percent: u8) {
        let percent = percent.min(100);
        self.drag = None;
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = self.pending.saturating_add(1);
        self.touch(&node.key(), |row| {
            row.percent = percent as u32;
        });
        enqueue(pipewire::volume_args(node.kind, node.index, percent), self.write_tx.clone());
    }

    fn commit_mute(&mut self, node: &Level) {
        let muted = !node.muted;
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = self.pending.saturating_add(1);
        self.touch(&node.key(), |row| row.muted = muted);
        enqueue(pipewire::mute_args(node.kind, node.index, muted), self.write_tx.clone());
    }

    fn touch(&mut self, key: &str, edit: impl FnOnce(&mut Level)) {
        let Some(levels) = &mut self.levels else { return };
        for row in levels
            .outputs
            .iter_mut()
            .chain(levels.playback.iter_mut())
            .chain(levels.inputs.iter_mut())
            .chain(levels.recording.iter_mut())
        {
            if row.key() == key {
                edit(row);
                return;
            }
        }
    }
}

pub(crate) fn service(app: &mut DeskApp, ctx: &egui::Context) {
    app.pc_audio.take_results();
    if app.page != Page::Pc {
        return;
    }
    let due = !app.pc_audio.fetching && app.pc_audio.drag.is_none() && app.pc_audio.pending == 0 && Instant::now() >= app.pc_audio.next;
    if due {
        app.pc_audio.start_fetch();
    }
    let wait = if app.pc_audio.fetching { 40 } else { 200 };
    ctx.request_repaint_after(Duration::from_millis(wait));
}

pub(crate) fn show(app: &mut DeskApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("PC MIXER").font(face("NotoSansBold", 20.0)).color(WHITE));
    ui.label(
        egui::RichText::new("Volume for this PC. Devices and programs. It does not move a route, and it does not change the 828 monitor, phones, or output trim.")
            .font(face("NotoSans", 12.5))
            .color(DIM),
    );
    if !app.pc_audio.error.is_empty() {
        ui.label(egui::RichText::new(&app.pc_audio.error).font(face("NotoSans", 12.5)).color(WARN));
    }
    ui.add_space(6.0);
    let levels = app.pc_audio.levels.clone();
    let drag = app.pc_audio.drag.clone();
    let mut acts = Vec::new();
    egui::ScrollArea::vertical().id_salt("pc-mix").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        match &levels {
            None => {
                ui.label(egui::RichText::new("Reading desktop volume.").font(face("NotoSans", 13.0)).color(DIM));
            }
            Some(levels) => {
                section(ui, "OUTPUT DEVICES", &levels.outputs, &drag, "No output device.", &mut acts);
                section(ui, "APPLICATIONS", &levels.playback, &drag, "Nothing is playing.", &mut acts);
                section(ui, "INPUT DEVICES", &levels.inputs, &drag, "No input device.", &mut acts);
                section(ui, "RECORDING", &levels.recording, &drag, "Nothing is recording.", &mut acts);
            }
        }
    });
    for act in acts {
        match act {
            Act::Preview(key, percent) => app.pc_audio.preview(&key, percent),
            Act::Volume(node, percent) => app.pc_audio.commit_volume(&node, percent),
            Act::Mute(node) => app.pc_audio.commit_mute(&node),
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str, rows: &[Level], drag: &Option<Drag>, empty: &str, acts: &mut Vec<Act>) {
    ui.label(egui::RichText::new(title).font(face("NotoSansBold", 11.0)).color(DIM));
    if rows.is_empty() {
        ui.label(egui::RichText::new(empty).font(face("NotoSans", 12.5)).color(DIM));
        return;
    }
    for node in rows {
        if let Some(act) = row(ui, node, drag) {
            acts.push(act);
        }
    }
}

fn row(ui: &mut egui::Ui, node: &Level, drag: &Option<Drag>) -> Option<Act> {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 6.0, CARD);
    let stored = shown_percent(node, drag);
    let name_w = name_width(rect.width());
    let mute_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 70.0, rect.center().y - 11.0), egui::vec2(58.0, 22.0));
    let pct_rect = egui::Rect::from_min_max(egui::pos2(mute_rect.left() - 52.0, rect.top()), egui::pos2(mute_rect.left() - 6.0, rect.bottom()));
    let slider_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 36.0 + name_w, rect.top() + 10.0),
        egui::pos2(pct_rect.left() - 6.0, rect.bottom() - 10.0),
    );
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 36.0, rect.top() + 6.0),
        egui::pos2(rect.left() + 32.0 + name_w, rect.bottom() - 6.0),
    );
    let mark = rect.left_center() + egui::vec2(18.0, 0.0);
    if matches!(node.kind, LevelKind::Source | LevelKind::Capture) {
        draw_mic(ui.painter(), mark, WHITE);
    } else {
        draw_speaker(ui.painter(), mark, WHITE);
    }
    let name_painter = ui.painter().with_clip_rect(name_rect);
    let title_y = if node.detail.is_empty() && !node.default_node { name_rect.center().y } else { name_rect.top() + 10.0 };
    name_painter.text(egui::pos2(name_rect.left(), title_y), egui::Align2::LEFT_CENTER, &node.title, face("NotoSans", 14.0), WHITE);
    let mut meta = node.detail.clone();
    if node.default_node {
        if !meta.is_empty() {
            meta.push_str("  ·  ");
        }
        meta.push_str("DEFAULT");
    }
    if !meta.is_empty() {
        name_painter.text(
            egui::pos2(name_rect.left(), name_rect.bottom() - 9.0),
            egui::Align2::LEFT_CENTER,
            meta,
            face("NotoSans", 11.0),
            if node.default_node { CYAN } else { DIM },
        );
    }
    let slide = slider(ui, &node.key(), slider_rect, stored);
    let label = match slide {
        Some(Slide::Preview(percent) | Slide::Commit(percent)) => format!("{percent}%"),
        None => format!("{stored}%"),
    };
    ui.painter().text(pct_rect.right_center(), egui::Align2::RIGHT_CENTER, label, face("NotoSans", 13.0), WHITE);
    let mute_id = ui.make_persistent_id(("pc-mute", node.key()));
    let mute = ui.interact(mute_rect, mute_id, egui::Sense::click());
    if mute.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if node.muted {
        ui.painter().rect_filled(mute_rect, 4.0, MUTED);
    } else {
        ui.painter().rect_stroke(mute_rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(150, 156, 164)), egui::StrokeKind::Inside);
    }
    ui.painter().text(mute_rect.center(), egui::Align2::CENTER_CENTER, "MUTE", face("NotoSansBold", 11.0), WHITE);
    if mute.clicked() {
        return Some(Act::Mute(node.clone()));
    }
    match slide {
        Some(Slide::Preview(percent)) => Some(Act::Preview(node.key(), percent)),
        Some(Slide::Commit(percent)) => Some(Act::Volume(node.clone(), percent)),
        None => None,
    }
}

#[derive(Clone, Copy)]
enum Slide {
    Preview(u8),
    Commit(u8),
}

fn shown_percent(node: &Level, drag: &Option<Drag>) -> u32 {
    if let Some(drag) = drag {
        if drag.key == node.key() {
            return drag.percent as u32;
        }
    }
    node.percent
}

fn name_width(row: f32) -> f32 {
    let room = row - 36.0 - 52.0 - 70.0 - 24.0;
    (room * 0.42).clamp(120.0, 220.0)
}

fn slider(ui: &mut egui::Ui, key: &str, rect: egui::Rect, stored: u32) -> Option<Slide> {
    if rect.width() < 24.0 {
        return None;
    }
    let id = ui.make_persistent_id(("pc-slider", key));
    let response = ui.interact(rect, id, egui::Sense::click_and_drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let fallback = stored.min(100) as u8;
    let live = pointer_percent(rect, &response);
    let shown = if response.double_clicked() {
        100
    } else if response.dragged() {
        live.unwrap_or(fallback)
    } else {
        fallback
    };
    paint_slider(ui.painter(), rect, shown);
    if response.double_clicked() {
        return Some(Slide::Commit(100));
    }
    if response.dragged() {
        return Some(Slide::Preview(shown));
    }
    if response.drag_stopped() || response.clicked() {
        return Some(Slide::Commit(live.unwrap_or(fallback)));
    }
    None
}

fn pointer_percent(rect: egui::Rect, response: &egui::Response) -> Option<u8> {
    let pos = response.interact_pointer_pos()?;
    let t = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    Some((t * 100.0).round() as u8)
}

fn paint_slider(painter: &egui::Painter, rect: egui::Rect, percent: u8) {
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 6.0));
    painter.rect_filled(track, 3.0, TRACK);
    let fill_w = track.width() * (percent as f32 / 100.0);
    if fill_w > 0.5 {
        painter.rect_filled(egui::Rect::from_min_size(track.left_top(), egui::vec2(fill_w, track.height())), 3.0, SLIDER);
    }
    let thumb = egui::pos2(track.left() + fill_w, track.center().y);
    painter.circle_filled(thumb, 7.0, WHITE);
    painter.circle_stroke(thumb, 7.0, egui::Stroke::new(1.0, SLIDER));
}

pub(crate) fn draw_speaker(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let body = egui::Rect::from_center_size(center + egui::vec2(-4.0, 0.0), egui::vec2(3.5, 6.0));
    painter.rect_filled(body, 0.6, color);
    let cone = vec![
        center + egui::vec2(-2.2, -3.0),
        center + egui::vec2(1.6, -6.2),
        center + egui::vec2(1.6, 6.2),
        center + egui::vec2(-2.2, 3.0),
    ];
    painter.add(egui::Shape::convex_polygon(cone, color, egui::Stroke::NONE));
    let stroke = egui::Stroke::new(1.3, color);
    for radius in [4.6_f32, 7.4] {
        let origin = center + egui::vec2(2.0, 0.0);
        let points = (0..5)
            .map(|step| {
                let angle = -0.85 + step as f32 * 1.7 / 4.0;
                origin + egui::vec2(angle.cos() * radius, angle.sin() * radius)
            })
            .collect();
        painter.add(egui::Shape::line(points, stroke));
    }
}

fn draw_mic(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.4, color);
    painter.circle_stroke(center + egui::vec2(0.0, -2.0), 3.2, stroke);
    painter.line_segment([center + egui::vec2(0.0, 1.2), center + egui::vec2(0.0, 5.5)], stroke);
    painter.line_segment([center + egui::vec2(-3.2, 5.5), center + egui::vec2(3.2, 5.5)], stroke);
    let points = (0..4)
        .map(|step| {
            let angle = 0.4 + step as f32 * 2.3 / 3.0;
            center + egui::vec2(angle.cos() * 5.5, -2.0 + angle.sin() * 5.5)
        })
        .collect();
    painter.add(egui::Shape::line(points, stroke));
}

fn face(name: &str, size: f32) -> egui::FontId {
    egui::FontId { size, family: egui::FontFamily::Name(name.into()) }
}

fn enqueue(args: Vec<String>, report: Sender<Result<(), String>>) {
    static START: OnceLock<Sender<(Vec<String>, Sender<Result<(), String>>)>> = OnceLock::new();
    let tx = START.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<(Vec<String>, Sender<Result<(), String>>)>();
        thread::spawn(move || {
            while let Ok((args, report)) = rx.recv() {
                let _ = report.send(apply_level(&args));
            }
        });
        tx
    });
    let _ = tx.send((args, report));
}

fn apply_level(args: &[String]) -> Result<(), String> {
    let out = PactlRunner.run(args)?;
    if out.code != 0 {
        let message = out.stderr.trim();
        if message.is_empty() {
            return Err("PipeWire did not take that volume.".into());
        }
        return Err(message.chars().take(160).collect());
    }
    Ok(())
}

//! Device, input, output, control, and discovery.
//! Level locks are listed here without a decibel value.
//! Monitor and phones move one stored step per click. Output trim is a lock only.

use eframe::egui;

use super::{card, chip, device_title, link_line, monitor_text, paint_padlock, pair_text, stored_fraction, DeskApp, Snap, CONTENT, DIM, UNAVAILABLE, WHITE};
use crate::engine::LockChange;
use crate::plan::Cmd;
use crate::topology::{self, NOTHING};

const MENU_EDGE: egui::Color32 = egui::Color32::from_rgb(20, 100, 155);
const MENU_VALUE: egui::Color32 = egui::Color32::from_rgb(158, 159, 160);

pub(crate) fn device(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    device_title(ui, app.wordmark.as_ref(), super::unit_open(snap));
    egui::ScrollArea::vertical().id_salt("device").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(2.0);
        let gear = &snap.desk.gear;
        let rate = if gear.sample_rate == 0 { "—".into() } else { gear.sample_rate.to_string() };
        pick_menu(app, ui, snap, "Sample Rate:", &rate, false, &[
            ("44100", Cmd::SampleRate { rate: 44_100 }),
            ("48000", Cmd::SampleRate { rate: 48_000 }),
            ("88200", Cmd::SampleRate { rate: 88_200 }),
            ("96000", Cmd::SampleRate { rate: 96_000 }),
            ("176400", Cmd::SampleRate { rate: 176_400 }),
            ("192000", Cmd::SampleRate { rate: 192_000 }),
        ]);
        pick_menu(app, ui, snap, "Clock Source:", &clock_label(gear.clock), false, &[
            ("Internal", Cmd::Clock { source: 0 }),
            ("Word Clock", Cmd::Clock { source: 5 }),
            ("S/PDIF", Cmd::Clock { source: 8 }),
            ("Optical A", Cmd::Clock { source: 12 }),
            ("Optical B", Cmd::Clock { source: 13 }),
        ]);
        pick_menu(app, ui, snap, "Loopback Location:", &loop_place(gear.loop_first), true, &[
            ("USB In 11-12 (Default)", Cmd::LoopFirst { which: 0 }),
            ("USB In 1-2", Cmd::LoopFirst { which: 1 }),
        ]);
        let meters = if app.meter_post { "Post-Fader" } else { "Pre-Fader" };
        let response = menu_row(ui, "Mix Input Meters:", meters, false);
        egui::Popup::menu(&response).show(|ui| {
            if ui.selectable_label(false, "Pre-Fader").clicked() {
                app.meter_post = false;
            }
            if ui.selectable_label(false, "Post-Fader").clicked() {
                app.meter_post = true;
            }
        });
        pick_menu(app, ui, snap, "Mix USB Channels:", &host_label(gear.host_mix), false, &[
            ("Default", Cmd::HostMix { mode: 0 }),
            ("Additional", Cmd::HostMix { mode: 1 }),
        ]);
        pick_menu(app, ui, snap, "MIDI Thru:", &on_label(gear.midi), false, &[
            ("Disabled", Cmd::MidiThru { on: false }),
            ("Enabled", Cmd::MidiThru { on: true }),
        ]);
        pick_menu(app, ui, snap, "Word Clock:", &word_label(gear.word), false, &[
            ("Out", Cmd::WordOut { out: true }),
            ("Thru", Cmd::WordOut { out: false }),
        ]);
        pick_menu(app, ui, snap, "Optical Expander:", &on_label(gear.expander), false, &[
            ("Disabled", Cmd::Expander { on: false }),
            ("Enabled", Cmd::Expander { on: true }),
        ]);
        name_row(app, ui, &snap.desk.name);
        let serial = if snap.desk.uid.is_empty() || snap.desk.uid == "demo" { "—" } else { snap.desk.uid.as_str() };
        identity_row(ui, "Serial Number:", serial);
        let firmware = if gear.firmware.is_empty() { "—" } else { gear.firmware.as_str() };
        identity_row(ui, "Firmware Version:", firmware);
        poll_update_notes(app);
        if update_busy() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        let open_fw = tool_row(ui, "Firmware Updater:", "fw-open", "Open...", "fw-check", "Check for updates");
        if open_fw.0 {
            start_update(app, ui.ctx(), UpdateJob::FirmwareOpen);
        }
        if open_fw.1 {
            let current = firmware.to_string();
            start_update(app, ui.ctx(), UpdateJob::FirmwareCheck(current));
        }
        identity_row(ui, "Driver Version:", DRIVER_VERSION);
        ui.add_space(18.0);
        connect_row(app, ui, snap);
        ui.add_space(24.0);
    });
}

fn pick_menu(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, label: &str, value: &str, warn: bool, choices: &[(&str, Cmd)]) {
    let response = menu_row(ui, label, value, warn);
    egui::Popup::menu(&response).show(|ui| {
        for (name, cmd) in choices {
            if ui.selectable_label(false, *name).clicked() && app.writable(snap) {
                app.act(cmd.clone());
            }
        }
    });
}

fn clock_label(value: Option<u8>) -> String {
    match value {
        Some(0) => "Internal",
        Some(5) => "Word Clock",
        Some(8) => "S/PDIF",
        Some(12) => "Optical A",
        Some(13) => "Optical B",
        Some(other) => return other.to_string(),
        None => "—",
    }
    .into()
}

fn loop_place(value: Option<u8>) -> String {
    match value {
        Some(0) => "USB In 11-12 (Default)".into(),
        Some(1) => "USB In 1-2".into(),
        Some(other) => other.to_string(),
        None => "—".into(),
    }
}

fn host_label(value: Option<u8>) -> String {
    match value {
        Some(0) => "Default".into(),
        Some(1) => "Additional".into(),
        Some(other) => other.to_string(),
        None => "—".into(),
    }
}

fn on_label(value: Option<u8>) -> String {
    match value {
        Some(0) => "Disabled".into(),
        Some(_) => "Enabled".into(),
        None => "—".into(),
    }
}

fn word_label(value: Option<u8>) -> String {
    match value {
        Some(1) => "Out".into(),
        Some(0) => "Thru".into(),
        Some(other) => other.to_string(),
        None => "—".into(),
    }
}

fn menu_row(ui: &mut egui::Ui, label: &str, value: &str, warn: bool) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 54.0), egui::Sense::hover());
    let box_rect = egui::Rect::from_min_size(egui::pos2(rect.left() + 172.0, rect.top() + 13.0), egui::vec2(160.0, 28.0));
    ui.painter().text(
        egui::pos2(rect.left() + 14.0, box_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(16.0),
        WHITE,
    );
    ui.painter().rect_stroke(box_rect, 2.0, egui::Stroke::new(1.0, MENU_EDGE), egui::StrokeKind::Inside);
    ui.painter().text(
        box_rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        value,
        egui::FontId::proportional(11.0),
        MENU_VALUE,
    );
    let response = ui.interact(box_rect, ui.make_persistent_id(("device-menu", label)), egui::Sense::click());
    if warn {
        let alert = box_rect.right_center() + egui::vec2(26.0, 0.0);
        paint_alert(ui.painter(), alert);
        let text = egui::pos2(alert.x + 24.0, alert.y);
        ui.painter().text(text + egui::vec2(0.0, -8.0), egui::Align2::LEFT_CENTER, "Changes to Loopback Location", egui::FontId::proportional(11.0), DIM);
        ui.painter().text(text + egui::vec2(0.0, 8.0), egui::Align2::LEFT_CENTER, "will reset the USB connection.", egui::FontId::proportional(11.0), DIM);
    }
    response
}

fn paint_alert(painter: &egui::Painter, center: egui::Pos2) {
    let color = egui::Color32::from_rgb(91, 93, 107);
    let top = center + egui::vec2(0.0, -11.0);
    let left = center + egui::vec2(-12.0, 9.0);
    let right = center + egui::vec2(12.0, 9.0);
    painter.add(egui::Shape::closed_line(vec![top, right, left], egui::Stroke::new(1.5, color)));
    painter.line_segment([center + egui::vec2(0.0, -3.0), center + egui::vec2(0.0, 3.0)], egui::Stroke::new(1.5, color));
    painter.circle_filled(center + egui::vec2(0.0, 6.5), 1.1, color);
}

fn name_row(app: &mut DeskApp, ui: &mut egui::Ui, name: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), egui::Sense::hover());
    let y = rect.top() + 30.0;
    ui.painter().text(egui::pos2(rect.left() + 14.0, y), egui::Align2::LEFT_CENTER, "Device Name:", egui::FontId::proportional(16.0), WHITE);
    if app.rename == Some(super::Rename::Device) {
        let field = egui::Rect::from_min_size(egui::pos2(rect.left() + 172.0, rect.top() + 15.0), egui::vec2(220.0, 30.0));
        app.edit_name(ui, field);
        return;
    }
    if name.is_empty() {
        let button = egui::Rect::from_min_size(egui::pos2(rect.left() + 172.0, rect.top() + 15.0), egui::vec2(70.0, 30.0));
        ui.painter().rect_stroke(button, 6.0, egui::Stroke::new(1.0, MENU_VALUE), egui::StrokeKind::Inside);
        ui.painter().text(button.center(), egui::Align2::CENTER_CENTER, "Set", egui::FontId::proportional(14.0), MENU_VALUE);
        if ui.interact(button, ui.make_persistent_id("device-name-set"), egui::Sense::click()).clicked() {
            app.begin_rename(super::Rename::Device, "");
        }
    } else {
        let text = egui::Rect::from_min_size(egui::pos2(rect.left() + 172.0, rect.top() + 15.0), egui::vec2(280.0, 30.0));
        ui.painter().text(egui::pos2(rect.left() + 172.0, y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(16.0), MENU_VALUE);
        if ui.interact(text, ui.make_persistent_id("device-name-edit"), egui::Sense::click()).double_clicked() {
            app.begin_rename(super::Rename::Device, name);
        }
    }
}

fn identity_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::hover());
    ui.painter().text(egui::pos2(rect.left() + 14.0, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(15.0), WHITE);
    ui.painter().text(egui::pos2(rect.left() + 172.0, rect.center().y), egui::Align2::LEFT_CENTER, value, egui::FontId::proportional(15.0), MENU_VALUE);
}

fn connect_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    if !matches!(snap.link, crate::engine::Link::Live | crate::engine::Link::ReadOnly { .. }) {
        return;
    }
    ui.horizontal(|ui| {
        if chip(ui, "Release the port", false, egui::vec2(150.0, 32.0)).clicked() {
            app.hub.lock().release();
        }
    });
}

/// Shown on the Device page. This is the app version, not MOTU's driver.
const DRIVER_VERSION: &str = "1.0.0";
const MOTU_FIRMWARE_LIST: &str = "http://firmware.motu.com/api/device/828/firmware";

enum UpdateJob {
    FirmwareCheck(String),
    FirmwareOpen,
}

fn update_inbox() -> &'static std::sync::Mutex<Vec<String>> {
    static INBOX: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    &INBOX
}

fn update_flag() -> &'static std::sync::atomic::AtomicBool {
    static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &BUSY
}

fn update_busy() -> bool {
    update_flag().load(std::sync::atomic::Ordering::Relaxed)
}

fn poll_update_notes(app: &DeskApp) {
    let notes = match update_inbox().lock() {
        Ok(mut inbox) => std::mem::take(&mut *inbox),
        Err(_) => return,
    };
    for note in notes {
        app.note(note);
    }
}

fn tool_row(ui: &mut egui::Ui, label: &str, open_id: &str, open_label: &str, check_id: &str, check_label: &str) -> (bool, bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::hover());
    let y = rect.center().y;
    ui.painter().text(
        egui::pos2(rect.left() + 14.0, y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(15.0),
        WHITE,
    );
    let open_w = button_width(ui, open_label);
    let open_pos = egui::pos2(rect.left() + 172.0, y - 15.0);
    let open = outline_button(ui, open_id, open_pos, open_label, open_w);
    let check_pos = egui::pos2(open_pos.x + open_w + 8.0, open_pos.y);
    let check = outline_button(ui, check_id, check_pos, check_label, button_width(ui, check_label));
    (open, check)
}

fn button_width(ui: &egui::Ui, label: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), egui::FontId::proportional(14.0), MENU_VALUE);
    (galley.size().x + 22.0).max(70.0)
}

fn outline_button(ui: &mut egui::Ui, id: &str, pos: egui::Pos2, label: &str, width: f32) -> bool {
    let rect = egui::Rect::from_min_size(pos, egui::vec2(width, 30.0));
    ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, MENU_VALUE), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(14.0), MENU_VALUE);
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click()).clicked()
}

/// Ask MOTU what the current 828 firmware is. This never writes the unit and never installs a file.
fn start_update(app: &DeskApp, ctx: &egui::Context, job: UpdateJob) {
    if update_flag().swap(true, std::sync::atomic::Ordering::AcqRel) {
        app.note("An update check is already running.");
        return;
    }
    app.note(match &job {
        UpdateJob::FirmwareCheck(_) => "Checking MOTU's firmware list.".to_string(),
        UpdateJob::FirmwareOpen => "Opening MOTU's current firmware file. RustMix does not install it.".to_string(),
    });
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let note = match job {
            UpdateJob::FirmwareCheck(current) => firmware_check(&current),
            UpdateJob::FirmwareOpen => firmware_open(),
        };
        if let Ok(mut inbox) = update_inbox().lock() {
            inbox.push(note);
        }
        update_flag().store(false, std::sync::atomic::Ordering::Release);
        ctx.request_repaint();
    });
}

fn firmware_check(current: &str) -> String {
    let body = match curl_text(MOTU_FIRMWARE_LIST) {
        Ok(body) => body,
        Err(_) => return "Could not reach MOTU's firmware list.".into(),
    };
    let (latest, _) = match latest_firmware(&body) {
        Ok(pair) => pair,
        Err(err) => return err,
    };
    if current.is_empty() || current == "—" {
        format!("MOTU lists {latest}. Connect the 828 to compare it with this unit.")
    } else if current == latest {
        format!("Firmware {current} matches the current MOTU release.")
    } else {
        format!("This unit is {current}. MOTU lists {latest}. Open downloads that file. RustMix does not install it.")
    }
}

fn firmware_open() -> String {
    let body = match curl_text(MOTU_FIRMWARE_LIST) {
        Ok(body) => body,
        Err(_) => return "Could not reach MOTU's firmware list.".into(),
    };
    let (version, image) = match latest_firmware(&body) {
        Ok(pair) => pair,
        Err(err) => return err,
    };
    if !trusted_firmware_url(&image) {
        return "MOTU's firmware list did not include a download RustMix will open.".into();
    }
    if open_url(&image) {
        format!("Opened MOTU's firmware {version}. RustMix does not install it.")
    } else {
        "Could not open MOTU's firmware file.".into()
    }
}

fn latest_firmware(body: &str) -> Result<(String, String), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "MOTU's firmware list was not readable.".to_string())?;
    let row = value.as_array().and_then(|rows| rows.first()).ok_or_else(|| "MOTU's firmware list was empty.".to_string())?;
    let version = row.get("version").and_then(|item| item.as_str()).unwrap_or("").to_string();
    let image = row.get("image").and_then(|item| item.as_str()).unwrap_or("").to_string();
    if version.is_empty() {
        return Err("MOTU's firmware list had no version.".into());
    }
    Ok((version, image))
}

fn trusted_firmware_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://cdn-data.motu.com/") else { return false };
    rest.contains("firmware") && !rest.contains("..") && !rest.chars().any(|ch| ch.is_control())
}

fn curl_text(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "8", url])
        .output()
        .map_err(|_| "Could not reach MOTU's firmware list.".to_string())?;
    if !output.status.success() {
        return Err("Could not reach MOTU's firmware list.".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "MOTU's firmware list was not readable.".into())
}

fn open_url(url: &str) -> bool {
    let url = url.to_string();
    std::thread::Builder::new().spawn(move || {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }).is_ok()
}

const EQ_LABEL: egui::Color32 = egui::Color32::from_rgb(116, 117, 135);
const TOGGLE_EDGE: egui::Color32 = egui::Color32::from_rgb(158, 159, 160);
const TOGGLE_BLUE: egui::Color32 = egui::Color32::from_rgb(52, 141, 221);
const ARC_TRACK: egui::Color32 = egui::Color32::from_rgb(35, 38, 44);

pub(crate) fn input(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    // The reference is a fixed canvas, not a grid that stretches with the window.
    // The page is already inset 18px, and the gain block starts 50px from the content edge.
    egui::ScrollArea::vertical()
        .id_salt("inputs")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (area, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 910.0), egui::Sense::hover());
            // Canvas margin is 50. The rule sits 34px above the gain group, same as the reference header.
            let origin = egui::pos2(area.left() + 50.0, area.top());
            let group = egui::Rect::from_min_size(origin + egui::vec2(0.0, 63.2), egui::vec2(760.0, 520.0));
            paint_input_header(ui, app.wordmark.as_ref(), area, super::unit_open(snap));
            label_frame(ui, group, "Input Gain", 16.0, 2.0, None);
            let inputs = group.left_top() - egui::vec2(20.0, 14.0);
            let mut x = inputs.x + 38.0;
            for col in 0..5 {
                let width = if col == 0 { 180.0 } else { 113.0 };
                for row in 0..2 {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(x, inputs.y + 44.0 + row as f32 * 240.0),
                        egui::vec2(width, 230.0),
                    );
                    paint_input_cell(app, ui, snap, rect, (col * 2 + row) as u16);
                }
                x += width + 20.0;
            }
            let below = inputs.y + 44.0 + 470.0 + 35.0;
            let spdif = egui::Rect::from_min_size(egui::pos2(inputs.x + 20.0, below), egui::vec2(120.0, 120.0));
            let optical = egui::Rect::from_min_size(egui::pos2(spdif.right() + 20.0, below), egui::vec2(620.0, 120.0));
            let foot = egui::Rect::from_min_size(egui::pos2(inputs.x + 20.0, optical.bottom() + 25.0), egui::vec2(762.0, 120.0));
            paint_spdif(ui, snap, spdif);
            paint_optical(app, ui, snap, optical);
            paint_foot(app, ui, snap, foot);
        });
}

fn paint_input_header(ui: &mut egui::Ui, wordmark: Option<&egui::TextureHandle>, area: egui::Rect, live: bool) {
    // Home and Device are already inset 18px. This canvas is the full page, so the rule
    // uses that same span and the same 828 placement.
    let rect = egui::Rect::from_min_max(
        egui::pos2(area.left() + 18.0, area.top()),
        egui::pos2(area.right() - 18.0, area.top() + 46.0),
    );
    super::paint_device_rule(ui, wordmark, live, rect, area.top() + 29.2);
}

fn paint_input_cell(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect, index: u16) {
    let name = match index {
        0 => "Mic",
        1 => "Mic/Line/Inst 2",
        _ => topology::input_name(index as usize),
    };
    label_frame(ui, rect, name, 14.0, 1.0, Some((index + 1).to_string()));
    let meter = egui::Rect::from_min_size(rect.left_top() + egui::vec2(12.0, 25.0), egui::vec2(10.0, 126.0));
    super::paint_level(ui.painter(), meter, super::meter_of(&snap.desk.gear, topology::input_meter(index as usize)));
    let gear = &snap.desk.gear;
    if index < 2 {
        let flags = [
            ("48V", crate::gear::flag(gear.phantom[index as usize])),
            ("Pad", crate::gear::flag(gear.pad[index as usize])),
            ("\u{00d8}", crate::gear::flag(gear.phase[index as usize])),
        ];
        for (step, (caption, on)) in flags.iter().enumerate() {
            let button = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 35.0, rect.top() + 27.0 + step as f32 * 46.0),
                egui::vec2(52.0, 30.0),
            );
            if toggle_button(ui, (index, *caption), button, caption, *on, TOGGLE_BLUE) && app.writable(snap) {
                let cmd = match step {
                    0 => Cmd::Phantom { index, on: !on },
                    1 => Cmd::Pad { index, on: !on },
                    _ => Cmd::Phase { index, invert: !on },
                };
                app.act(cmd);
            }
        }
        // Channel 1 shows the insert pair on this 828. The other mic is one preamp knob.
        if index == 0 {
            paint_gain(app, ui, snap, 0, (index, "pre"), rect.left_top() + egui::vec2(100.0, 15.0), Some(("Preamp", "Gain")));
            paint_gain(app, ui, snap, 10, (index, "ins"), rect.left_top() + egui::vec2(100.0, 95.0), Some(("Insert", "Gain")));
        } else {
            paint_gain(app, ui, snap, 1, (index, "gain"), rect.left_top() + egui::vec2(100.0, 27.0), None);
        }
    } else {
        paint_gain(app, ui, snap, index, (index, "gain"), rect.left_top() + egui::vec2(33.0, 27.0), None);
        let phase = egui::Rect::from_min_size(egui::pos2(rect.left() + 42.0, rect.top() + 119.0), egui::vec2(52.0, 30.0));
        let on = crate::gear::flag(gear.phase.get(index as usize).copied().flatten());
        if toggle_button(ui, (index, "phase"), phase, "\u{00d8}", on, TOGGLE_BLUE) && app.writable(snap) {
            app.act(Cmd::Phase { index, invert: !on });
        }
    }
    let menu = egui::Rect::from_min_size(rect.left_top() + egui::vec2(12.0, 178.0), egui::vec2(rect.width() - 24.0, 28.0));
    let slot = usb_slot(snap, index);
    let pre = topology::local_source(index as usize);
    let post = 92 + index as u8;
    let current = snap.desk.patch.get(slot as usize).copied().unwrap_or(NOTHING);
    let usb = if current == pre { "Pre Fx" } else if current == post { "Post Fx" } else { "Custom" };
    ui.painter().text(
        egui::pos2(menu.left() + 2.0, menu.top() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        "To USB host:",
        noto("NotoSans", 14.0),
        EQ_LABEL,
    );
    ui.painter().rect_stroke(menu, 2.0, egui::Stroke::new(1.0, MENU_EDGE), egui::StrokeKind::Inside);
    ui.painter().text(
        menu.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        usb,
        noto("NotoSansBold", 11.0),
        MENU_VALUE,
    );
    let response = ui.interact(menu, ui.make_persistent_id((index, "usb")), egui::Sense::click());
    egui::Popup::menu(&response).show(|ui| {
        for (label, source) in [("Pre Fx", pre), ("Post Fx", post)] {
            if ui.selectable_label(false, label).clicked() && source != current && app.writable(snap) {
                app.act(Cmd::Patch { slot, source });
            }
        }
    });
}

fn usb_slot(snap: &Snap, index: u16) -> u16 {
    let first = u16::from(snap.desk.gear.loop_first.unwrap_or(0));
    index + first * 2
}

fn paint_gain(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, preamp: u16, id: impl std::hash::Hash + std::fmt::Debug + Copy, at: egui::Pos2, title: Option<(&str, &str)>) {
    let rect = egui::Rect::from_min_size(at, egui::vec2(68.0, 75.0));
    let center = egui::pos2(rect.left() + 34.0, rect.top() + 35.0);
    let (fraction, value) = super::preamp_view(&snap.desk.gear, preamp);
    let mut track = Vec::with_capacity(49);
    let mut hot = Vec::new();
    let start = 0.75 * std::f32::consts::PI;
    let sweep = 1.5 * std::f32::consts::PI;
    for step in 0..=48 {
        let t = step as f32 / 48.0;
        let angle = start + sweep * t;
        let point = center + egui::vec2(angle.cos(), angle.sin()) * 27.0;
        track.push(point);
        if t <= fraction + f32::EPSILON {
            hot.push(point);
        }
    }
    ui.painter().add(egui::Shape::line(track, egui::Stroke::new(7.0, ARC_TRACK)));
    if hot.len() >= 2 {
        ui.painter().add(egui::Shape::line(hot, egui::Stroke::new(7.0, TOGGLE_BLUE)));
    }
    if let Some((upper, lower)) = title {
        ui.painter().text(center + egui::vec2(0.0, -7.0), egui::Align2::CENTER_CENTER, upper, noto("NotoSansReg", 10.0), TOGGLE_BLUE);
        ui.painter().text(center + egui::vec2(0.0, 7.0), egui::Align2::CENTER_CENTER, lower, noto("NotoSansReg", 10.0), TOGGLE_BLUE);
    }
    let lock = egui::Rect::from_min_size(rect.left_top() + egui::vec2(48.0, 1.0), egui::vec2(18.0, 16.0));
    paint_padlock(ui.painter(), lock, false);
    let readout = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.top() + 57.0), egui::vec2(68.0, 18.0));
    ui.painter().text(readout.center(), egui::Align2::CENTER_CENTER, &value, noto("NotoSansReg", 12.0), egui::Color32::from_rgb(128, 128, 128));
    let arc_hit = ui.interact(rect, ui.make_persistent_id((id, "arc")), egui::Sense::click()).clicked();
    let lock_hit = ui.interact(lock, ui.make_persistent_id((id, "lock")), egui::Sense::click()).clicked();
    if lock_hit {
        app.note(UNAVAILABLE);
    } else if arc_hit && app.writable(snap) {
        app.act(Cmd::StepPreamp { index: preamp, louder: super::arc_louder(ui, center.y) });
    }
}

fn paint_spdif(ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    label_frame(ui, rect, "S/PDIF", 16.0, 2.0, None);
    let pair = 22.0;
    let left = rect.left() + (rect.width() - pair) * 0.5;
    for index in 0..2 {
        let meter = egui::Rect::from_min_size(egui::pos2(left + index as f32 * 12.0, rect.top() + 25.0), egui::vec2(10.0, 80.0));
        super::paint_level(ui.painter(), meter, snap.desk.gear.meter_db(58 + index));
    }
}

fn paint_optical(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    label_frame(ui, rect, "Optical In", 16.0, 2.0, None);
    let adat = egui::Rect::from_min_size(rect.left_top() + egui::vec2(32.0, 58.0), egui::vec2(48.0, 26.0));
    let tos = egui::Rect::from_min_size(egui::pos2(adat.right() + 17.0, adat.top()), egui::vec2(48.0, 26.0));
    ui.painter().text(
        egui::pos2(adat.left() + 1.0, adat.top() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        "Bank A format:",
        noto("NotoSans", 14.0),
        EQ_LABEL,
    );
    let mode = snap.desk.gear.optical[0];
    let adat_on = mode == Some(0);
    let tos_on = mode == Some(1);
    if toggle_button(ui, "optical-adat", adat, "ADAT", adat_on, TOGGLE_BLUE) && !adat_on && app.writable(snap) && rate_allows(snap) {
        app.act(Cmd::Optical { index: 0, tos: false });
    }
    if toggle_button(ui, "optical-tos", tos, "TOS", tos_on, TOGGLE_BLUE) && !tos_on && app.writable(snap) && rate_allows(snap) {
        app.act(Cmd::Optical { index: 0, tos: true });
    }
    // Two optical banks, eight channels each, at the 1x layout.
    for bank in 0..2 {
        for meter_index in 0..8 {
            let pair = meter_index / 2;
            let within = if meter_index % 2 == 0 { 0.0 } else { 12.0 };
            let x = rect.left() + 225.0 + bank as f32 * 142.0 + pair as f32 * 28.0 + within;
            let meter = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 25.0), egui::vec2(10.0, 80.0));
            let which = topology::input_meter(12 + bank * 8 + meter_index);
            super::paint_level(ui.painter(), meter, super::meter_of(&snap.desk.gear, which));
        }
    }
}

fn rate_allows(snap: &Snap) -> bool {
    let rate = snap.desk.gear.sample_rate;
    rate == 0 || rate <= 96_000
}

fn paint_foot(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    label_frame(ui, rect, "Foot Switch", 16.0, 2.0, None);
    let enable = egui::Rect::from_min_size(rect.left_top() + egui::vec2(23.0, 54.0), egui::vec2(58.0, 26.0));
    let enabled = crate::gear::flag(snap.desk.gear.foot_en);
    if toggle_button(ui, "foot-enable", enable, "Enable", enabled, TOGGLE_BLUE) && app.writable(snap) {
        app.act(Cmd::FootEnable { on: !enabled });
    }
    paint_alert(ui.painter(), rect.left_top() + egui::vec2(112.0, 66.0));
    let lines = [
        "Enabling the foot switch will cause",
        "the USB connection to disconnect",
        "briefly. Your PC will connect the",
        "foot switch as a \"USB keyboard\"",
        "(so it can trigger keystrokes).",
    ];
    for (index, line) in lines.iter().enumerate() {
        ui.painter().text(
            egui::pos2(rect.left() + 145.0, rect.top() + 37.0 + index as f32 * 15.0),
            egui::Align2::LEFT_BOTTOM,
            *line,
            noto("NotoSansBold", 10.0),
            EQ_LABEL,
        );
    }
    let down = egui::Rect::from_min_size(egui::pos2(enable.right() + 282.0, rect.top() + 60.0), egui::vec2(145.0, 28.0));
    let up = egui::Rect::from_min_size(egui::pos2(down.right() + 57.0, down.top()), egui::vec2(145.0, 28.0));
    let keys = [
        ("foot-down", down, "Down keystroke:", false, snap.desk.gear.foot_down.as_str()),
        ("foot-up", up, "Up keystroke:", true, snap.desk.gear.foot_up.as_str()),
    ];
    for (id, entry, caption, up_key, stored) in keys {
        ui.painter().text(
            egui::pos2(entry.left() + 3.0, entry.top() - 4.0),
            egui::Align2::LEFT_BOTTOM,
            caption,
            noto("NotoSans", 14.0),
            EQ_LABEL,
        );
        let armed = app.foot_arm == Some(up_key);
        let shown = if armed { "Press a key".to_string() } else { super::foot_label(stored) };
        ui.painter().rect_stroke(entry, 2.0, egui::Stroke::new(1.0, if armed { TOGGLE_BLUE } else { MENU_EDGE }), egui::StrokeKind::Middle);
        ui.painter().text(entry.center(), egui::Align2::CENTER_CENTER, &shown, noto("NotoSansBold", 11.0), EQ_LABEL);
        if ui.interact(entry, ui.make_persistent_id(id), egui::Sense::click()).clicked() {
            app.rename = None;
            app.foot_arm = Some(up_key);
        }
    }
}

fn label_frame(ui: &mut egui::Ui, rect: egui::Rect, title: &str, size: f32, width: f32, badge: Option<String>) {
    // The reference stroke does not follow the view's top edge. A 2px group drops
    // 12px, then half the stroke. A channel frame is inset 1px and drops another 10.
    let frame = if width >= 2.0 {
        let inset = width * 0.5;
        egui::Rect::from_min_max(
            egui::pos2(rect.left() + inset, rect.top() + 12.0 + inset),
            egui::pos2(rect.right() - inset, rect.bottom()),
        )
    } else {
        egui::Rect::from_min_max(
            egui::pos2(rect.left() + 1.0, rect.top() + 11.0),
            egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0),
        )
    };
    ui.painter().rect_stroke(frame, 6.0, egui::Stroke::new(width, EQ_LABEL), egui::StrokeKind::Middle);
    let galley = ui.painter().layout_no_wrap(title.to_owned(), noto("NotoSans", size), EQ_LABEL);
    let text_pos = egui::pos2(frame.left() + 8.0, frame.top() - galley.size().y * 0.45);
    let cover = egui::Rect::from_min_size(text_pos - egui::vec2(2.0, 1.0), galley.size() + egui::vec2(6.0, 2.0));
    ui.painter().rect_filled(cover, 0.0, CONTENT);
    ui.painter().galley(text_pos, galley, EQ_LABEL);
    if let Some(number) = badge {
        let chip = egui::Rect::from_min_size(frame.right_bottom() - egui::vec2(16.0, 16.0), egui::vec2(16.0, 16.0));
        ui.painter().rect_filled(chip, 4.0, EQ_LABEL);
        ui.painter().text(chip.center(), egui::Align2::CENTER_CENTER, number, noto("NotoSansReg", 11.0), CONTENT);
    }
}

fn toggle_button(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, rect: egui::Rect, label: &str, on: bool, on_color: egui::Color32) -> bool {
    let visual = rect.shrink(2.0);
    if on {
        ui.painter().rect_filled(visual, 6.0, on_color);
        ui.painter().text(visual.center(), egui::Align2::CENTER_CENTER, label, noto("NotoSans", 14.0), egui::Color32::BLACK);
    } else {
        ui.painter().rect_stroke(visual, 6.0, egui::Stroke::new(1.0, TOGGLE_EDGE), egui::StrokeKind::Middle);
        ui.painter().text(visual.center(), egui::Align2::CENTER_CENTER, label, noto("NotoSans", 14.0), TOGGLE_EDGE);
    }
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click()).clicked()
}

fn noto(name: &str, size: f32) -> egui::FontId {
    egui::FontId { size, family: egui::FontFamily::Name(std::sync::Arc::from(name)) }
}

const READOUT: egui::Color32 = egui::Color32::from_rgb(128, 128, 128);
const CHIP_VALUE: egui::Color32 = egui::Color32::from_rgb(185, 191, 217);
const CHIP_EDGE: egui::Color32 = egui::Color32::from_rgb(0, 207, 213);
const CTRL_BODY: egui::Color32 = egui::Color32::from_rgb(196, 200, 210);
const CTRL_HEAD: egui::Color32 = egui::Color32::from_rgb(220, 224, 232);
const CTRL_GROUP: egui::Color32 = egui::Color32::from_rgb(176, 181, 194);
const CTRL_TEXT: egui::Color32 = egui::Color32::from_rgb(232, 235, 242);
const SOFT_ON: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(0, 207, 213, 41);
const LOCK_ON: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(239, 183, 28, 46);
const COL_W: f32 = 560.0;
const NOTE_W: f32 = 640.0;
const CHIP_ON: egui::Color32 = egui::Color32::from_rgb(239, 183, 28);
const MUTE_RED: egui::Color32 = egui::Color32::from_rgb(207, 31, 37);

const PHONE_SOURCES: [(&str, u8, u8); 9] = [
    ("Nothing", NOTHING, NOTHING),
    ("Main 1-2 Mix (L-R)", 76, 77),
    ("Line 3-4 Mix (L-R)", 78, 79),
    ("Line 5-6 Mix (L-R)", 80, 81),
    ("Line 7-8 Mix (L-R)", 82, 83),
    ("Line 9-10 Mix (L-R)", 84, 85),
    ("Phones 1 Mix (L-R)", 86, 87),
    ("Phones 2 Mix (L-R)", 88, 89),
    ("Reverb (L-R)", 90, 91),
];

pub(crate) fn output(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    // Fixed canvas. The 480px window scrolls; the blocks do not stretch.
    egui::ScrollArea::vertical()
        .id_salt("outputs")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (area, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1040.0), egui::Sense::hover());
            let left = area.left();
            let top = area.top();
            paint_input_header(ui, app.wordmark.as_ref(), area, super::unit_open(snap));
            let monitoring = egui::Rect::from_min_size(egui::pos2(left + 50.0, top + 60.0), egui::vec2(685.0, 140.0));
            paint_monitoring(app, ui, snap, monitoring);
            let phones_y = monitoring.bottom() + 20.0;
            paint_phones(app, ui, snap, 0, egui::Rect::from_min_size(egui::pos2(left + 50.0, phones_y), egui::vec2(334.0, 148.0)));
            paint_phones(app, ui, snap, 1, egui::Rect::from_min_size(egui::pos2(left + 404.0, phones_y), egui::vec2(334.0, 148.0)));
            let grid = egui::Rect::from_min_size(egui::pos2(left + 50.0, phones_y + 168.0), egui::vec2(685.0, 300.0));
            paint_trim_grid(app, ui, snap, grid);
            let digital_y = grid.bottom() + 20.0;
            paint_loopback(app, ui, snap, egui::Rect::from_min_size(egui::pos2(left + 50.0, digital_y), egui::vec2(334.0, 120.0)));
            paint_spdif_out(app, ui, snap, egui::Rect::from_min_size(egui::pos2(left + 404.0, digital_y), egui::vec2(334.0, 120.0)));
            paint_optical_out(
                app,
                ui,
                snap,
                egui::Rect::from_min_size(egui::pos2(left + 50.0, digital_y + 140.0), egui::vec2(685.0, 120.0)),
            );
        });
}

fn paint_monitoring(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    // drawLabelGroup is called at l+2, t+2. The 100px block is centered under an 11px pad.
    let frame = egui::Rect::from_min_max(rect.left_top() + egui::vec2(2.0, 2.0), rect.right_bottom());
    label_frame(ui, frame, "Monitoring", 16.0, 2.0, None);
    let block_y = 26.0;
    level_pair(
        ui,
        snap,
        rect.left_top() + egui::vec2(16.0, block_y),
        10.0,
        100.0,
        topology::jack_meter(patch_source(snap, topology::line_slot(0))),
        topology::jack_meter(patch_source(snap, topology::line_slot(1))),
    );
    let locked = snap.locks.blocks_monitor();
    let all = snap.locks.all;
    let quiet = locked || !app.writable(snap);
    let value = monitor_text(&snap.desk);
    let knob = egui::Rect::from_min_size(rect.left_top() + egui::vec2(48.0, block_y), egui::vec2(68.0, 100.0));
    let press = paint_io_knob(ui, "out-mon", knob, stored_fraction(snap.desk.monitor), locked, true, all, true, quiet, &value);
    if press.all {
        app.lock(LockChange::All(!all));
    }
    if press.lock {
        app.lock(LockChange::Monitor(!locked));
    }
    if !quiet && press.down {
        app.act(Cmd::StepMonitor { louder: false });
    }
    if !quiet && press.up {
        app.act(Cmd::StepMonitor { louder: true });
    }
    let mute = egui::Rect::from_min_size(rect.left_top() + egui::vec2(131.0, block_y), egui::vec2(68.0, 30.0));
    let mono = egui::Rect::from_min_size(egui::pos2(mute.left(), mute.top() + 70.0), egui::vec2(68.0, 30.0));
    let muted = crate::gear::flag(snap.desk.gear.mon_mute);
    let mono_on = crate::gear::flag(snap.desk.gear.mono);
    if toggle_button(ui, "out-mute", mute, "MUTE", muted, MUTE_RED) && app.writable(snap) {
        app.act(Cmd::MonMute { on: !muted });
    }
    if toggle_button(ui, "out-mono", mono, "MONO", mono_on, TOGGLE_BLUE) && app.writable(snap) {
        app.act(Cmd::MonMono { on: !mono_on });
    }
    let group = egui::Rect::from_min_size(rect.left_top() + egui::vec2(214.0, 0.0), egui::vec2(192.0, 140.0));
    paint_monitor_buttons(app, ui, snap, group);
    let ab = egui::Rect::from_min_size(rect.left_top() + egui::vec2(414.0, 0.0), egui::vec2(260.0, 138.0));
    paint_ab_row(app, ui, snap, ab);
}

fn paint_monitor_buttons(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    // Canvas baselines land the glyph centers here. "1 - 2" is a little farther down than an even step.
    let center_x = rect.left() + 45.0;
    for (index, line) in ["Main", "Out", "1 - 2"].iter().enumerate() {
        let y = rect.top() + [70.0, 87.0, 107.0][index];
        ui.painter().text(
            egui::pos2(center_x, y),
            egui::Align2::CENTER_CENTER,
            *line,
            noto("NotoSans", 14.0),
            TOGGLE_BLUE,
        );
    }
    // The rule runs from the label column to the last button. The title is centered on that span.
    let rule_y = rect.top() + 40.0;
    let title = "Monitor Group";
    let galley = ui.painter().layout_no_wrap(title.to_owned(), noto("NotoSans", 14.0), EQ_LABEL);
    let title_x = rect.left() + 105.0 - galley.size().x * 0.5;
    ui.painter().line_segment(
        [egui::pos2(rect.left() + 20.0, rule_y), egui::pos2(rect.right(), rule_y)],
        egui::Stroke::new(1.0, EQ_LABEL),
    );
    let cover = egui::Rect::from_min_size(egui::pos2(title_x - 6.0, rule_y - galley.size().y * 0.5), galley.size() + egui::vec2(12.0, 0.0));
    ui.painter().rect_filled(cover, 0.0, CONTENT);
    ui.painter().galley(egui::pos2(title_x, rule_y - galley.size().y * 0.5), galley, EQ_LABEL);
    for col in 0..4 {
        let x = rect.left() + 68.0 + col as f32 * 33.0;
        for (row, number) in [(58.0, col * 2 + 3), (88.0, col * 2 + 4)] {
            let button = egui::Rect::from_min_size(egui::pos2(x, rect.top() + row), egui::vec2(25.0, 25.0));
            let label = number.to_string();
            let bit = topology::group_bit(number as u8);
            let mask = snap.desk.gear.main_group.unwrap_or(0);
            let on = bit.is_some_and(|bit| (mask & bit) != 0);
            if toggle_button(ui, ("out-group", number), button, &label, on, TOGGLE_BLUE) {
                if let Some(bit) = bit {
                    if app.writable(snap) {
                        let next = if on { mask & !bit } else { mask | bit };
                        app.act(Cmd::Group { mask: next });
                    }
                }
            }
        }
    }
}

fn paint_ab_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    let inner = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + 15.0), egui::pos2(rect.right() - 2.0, rect.bottom()));
    let place = |cx: f32, cy: f32| egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(60.0, 30.0));
    let gear = &snap.desk.gear;
    let a = crate::gear::flag(gear.a_on);
    let b = crate::gear::flag(gear.b_on);
    let buttons = [
        ("out-ab-a", inner.center().x, inner.top() + 40.0, "A", a && !b, super::AbButton::A),
        ("out-ab-on", inner.left() + 55.0, inner.top() + inner.height() * 0.5, "AB ON", a || b, super::AbButton::On),
        ("out-ab-both", inner.right() - 55.0, inner.top() + inner.height() * 0.5, "BOTH", a && b, super::AbButton::Both),
        ("out-ab-b", inner.center().x, inner.bottom() - 40.0, "B", b && !a, super::AbButton::B),
    ];
    let blocked = crate::gear::flag(gear.expander);
    for (id, x, y, label, on, which) in buttons {
        if toggle_button(ui, id, place(x, y), label, on, TOGGLE_BLUE) {
            if blocked {
                app.note("The optical expander turns A/B off.");
            } else if app.writable(snap) {
                let (enable, a, b) = super::ab_next(a, b, which);
                app.act(Cmd::Ab { enable, a, b });
            }
        }
    }
}

fn paint_phones(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, pair: u8, rect: egui::Rect) {
    let title = if pair == 0 { "Phones 1" } else { "Phones 2" };
    label_frame(ui, rect, title, 16.0, 2.0, None);
    level_pair(
        ui,
        snap,
        rect.left_top() + egui::vec2(17.0, 25.0),
        10.0,
        80.0,
        topology::jack_meter(patch_source(snap, topology::phone_slot(pair as usize * 2))),
        topology::jack_meter(patch_source(snap, topology::phone_slot(pair as usize * 2 + 1))),
    );
    let locked = snap.locks.blocks_phones(pair);
    let quiet = locked || !app.writable(snap);
    let left = snap.desk.phones.get(pair as usize * 2).copied().flatten();
    let right = snap.desk.phones.get(pair as usize * 2 + 1).copied().flatten();
    let value = pair_text(left, right);
    let knob = egui::Rect::from_min_size(rect.left_top() + egui::vec2(53.0, 27.0), egui::vec2(68.0, 100.0));
    let press = paint_io_knob(ui, ("out-ph", pair), knob, stored_fraction(left.or(right)), locked, false, false, true, quiet, &value);
    if press.lock {
        app.lock(LockChange::Phones(pair, !locked));
    }
    if !quiet && press.down {
        app.act(Cmd::StepPhones { pair, louder: false });
    }
    if !quiet && press.up {
        app.act(Cmd::StepPhones { pair, louder: true });
    }
    let menu = egui::Rect::from_min_size(rect.left_top() + egui::vec2(140.0, 60.0), egui::vec2(180.0, 30.0));
    let (label, have_l, have_r) = phone_label(snap, pair);
    let response = menu_field(ui, ("out-ph-menu", pair), menu, "Source:", &label);
    let slot_l = topology::phone_slot(pair as usize * 2);
    let slot_r = topology::phone_slot(pair as usize * 2 + 1);
    egui::Popup::menu(&response).show(|ui| {
        for (name, left, right) in PHONE_SOURCES {
            if ui.selectable_label(false, name).clicked() && (have_l, have_r) != (left, right) && app.writable(snap) {
                app.act_all(&[
                    Cmd::Patch { slot: slot_l, source: left },
                    Cmd::Patch { slot: slot_r, source: right },
                ]);
            }
        }
    });
}

fn phone_label(snap: &Snap, pair: u8) -> (String, u8, u8) {
    let left = snap.desk.patch.get(topology::phone_slot(pair as usize * 2) as usize).copied().unwrap_or(NOTHING);
    let right = snap.desk.patch.get(topology::phone_slot(pair as usize * 2 + 1) as usize).copied().unwrap_or(NOTHING);
    let name = PHONE_SOURCES
        .iter()
        .find(|source| source.1 == left && source.2 == right)
        .map(|source| source.0.to_string())
        .unwrap_or_else(|| topology::source_label(left));
    (name, left, right)
}

fn paint_trim_grid(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    // drawLabelGroup(c, "Output Trim", l, t+2, w-2, h-2)
    let frame = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + 2.0), egui::pos2(rect.right() - 2.0, rect.bottom()));
    label_frame(ui, frame, "Output Trim", 16.0, 2.0, None);
    for col in 0..5 {
        for row in 0..2 {
            let index = (col * 2 + row) as u8;
            let cell = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 20.0 + col as f32 * 133.0, rect.top() + 30.0 + row as f32 * 130.0),
                egui::vec2(113.0, 120.0),
            );
            paint_trim_cell(app, ui, snap, index, cell);
        }
    }
}

fn paint_trim_cell(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, which: u8, rect: egui::Rect) {
    let title = trim_channel_name(snap, which);
    label_frame(ui, rect, &title, 14.0, 1.0, Some((which + 1).to_string()));
    let meter = egui::Rect::from_min_size(rect.left_top() + egui::vec2(12.5, 22.5), egui::vec2(10.0, 80.0));
    super::paint_level(
        ui.painter(),
        meter,
        super::meter_of(&snap.desk.gear, topology::jack_meter(patch_source(snap, topology::line_slot(which as usize)))),
    );
    let stored = snap.desk.trims.get(which as usize).copied().flatten();
    let locked = snap.locks.blocks_trim(which);
    let value = stored.map(topology::stored_text).unwrap_or_else(|| "—".into());
    let knob = egui::Rect::from_min_size(rect.left_top() + egui::vec2(32.5, 26.5), egui::vec2(68.0, 75.0));
    let press = paint_io_knob(ui, ("out-trim", which), knob, stored_fraction(stored), locked, false, false, false, true, &value);
    if press.lock {
        app.lock(LockChange::Trim(which, !locked));
    }
}

/// One physical output. A custom name, including line 5, wins over the default.
fn trim_channel_name(snap: &Snap, which: u8) -> String {
    let custom = snap
        .desk
        .gear
        .out_names
        .get(which as usize)
        .map(|text| text.trim_matches('\0').trim())
        .unwrap_or("");
    if custom.is_empty() {
        topology::output_jack_name(which as usize)
    } else {
        custom.to_string()
    }
}

fn paint_loopback(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    label_frame(ui, rect, "Loopback", 16.0, 2.0, None);
    let left = snap.desk.gear.loop_l.unwrap_or(NOTHING);
    let right = snap.desk.gear.loop_r.unwrap_or(NOTHING);
    level_pair(ui, snap, rect.left_top() + egui::vec2(28.0, 25.0), 10.0, 80.0, Some(left as usize), Some(right as usize));
    let menu = egui::Rect::from_min_size(rect.left_top() + egui::vec2(78.0, 60.0), egui::vec2(210.0, 30.0));
    let choices = topology::loop_sources(snap.desk.gear.sample_rate);
    let label = choices.iter().find(|item| item.left == left && item.right == right).map(|item| item.label).unwrap_or("Nothing");
    let response = menu_field(ui, "out-loop", menu, "Source:", label);
    scroll_menu(&response, |ui| {
        if ui.selectable_label(false, "Nothing").clicked() && app.writable(snap) && (left, right) != (NOTHING, NOTHING) {
            app.act(Cmd::LoopSource { left: NOTHING, right: NOTHING });
        }
        for item in choices {
            if ui.selectable_label(false, item.label).clicked() && app.writable(snap) && (left, right) != (item.left, item.right) {
                app.act(Cmd::LoopSource { left: item.left, right: item.right });
            }
        }
    });
}

fn paint_spdif_out(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    label_frame(ui, rect, "S/PDIF Out", 16.0, 2.0, None);
    let meter = egui::Rect::from_min_size(rect.left_top() + egui::vec2(37.0, 25.0), egui::vec2(10.0, 80.0));
    let meter_r = egui::Rect::from_min_size(egui::pos2(meter.right() + 2.0, meter.top()), egui::vec2(10.0, 80.0));
    let left = snap.desk.patch.get(58).copied().unwrap_or(NOTHING);
    let right = snap.desk.patch.get(59).copied().unwrap_or(NOTHING);
    super::paint_level(ui.painter(), meter, source_meter(&snap.desk.gear, left));
    super::paint_level(ui.painter(), meter_r, source_meter(&snap.desk.gear, right));
    let menu = egui::Rect::from_min_size(egui::pos2(meter_r.right() + 45.9, rect.top() + 60.0), egui::vec2(160.0, 30.0));
    let text = stereo_text(left, right);
    let response = menu_field(ui, "out-spdif", menu, "Source:", &text);
    if rate_allows(snap) {
        let choices = stereo_choices(false);
        scroll_menu(&response, |ui| {
            for (name, src_l, src_r) in choices {
                if ui.selectable_label(false, name).clicked() && app.writable(snap) && (left, right) != (src_l, src_r) {
                    app.act_all(&[
                        Cmd::Patch { slot: 58, source: src_l },
                        Cmd::Patch { slot: 59, source: src_r },
                    ]);
                }
            }
        });
    }
}

fn paint_optical_out(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, rect: egui::Rect) {
    label_frame(ui, rect, "Optical Out", 16.0, 2.0, None);
    let control_y = rect.top() + 55.0;
    let adat = egui::Rect::from_min_size(egui::pos2(rect.left() + 19.2, control_y), egui::vec2(49.0, 30.0));
    let tos = egui::Rect::from_min_size(egui::pos2(adat.right() + 8.0, control_y), egui::vec2(49.0, 30.0));
    ui.painter().text(
        egui::pos2(adat.left(), adat.top() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        "Bank format:",
        noto("NotoSans", 14.0),
        EQ_LABEL,
    );
    let mode = snap.desk.gear.optical[1];
    let adat_on = mode == Some(0);
    let tos_on = mode == Some(1);
    if toggle_button(ui, "out-adat", adat.expand2(egui::vec2(2.0, 0.0)), "ADAT", adat_on, TOGGLE_BLUE) && !adat_on && app.writable(snap) && rate_allows(snap) {
        app.act(Cmd::Optical { index: 1, tos: false });
    }
    if toggle_button(ui, "out-tos", tos, "TOS", tos_on, TOGGLE_BLUE) && !tos_on && app.writable(snap) && rate_allows(snap) {
        app.act(Cmd::Optical { index: 1, tos: true });
    }
    let menu_a = egui::Rect::from_min_size(egui::pos2(rect.left() + 144.4, control_y), egui::vec2(145.0, 30.0));
    let menu_b = egui::Rect::from_min_size(egui::pos2(rect.left() + 520.8, control_y), egui::vec2(145.0, 30.0));
    for (bank, id, menu, caption) in [
        (0u8, "out-opt-a", menu_a, "Bank A Source:"),
        (1u8, "out-opt-b", menu_b, "Bank B Source:"),
    ] {
        let slot = topology::optical_dest(bank, 0) as usize;
        let source = snap.desk.patch.get(slot).copied().unwrap_or(NOTHING);
        let response = menu_field(ui, id, menu, caption, &topology::source_label(source));
        if rate_allows(snap) {
            let choices = stereo_choices(true);
            scroll_menu(&response, |ui| {
                for (name, start, _) in choices {
                    if ui.selectable_label(false, name).clicked() && app.writable(snap) {
                        spread_optical(app, bank, start, tos_on);
                    }
                }
            });
        }
    }
    let mut x = rect.left() + 308.6;
    for bank in 0..2u8 {
        if bank == 1 {
            x += 9.0;
        }
        for pair in 0..4 {
            for side in 0..2 {
                let meter = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 25.0), egui::vec2(8.0, 80.0));
                let dest = topology::optical_dest(bank, pair * 2 + side);
                let source = snap.desk.patch.get(dest as usize).copied().unwrap_or(NOTHING);
                super::paint_level(ui.painter(), meter, source_meter(&snap.desk.gear, source));
                x += 10.0;
            }
            x += 3.0;
        }
    }
}

fn spread_optical(app: &mut DeskApp, bank: u8, start: u8, tos: bool) {
    let count = if tos { 2 } else { 8 };
    let cmds: Vec<Cmd> = (0..count)
        .map(|index| Cmd::Patch {
            slot: topology::optical_dest(bank, index),
            source: topology::spread_source(start, index),
        })
        .collect();
    app.act_all(&cmds);
}

fn stereo_text(left: u8, right: u8) -> String {
    if left >= NOTHING && right >= NOTHING {
        "Nothing".into()
    } else if left == right {
        topology::source_label(left)
    } else {
        format!("{}  /  {}", topology::source_label(left), topology::source_label(right))
    }
}

fn stereo_choices(allow_spdif_in: bool) -> Vec<(String, u8, u8)> {
    let mut rows = vec![("Nothing".into(), NOTHING, NOTHING)];
    for index in (0..10).step_by(2) {
        let left = topology::local_source(index);
        let right = topology::spread_source(left, 1);
        rows.push((format!("{} / {}", topology::input_name(index), topology::input_name(index + 1)), left, right));
    }
    if allow_spdif_in {
        rows.push(("S/PDIF in".into(), 58, 59));
    }
    for index in (0..16).step_by(2) {
        rows.push((
            format!("ADAT {}-{}", index + 1, index + 2),
            topology::adat_code(index),
            topology::adat_code(index + 1),
        ));
    }
    for index in (0..18).step_by(2) {
        rows.push((format!("Computer {}-{}", index + 1, index + 2), index as u8, index as u8 + 1));
    }
    for (name, left, right) in PHONE_SOURCES.iter().skip(1) {
        rows.push(((*name).into(), *left, *right));
    }
    rows
}

fn patch_source(snap: &Snap, slot: u16) -> u8 {
    snap.desk.patch.get(slot as usize).copied().unwrap_or(NOTHING)
}

fn source_meter(gear: &crate::gear::Gear, source: u8) -> Option<f32> {
    if source >= NOTHING {
        None
    } else {
        gear.meter_db(source as usize)
    }
}

fn level_pair(ui: &mut egui::Ui, snap: &Snap, origin: egui::Pos2, width: f32, height: f32, left: Option<usize>, right: Option<usize>) {
    for (index, which) in [left, right].into_iter().enumerate() {
        let meter = egui::Rect::from_min_size(origin + egui::vec2(index as f32 * (width + 2.0), 0.0), egui::vec2(width, height));
        let db = which.and_then(|index| if index >= usize::from(NOTHING) { None } else { snap.desk.gear.meter_db(index) });
        super::paint_level(ui.painter(), meter, db);
    }
}

/// Keep a long source list on screen. A click still writes the same source as before.
fn scroll_menu(response: &egui::Response, body: impl FnOnce(&mut egui::Ui)) {
    egui::Popup::menu(response).width(response.rect.width().max(248.0)).show(|ui| {
        ui.set_max_height(240.0);
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                ui.set_min_width(220.0);
                body(ui);
            });
    });
}

fn menu_field(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, rect: egui::Rect, caption: &str, value: &str) -> egui::Response {
    ui.painter().text(
        egui::pos2(rect.left(), rect.top() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        caption,
        noto("NotoSans", 14.0),
        EQ_LABEL,
    );
    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, MENU_EDGE), egui::StrokeKind::Inside);
    ui.painter().text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        value,
        noto("NotoSansBold", 11.0),
        MENU_VALUE,
    );
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click())
}

struct KnobPress {
    all: bool,
    lock: bool,
    down: bool,
    up: bool,
}

fn paint_io_knob(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    rect: egui::Rect,
    fraction: f32,
    locked: bool,
    all: bool,
    all_on: bool,
    steps: bool,
    quiet: bool,
    value: &str,
) -> KnobPress {
    let center = egui::pos2(rect.left() + 34.0, rect.top() + 35.0);
    let mut track = Vec::with_capacity(49);
    let mut hot = Vec::new();
    let start = 0.75 * std::f32::consts::PI;
    let sweep = 1.5 * std::f32::consts::PI;
    let fraction = fraction.clamp(0.0, 1.0);
    for step in 0..=48 {
        let t = step as f32 / 48.0;
        let angle = start + sweep * t;
        let point = center + egui::vec2(angle.cos(), angle.sin()) * 27.0;
        track.push(point);
        if t <= fraction + f32::EPSILON {
            hot.push(point);
        }
    }
    ui.painter().add(egui::Shape::line(track, egui::Stroke::new(7.0, ARC_TRACK)));
    if hot.len() >= 2 {
        ui.painter().add(egui::Shape::line(hot, egui::Stroke::new(7.0, TOGGLE_BLUE)));
    }
    let mut press = KnobPress { all: false, lock: false, down: false, up: false };
    if all {
        let chip = egui::Rect::from_min_size(rect.left_top() + egui::vec2(2.0, 1.0), egui::vec2(26.0, 14.0));
        press.all = hearing_chip(ui, (id, "all"), chip, "ALL", all_on, false);
    }
    let lock = egui::Rect::from_min_size(egui::pos2(rect.right() - 20.0, rect.top() + 1.0), egui::vec2(18.0, 16.0));
    paint_padlock(ui.painter(), lock, locked);
    press.lock = ui.interact(lock, ui.make_persistent_id((id, "lock")), egui::Sense::click()).clicked();
    let text_top = if rect.height() > 90.0 { rect.bottom() - 36.0 } else { rect.bottom() - 18.0 };
    ui.painter().text(
        egui::pos2(rect.center().x, text_top + 9.0),
        egui::Align2::CENTER_CENTER,
        value,
        noto("NotoSansReg", 12.0),
        READOUT,
    );
    if steps {
        let y = rect.bottom() - 18.0;
        let down = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, y), egui::vec2(30.0, 16.0));
        let up = egui::Rect::from_min_size(egui::pos2(rect.left() + 36.0, y), egui::vec2(30.0, 16.0));
        press.down = hearing_chip(ui, (id, "down"), down, "−1", false, quiet);
        press.up = hearing_chip(ui, (id, "up"), up, "+1", false, quiet);
    }
    press
}

fn hearing_chip(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, rect: egui::Rect, label: &str, on: bool, dim: bool) -> bool {
    let dim_c = egui::Color32::from_rgb(48, 48, 48);
    if dim {
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, dim_c), egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, noto("NotoSansReg", 9.0), dim_c);
    } else if on {
        ui.painter().rect_filled(rect, 2.0, CHIP_ON);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, noto("NotoSansReg", 9.0), egui::Color32::BLACK);
    } else {
        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140));
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, CHIP_EDGE), egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, noto("NotoSansReg", 9.0), CHIP_VALUE);
    }
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click()).clicked()
}



pub(crate) fn control(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    device_title(ui, app.wordmark.as_ref(), super::unit_open(snap));
    // The page shell is already inset 18px. The reference bar is 28px from the sidebar and 18px from the top.
    let outer = ui.max_rect();
    let content = egui::Rect::from_min_max(
        egui::pos2(outer.left() + 10.0, ui.cursor().min.y + 6.0),
        egui::pos2(outer.right() - 10.0, outer.bottom()),
    );
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(content));
    egui::ScrollArea::vertical().id_salt("control").auto_shrink([false, false]).show(&mut page, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        tracked(ui, "CONTROL", 20.0, 0.12, WHITE);
        ui.add_space(8.0);
        heading(ui, "MCP", false);
        note(ui, &format!("{}. The assistant talks to this window. It does not open another mixer.", crate::APP_NAME), CTRL_BODY);
        note(ui, "It can change routing and move programs on this PC.", CTRL_BODY);
        note(ui, "Monitor level, volume, and headphone level are refused.", super::WARN);
        note(ui, "It cannot read a level, and it cannot change these locks.", CTRL_BODY);
        column(ui, |ui| {
            if outline(ui, "MCP", if snap.mcp_enabled { "ON" } else { "OFF" }, snap.mcp_enabled, Tone::Soft) {
                app.set_mcp_enabled(!snap.mcp_enabled);
            }
            if let Some(text) = mcp_field(ui, "Address", &mut app.mcp_host_buf, "mcp-host", "127.0.0.1", &snap.mcp_host) {
                app.set_mcp_host(&text);
            }
            let port_text = snap.mcp_port.to_string();
            if let Some(text) = mcp_field(ui, "Port", &mut app.mcp_port_buf, "mcp-port", "4771", &port_text) {
                app.set_mcp_port_text(&text);
            }
        });
        let listen = if snap.mcp_listen.is_empty() { "MCP is turning on." } else { snap.mcp_listen.as_str() };
        let bad = listen.contains("in use") || listen.contains("not on this") || listen.contains("could not");
        note(ui, listen, if bad { super::WARN } else { CTRL_BODY });
        if !crate::settings::mcp_is_loopback(&snap.mcp_host) {
            note(ui, "Other computers can reach this address. They can change routing.", super::WARN);
        }
        heading(ui, "SOFTWARE", false);
        note(ui, &format!("Choose how {} opens and closes on this PC.", crate::APP_NAME), CTRL_BODY);
        column(ui, |ui| {
            if outline(ui, "Launch when this PC starts", if snap.launch { "ON" } else { "OFF" }, snap.launch, Tone::Soft) {
                let mut engine = app.hub.lock();
                if let Err(err) = engine.set_launch(!snap.launch) {
                    engine.notice = err;
                }
            }
            if outline(ui, "Start minimized", if snap.start_minimized { "ON" } else { "OFF" }, snap.start_minimized, Tone::Soft) {
                let mut engine = app.hub.lock();
                if let Err(err) = engine.set_minimized(!snap.start_minimized) {
                    engine.notice = err;
                }
            }
        });
        note(ui, &format!("Starting minimized applies the next time {} opens.", crate::APP_NAME), CTRL_BODY);
        heading(ui, "SCREEN", true);
        let mut screens = app.screens.clone().unwrap_or_default();
        screens.sort_by(|left, right| {
            left.geo
                .y
                .total_cmp(&right.geo.y)
                .then(left.geo.x.total_cmp(&right.geo.x))
                .then(left.name.cmp(&right.name))
        });
        note(ui, screen_note(&screens, snap.screen_key.as_deref()), CTRL_BODY);
        column(ui, |ui| {
            for screen in &screens {
                let on = snap.screen_key.as_deref() == Some(screen.key.as_str())
                    || (screens.len() == 1 && snap.screen_key.is_none());
                if outline(ui, &screen.label, if on { "ON" } else { "OFF" }, on, Tone::Soft) && !on {
                    app.choose_screen(screen);
                }
            }
        });
        heading(ui, "CLOSE", true);
        note(ui, "The X button follows this choice. When the window is already minimized, closing it quits. The 828 keeps its current mix.", CTRL_BODY);
        column(ui, |ui| {
            if outline(ui, "Quit the program", if snap.close_quits { "ON" } else { "OFF" }, snap.close_quits, Tone::Soft) && !snap.close_quits {
                let mut engine = app.hub.lock();
                if let Err(err) = engine.set_close_quits(true) {
                    engine.notice = err;
                }
            }
            if outline(ui, "Minimize to the taskbar", if snap.close_quits { "OFF" } else { "ON" }, !snap.close_quits, Tone::Soft) && snap.close_quits {
                let mut engine = app.hub.lock();
                if let Err(err) = engine.set_close_quits(false) {
                    engine.notice = err;
                }
            }
        });
        if quit_button(ui, "Quit RustMix Crimson Edition").clicked() {
            app.force_quit = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        heading(ui, "LEVEL LOCKS", false);
        note(ui, "A lock keeps the current level where it is. Turning lock all off leaves the switches below as they were.", CTRL_BODY);
        column(ui, |ui| {
            if outline(ui, "Lock all levels", if snap.locks.all { "ON" } else { "OFF" }, snap.locks.all, Tone::Lock) {
                app.lock(LockChange::All(!snap.locks.all));
            }
        });
        heading(ui, "MONITOR", true);
        column(ui, |ui| {
            lock_bar(app, ui, "Monitor", snap.locks.all, snap.locks.monitor, |on| LockChange::Monitor(on));
        });
        heading(ui, "HEADPHONES", true);
        column(ui, |ui| {
            for pair in 0..2u8 {
                let label = if pair == 0 { "Headphones 1" } else { "Headphones 2" };
                lock_bar(app, ui, label, snap.locks.all, snap.locks.phones[pair as usize], |on| LockChange::Phones(pair, on));
            }
        });
        heading(ui, "OUTPUT TRIMS", true);
        const NAMES: [&str; 10] = [
            "Main 1", "Main 2", "Line 3", "Line 4", "Line 5", "Line 6", "Line 7", "Line 8", "Line 9", "Line 10",
        ];
        column(ui, |ui| {
            for which in 0..10u8 {
                lock_bar(app, ui, NAMES[which as usize], snap.locks.all, snap.locks.trims[which as usize], |on| LockChange::Trim(which, on));
            }
        });
        heading(ui, "INPUT GAIN", true);
        column(ui, |ui| {
            for label in ["Mic", "Mic/Line/Inst 2", "Line 3", "Line 4", "Line 5", "Line 6", "Line 7", "Line 8", "Line 9", "Line 10", "Mic insert", "Mic/Line/Inst 2 insert"] {
                if outline(ui, label, "OPEN", false, Tone::Lock) {
                    app.note(UNAVAILABLE);
                }
            }
        });
        heading(ui, "TALKBACK", true);
        column(ui, |ui| {
            for label in ["Talkback level", "Talkback dim"] {
                if outline(ui, label, "OPEN", false, Tone::Lock) {
                    app.note(UNAVAILABLE);
                }
            }
        });
        let cells = snap.locks.cells.clone();
        if !cells.is_empty() {
            heading(ui, "MIX FADERS", true);
            column(ui, |ui| {
                for (bus, input) in &cells {
                    let label = format!("{} · {}", super::single_name(*bus), topology::input_name(*input as usize));
                    lock_bar(app, ui, &label, snap.locks.all, true, |on| LockChange::Cell(*bus, *input, on));
                }
            });
        }
        let buses = snap.locks.buses.clone();
        if !buses.is_empty() {
            heading(ui, "BUS MASTERS", true);
            column(ui, |ui| {
                for bus in buses {
                    let label = format!("{} master", super::single_name(bus));
                    lock_bar(app, ui, &label, snap.locks.all, true, |on| LockChange::Bus(bus, on));
                }
            });
        }
        ui.add_space(8.0);
        note(ui, &link_line(&snap.link), CTRL_BODY);
    });
}

fn screen_note(screens: &[crate::settings::Screen], key: Option<&str>) -> &'static str {
    if screens.is_empty() {
        "No screens were listed."
    } else if key.is_some_and(|key| screens.iter().all(|screen| screen.key != key)) {
        "The saved screen is unplugged. Pick one of the screens below."
    } else if screens.len() == 1 {
        "This PC has one screen. RustMix Crimson Edition stays on it."
    } else {
        "RustMix Crimson Edition stays on the screen you pick. It opens there next time, in the same place."
    }
}

enum Tone {
    Soft,
    Lock,
}

fn column(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(COL_W));
        ui.spacing_mut().item_spacing.y = 6.0;
        body(ui);
    });
}

fn heading(ui: &mut egui::Ui, title: &str, sub: bool) {
    ui.add_space(if sub { 16.0 } else { 18.0 });
    let size = if sub { 12.0 } else { 13.0 };
    let tracking = if sub { 0.08 } else { 0.12 };
    let color = if sub { CTRL_GROUP } else { CTRL_HEAD };
    tracked(ui, title, size, tracking, color);
    ui.add_space(if sub { 6.0 } else { 8.0 });
}

fn note(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let width = ui.available_width().min(NOTE_W);
    let galley = tracked_galley(ui, text, 15.0, 0.0, color, Some(22.0), width);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, galley.size().y), egui::Sense::hover());
    ui.painter().galley(rect.left_top(), galley, color);
    ui.add_space(8.0);
}

fn tracked(ui: &mut egui::Ui, text: &str, size: f32, em: f32, color: egui::Color32) {
    let galley = tracked_galley(ui, text, size, em, color, None, f32::INFINITY);
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    ui.painter().galley(rect.left_top(), galley, color);
}

fn tracked_galley(
    ui: &egui::Ui,
    text: &str,
    size: f32,
    em: f32,
    color: egui::Color32,
    line_height: Option<f32>,
    wrap: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: noto("NotoSansReg", size),
            extra_letter_spacing: size * em,
            line_height,
            color,
            ..Default::default()
        },
    );
    ui.painter().layout_job(job)
}

fn outline(ui: &mut egui::Ui, label: &str, state: &str, on: bool, tone: Tone) -> bool {
    let width = ui.available_width().min(COL_W);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 38.0), egui::Sense::click());
    let (fill, border, text) = match (tone, on) {
        (Tone::Soft, true) => (SOFT_ON, CHIP_EDGE, CHIP_EDGE),
        (Tone::Lock, true) => (LOCK_ON, super::WARN, super::WARN),
        _ => (egui::Color32::TRANSPARENT, CHIP_EDGE, CTRL_TEXT),
    };
    if fill.a() > 0 {
        ui.painter().rect_filled(rect, 2.0, fill);
    }
    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
    ui.painter().text(
        rect.left_center() + egui::vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        noto("NotoSansReg", 15.0),
        text,
    );
    let state_galley = tracked_galley(ui, state, 13.0, 0.06, text, None, f32::INFINITY);
    let state_pos = rect.right_center() - egui::vec2(12.0 + state_galley.size().x, state_galley.size().y * 0.5);
    ui.painter().galley(state_pos, state_galley, text);
    response.clicked()
}

fn lock_bar(app: &mut DeskApp, ui: &mut egui::Ui, label: &str, held: bool, stored: bool, change: impl FnOnce(bool) -> LockChange) {
    let (word, on) = if held {
        ("HELD", true)
    } else if stored {
        ("LOCKED", true)
    } else {
        ("OPEN", false)
    };
    // While lock-all is on, the row is held and a click does not change the switch underneath.
    if outline(ui, label, word, on, Tone::Lock) && !held {
        app.lock(change(!stored));
    }
}

fn quit_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_space(8.0);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), noto("NotoSansReg", 15.0), WHITE);
    let (rect, response) = ui.allocate_exact_size(egui::vec2((galley.size().x + 28.0).ceil(), 38.0), egui::Sense::click());
    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, CTRL_TEXT), egui::StrokeKind::Inside);
    ui.painter().galley(egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y * 0.5), galley, WHITE);
    response
}

fn mcp_field(ui: &mut egui::Ui, label: &str, buf: &mut String, id: &str, hint: &str, committed: &str) -> Option<String> {
    let width = ui.available_width().min(COL_W);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
    ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, label, noto("NotoSansReg", 15.0), CTRL_TEXT);
    let box_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 180.0, rect.top()), egui::vec2(180.0, 34.0));
    let frame = egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, CHIP_EDGE))
        .corner_radius(2)
        .inner_margin(egui::Margin::symmetric(8, 6));
    let edit = egui::TextEdit::singleline(buf)
        .id_salt(id)
        .desired_width(160.0)
        .min_size(egui::vec2(180.0, 34.0))
        .hint_text(hint)
        .font(egui::FontSelection::FontId(noto("NotoSansReg", 15.0)))
        .frame(frame)
        .align(egui::Align2::LEFT_CENTER)
        .text_color(WHITE);
    let response = ui.put(box_rect, edit);
    let enter = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if (response.lost_focus() || enter) && buf.trim() != committed {
        Some(buf.clone())
    } else {
        None
    }
}

pub(crate) fn discovery(ui: &mut egui::Ui, snap: &Snap) {
    ui.label(egui::RichText::new("DISCOVERY").size(22.0).strong());
    ui.add_space(12.0);
    card(ui, "This computer", egui::vec2(ui.available_width().min(520.0), 132.0), |ui| {
        let name = if snap.desk.name.is_empty() { "828" } else { snap.desk.name.as_str() };
        ui.label(egui::RichText::new(name).size(20.0).strong());
        ui.label(egui::RichText::new(link_line(&snap.link)).color(DIM));
        ui.label("RustMix Crimson Edition talks to the 828 plugged into this computer.");
        ui.label("It does not scan the network.");
    });
}

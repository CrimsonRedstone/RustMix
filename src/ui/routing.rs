//! Sources on the left, destinations on the right.
//! Drag a cable from either jack. A click on one jack and then the other connects them.
//! Click a drawn cable to pull that route off. A missed drop eases back.
//! One source can feed many outputs. Stereo, when on, sends the partner too.

use eframe::egui;

use super::{cable, DeskApp, Pick, Retract, Snap, CARD, CYAN, DIM, WARN, WHITE};
use crate::pipewire::{Program, Role};
use crate::plan::{Cmd, Origin};
use crate::topology::{self, NOTHING};

const BAY_GAP: f32 = 36.0;
const SRC_PC: u8 = 4;
const SRC_DUMMY: u8 = 5;
const DST_PC: u8 = 2;
const DST_DUMMY: u8 = 3;

pub(crate) fn show(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    app.landed = false;
    app.src_pos.clear();
    app.dst_pos.clear();
    app.pc_pos.clear();
    remember_programs(app, snap);
    // The click lands on the cord that was painted last frame, not on the jack under its end.
    app.cable_click = None;
    app.pc_cable_click = None;
    app.dummy_cable_click = None;
    let dragging = egui::DragAndDrop::payload::<Pick>(ui.ctx()).is_some();
    if !dragging {
        if let Some(pointer) = ui.input(|input| input.pointer.interact_pos().filter(|_| input.pointer.primary_clicked())) {
            let hardware = closest_cable(&app.cable_hits, pointer);
            let program = closest_cable(&app.pc_cable_hits, pointer);
            let dummy = closest_cable(&app.dummy_cable_hits, pointer);
            let mut best = f32::MAX;
            if let Some((slot, dist)) = hardware {
                if dist < best {
                    best = dist;
                    app.cable_click = Some(slot);
                }
            }
            if let Some((id, dist)) = program {
                if dist < best {
                    best = dist;
                    app.cable_click = None;
                    app.pc_cable_click = Some(id);
                }
            }
            if let Some((cable, dist)) = dummy {
                if dist < best {
                    app.cable_click = None;
                    app.pc_cable_click = None;
                    app.dummy_cable_click = Some(cable);
                }
            }
        }
    }
    if let Some(slot) = app.cable_click {
        if !uncable_if_dummy(app, snap, slot) {
            disconnect(app, snap, slot);
        }
    }
    if let Some(id) = app.pc_cable_click.clone() {
        unroute_program_cable(app, snap, &id);
    }
    if let Some(cable) = app.dummy_cable_click.clone() {
        pull_dummy_cable(app, snap, &cable);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(egui::RichText::new("ROUTING").font(face("NotoSansBold", 20.0)).color(WHITE));
        if super::chip(ui, "STEREO", app.stereo, egui::vec2(78.0, 26.0)).clicked() {
            app.stereo = !app.stereo;
        }
        if super::chip(ui, "DEFAULT", false, egui::vec2(86.0, 26.0)).clicked() {
            apply_slots(app, snap, &crate::desk::Desk::factory_patch(), "Routing is at its default.");
        }
        if super::chip(ui, "SAVE", false, egui::vec2(62.0, 26.0)).clicked() {
            save_state(app, snap);
        }
        if super::chip(ui, "LOAD", false, egui::vec2(62.0, 26.0)).clicked() {
            load_state(app, snap);
        }
        if super::chip(ui, "DUMMY", false, egui::vec2(70.0, 26.0)).clicked() {
            create_dummy(app);
        }
    });
    let mut hint = "Drag a cable from one end to the other. Click a cable to pull it off.".to_owned();
    if app.stereo {
        hint.push_str(" Stereo sends the partner too.");
    }
    hint.push_str(" DUMMY mixes several apps or inputs. Its out can feed one jack or several.");
    if app.src_bank == SRC_PC || app.dst_bank == DST_PC {
        hint.push_str(" On PC, click the route or drag the jack. The lock keeps it.");
        hint.push_str(" Replacing what an output is playing asks first.");
    }
    ui.label(egui::RichText::new(hint).font(face("NotoSans", 12.5)).color(DIM));
    ui.add_space(4.0);
    consent_card(app, ui, snap);
    let width = ui.available_width();
    let half = ((width - BAY_GAP) * 0.5).max(160.0);
    let body = ui.available_height().max(180.0);
    let mut left = egui::Rect::NOTHING;
    let mut right = egui::Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = BAY_GAP;
        left = source_column(app, ui, snap, half, body);
        right = dest_column(app, ui, snap, half, body);
    });
    paint(app, ui, snap, left, right);
    finish_drag(app, ui);
}

fn source_column(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32, height: f32) -> egui::Rect {
    let mut bank = app.src_bank;
    let clip = column_shell(ui, width, height, "SOURCES", &["DEVICE", "MIXER", "HOST", "DSP", "PC", "DUMMY"], &mut bank, |ui, bank| {
        if bank == SRC_DUMMY {
            dummy_outs(app, ui, snap);
            return ui.clip_rect();
        }
        if bank == SRC_PC {
            pc_programs(app, ui, snap, false);
            return ui.clip_rect();
        }
        let mut head = "";
        for jack in listed_sources(bank) {
            let next = source_head(&jack);
            if next != head {
                if !head.is_empty() {
                    ui.add_space(6.0);
                }
                head = next;
                ui.label(egui::RichText::new(head).font(face("NotoSansBold", 11.0)).color(DIM));
            }
            source_row(app, ui, snap, jack.id, jack.name);
        }
        ui.clip_rect()
    });
    app.src_bank = bank;
    clip
}

fn dest_column(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, width: f32, height: f32) -> egui::Rect {
    let mut bank = app.dst_bank;
    let clip = column_shell(ui, width, height, "DESTINATIONS", &["DEVICE", "HOST", "PC", "DUMMY"], &mut bank, |ui, bank| {
        if bank == DST_DUMMY {
            dummy_ins(app, ui, snap);
            return ui.clip_rect();
        }
        if bank == DST_PC {
            pc_programs(app, ui, snap, true);
            return ui.clip_rect();
        }
        let mut head = "";
        for jack in listed_dests(bank) {
            let next = dest_head(&jack);
            if next != head {
                if !head.is_empty() {
                    ui.add_space(6.0);
                }
                head = next;
                ui.label(egui::RichText::new(head).font(face("NotoSansBold", 11.0)).color(DIM));
            }
            let source = snap.desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING);
            dest_row(app, ui, snap, jack.id, &dest_caption(&jack), source);
        }
        ui.clip_rect()
    });
    app.dst_bank = bank;
    clip
}

fn column_shell(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    title: &str,
    names: &[&str],
    selected: &mut u8,
    body: impl FnOnce(&mut egui::Ui, u8) -> egui::Rect,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 10.0, CARD);
    let mut col = ui.new_child(
        egui::UiBuilder::new().max_rect(rect.shrink(10.0)).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    col.set_clip_rect(rect);
    col.label(egui::RichText::new(title).font(face("NotoSansBold", 12.0)).color(DIM));
    col.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, name) in names.iter().enumerate() {
            let wide = (name.len() as f32 * 8.0 + 16.0).clamp(52.0, 78.0);
            if super::chip(ui, name, *selected == index as u8, egui::vec2(wide, 24.0)).clicked() {
                *selected = index as u8;
            }
        }
    });
    let chosen = *selected;
    let scroll_h = (rect.bottom() - col.cursor().min.y - 4.0).max(80.0);
    let mut clip = egui::Rect::NOTHING;
    egui::ScrollArea::vertical()
        .id_salt(("bay-col", title))
        .max_height(scroll_h)
        .auto_shrink([false, false])
        .show(&mut col, |ui| {
            clip = body(ui, chosen);
        });
    clip
}

fn listed_sources(bank: u8) -> Vec<topology::Jack> {
    topology::sources().into_iter().filter(|jack| source_bank(jack) == bank).collect()
}

fn listed_dests(bank: u8) -> Vec<topology::Jack> {
    topology::destinations().into_iter().filter(|jack| dest_bank(jack) == bank).collect()
}

fn source_bank(jack: &topology::Jack) -> u8 {
    match jack.group {
        "Inputs" | "ADAT" => 0,
        "Mixes" => 1,
        "Computer" => 2,
        "Reverb" | "Post-FX" => 3,
        _ => 9,
    }
}

fn dest_bank(jack: &topology::Jack) -> u8 {
    match jack.group {
        "Outputs" | "Phones" | "S/PDIF" | "Optical out" => 0,
        "To the computer" => 1,
        _ => 9,
    }
}

fn source_head(jack: &topology::Jack) -> &'static str {
    match jack.group {
        "Inputs" if jack.name.starts_with("Mic") => "MICS",
        "Inputs" if jack.name.starts_with("Line") => "LINE INPUTS",
        "Inputs" => "S/PDIF",
        "ADAT" if jack.name.contains(" A ") => "ADAT A",
        "ADAT" => "ADAT B",
        "Mixes" if jack.name.starts_with("Main") => "MAIN",
        "Mixes" if jack.name.starts_with("Analog") => "ANALOG",
        "Mixes" if jack.name.starts_with("Phones") => "PHONES",
        "Mixes" => "OTHER MIXES",
        "Computer" => "HOST",
        "Reverb" => "REVERB",
        "Post-FX" => "POST-FX",
        _ => jack.group,
    }
}

fn dest_head(jack: &topology::Jack) -> &'static str {
    match jack.group {
        "Outputs" if jack.name.starts_with("Main") => "MAIN",
        "Outputs" => "LINE OUTS",
        "Phones" => "PHONES",
        "S/PDIF" => "S/PDIF",
        "Optical out" if jack.name.contains(" A ") => "ADAT A",
        "Optical out" => "ADAT B",
        "To the computer" => "HOST",
        _ => jack.group,
    }
}

fn dest_caption(jack: &topology::Jack) -> String {
    match jack.name {
        "Main L" => "Main 1".into(),
        "Main R" => "Main 2".into(),
        "Line 3" | "Line 4" | "Line 5" | "Line 6" | "Line 7" | "Line 8" | "Line 9" | "Line 10" => {
            format!("Line out {}", &jack.name[5..])
        }
        other if jack.group == "Optical out" => other.replace("ADAT A ", "ADAT A out ").replace("ADAT B ", "ADAT B out "),
        other => other.into(),
    }
}

fn source_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, id: u16, name: &str) {
    let pick = Pick { from_dest: false, id };
    let armed = matches!(app.arm, Some(Pick { from_dest: false, id: armed_id }) if armed_id == id);
    let dragged_id = egui::Id::unique(("src-drag", id));
    let mut clicked = false;
    let frame = row_frame(armed);
    let (_zone, dropped) = ui.dnd_drop_zone::<Pick, _>(frame, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let name_w = (ui.available_width() - 48.0).clamp(36.0, 220.0);
            ui.add_sized(
                [name_w, 22.0],
                egui::Label::new(
                    egui::RichText::new(name).font(face("NotoSansReg", 13.0)).color(if armed { CYAN } else { WHITE }),
                )
                .truncate(),
            );
            let feeds = visible_feeds(snap, id);
            if feeds > 0 {
                let (badge, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().circle_filled(badge.center(), 8.0, CYAN);
                ui.painter().text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    feeds.to_string(),
                    face("NotoSansBold", 10.0),
                    egui::Color32::from_rgb(8, 12, 14),
                );
            }
            let drag = ui.dnd_drag_source(dragged_id, pick, |ui| {
                jack_dot(ui, armed || ui.ctx().is_being_dragged(dragged_id));
            });
            let visible = drag.response.rect.intersects(ui.clip_rect());
            if visible {
                app.src_pos.insert(id, drag.response.rect.center());
            } else {
                app.src_pos.remove(&id);
            }
            if drag.response.clicked() && app.cable_click.is_none() {
                clicked = true;
            }
        });
    });
    if let Some(payload) = dropped {
        if let Some(program_id) = pc_key(app, payload.id) {
            if payload.from_dest {
                send_capture(app, snap, &program_id, id);
            }
        } else if payload.from_dest {
            if crate::dummy::is_in(payload.id) {
                deliver_source_to_dummy(app, snap, id, payload.id);
            } else {
                connect(app, snap, payload.id, id);
            }
        }
    }
    if clicked {
        on_click(app, snap, false, id);
    }
}

fn dest_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, slot: u16, name: &str, source: u8) {
    let pick = Pick { from_dest: true, id: slot };
    let armed = matches!(app.arm, Some(Pick { from_dest: true, id }) if id == slot);
    let dragged_id = egui::Id::unique(("dst-drag", slot));
    let mut clicked = false;
    let frame = row_frame(armed);
    let (_zone, dropped) = ui.dnd_drop_zone::<Pick, _>(frame, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let drag = ui.dnd_drag_source(dragged_id, pick, |ui| {
                jack_dot(ui, armed || ui.ctx().is_being_dragged(dragged_id));
            });
            if drag.response.rect.intersects(ui.clip_rect()) {
                app.dst_pos.insert(slot, drag.response.rect.center());
            } else {
                app.dst_pos.remove(&slot);
            }
            if drag.response.clicked() && app.cable_click.is_none() {
                clicked = true;
            }
            let (caption, connected) = slot_caption(snap, slot, source);
            let caption_w = 112.0;
            let name_w = (ui.available_width() - caption_w - 6.0).max(72.0);
            ui.add_sized(
                [name_w, 22.0],
                egui::Label::new(egui::RichText::new(name).font(face("NotoSansReg", 13.0)).color(WHITE)).truncate(),
            );
            let caption_color = if connected { CYAN } else { DIM };
            ui.add_sized(
                [caption_w, 22.0],
                egui::Label::new(egui::RichText::new(caption).font(face("NotoSansReg", 12.0)).color(caption_color)).truncate(),
            );
        });
    });
    if let Some(payload) = dropped {
        if let Some(program_id) = pc_key(app, payload.id) {
            if !payload.from_dest {
                send_playback(app, snap, &program_id, slot);
            }
            return;
        }
        if !payload.from_dest {
            if crate::dummy::is_out(payload.id) {
                deliver_dummy_out(app, snap, payload.id, slot);
            } else {
                connect(app, snap, slot, payload.id);
            }
            return;
        }
    }
    if clicked {
        on_click(app, snap, true, slot);
    }
}

fn on_click(app: &mut DeskApp, snap: &Snap, from_dest: bool, id: u16) {
    if let Some(pick) = app.arm {
        if let Some(program_id) = pc_key(app, pick.id) {
            if from_dest != pick.from_dest {
                if from_dest {
                    send_playback(app, snap, &program_id, id);
                } else {
                    send_capture(app, snap, &program_id, id);
                }
                app.arm = None;
                app.landed = true;
                return;
            }
        }
    }
    if pc_key(app, id).is_some() {
        return;
    }
    if crate::dummy::index_of(id).is_some() || app.arm.is_some_and(|pick| crate::dummy::index_of(pick.id).is_some()) {
        click_dummy(app, snap, from_dest, id);
        return;
    }
    match app.arm {
        Some(Pick { from_dest: false, id: source }) if from_dest => connect(app, snap, id, source),
        Some(Pick { from_dest: true, id: slot }) if !from_dest => connect(app, snap, slot, id),
        Some(Pick { from_dest: have_dest, id: have }) if have_dest == from_dest && have == id => {
            app.arm = None;
        }
        _ => app.arm = Some(Pick { from_dest, id }),
    }
}

fn connect(app: &mut DeskApp, snap: &Snap, slot: u16, source: u16) {
    let Some(source) = u8::try_from(source).ok() else { return };
    let mut cmds = vec![Cmd::Patch { slot, source }];
    if app.stereo {
        if let (Some(partner_slot), Some(partner_source)) = (topology::dest_partner(slot), topology::source_partner(source as u16)) {
            if partner_slot != slot {
                if let Ok(partner_source) = u8::try_from(partner_source) {
                    cmds.push(Cmd::Patch { slot: partner_slot, source: partner_source });
                }
            }
        }
    }
    send(app, snap, cmds);
}

fn disconnect(app: &mut DeskApp, snap: &Snap, slot: u16) {
    let mut cmds = vec![Cmd::Patch { slot, source: NOTHING }];
    if app.stereo {
        if let Some(partner) = topology::dest_partner(slot) {
            if partner != slot {
                cmds.push(Cmd::Patch { slot: partner, source: NOTHING });
            }
        }
    }
    send(app, snap, cmds);
}

fn apply_slots(app: &mut DeskApp, snap: &Snap, wanted: &[u8], done: &str) {
    if !app.writable(snap) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    let mut cmds = Vec::new();
    for jack in topology::destinations() {
        let have = snap.desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING);
        let source = wanted.get(jack.id as usize).copied().unwrap_or(NOTHING);
        if have != source {
            cmds.push(Cmd::Patch { slot: jack.id, source });
        }
    }
    if cmds.is_empty() {
        app.note("Routing is already at that state.");
        return;
    }
    let mut engine = app.hub.lock();
    match engine.call(Origin::Person, &cmds) {
        Ok(()) => engine.notice = done.to_string(),
        Err(err) => {
            let mut applied = 0usize;
            for cmd in &cmds {
                if engine.call(Origin::Person, &[*cmd]).is_ok() {
                    applied += 1;
                }
            }
            engine.notice = if applied == 0 {
                err
            } else {
                format!("Some routes stayed off. {err}")
            };
        }
    }
}

fn routing_file() -> std::path::PathBuf {
    crate::settings::config_dir().join("routing.json")
}

fn save_state(app: &mut DeskApp, snap: &Snap) {
    let mut slots = serde_json::Map::new();
    for jack in topology::destinations() {
        let source = snap.desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING);
        slots.insert(jack.id.to_string(), serde_json::Value::from(source));
    }
    let body = serde_json::Value::Object(serde_json::Map::from_iter([("slots".to_string(), serde_json::Value::Object(slots))]));
    let bytes = serde_json::to_vec_pretty(&body).unwrap_or_default();
    match crate::settings::write_atomic(&routing_file(), &bytes) {
        Ok(()) => app.note("Routing saved."),
        Err(_) => app.note("Routing could not be saved."),
    }
}

fn load_state(app: &mut DeskApp, snap: &Snap) {
    let bytes = match std::fs::read(routing_file()) {
        Ok(bytes) => bytes,
        Err(_) => {
            app.note("No routing is saved yet.");
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            app.note("The saved routing could not be read.");
            return;
        }
    };
    let Some(slots) = value.get("slots").and_then(|item| item.as_object()) else {
        app.note("The saved routing could not be read.");
        return;
    };
    let mut wanted = snap.desk.patch.clone();
    if wanted.len() < crate::topology::PATCH_LEN {
        wanted.resize(crate::topology::PATCH_LEN, NOTHING);
    }
    for jack in topology::destinations() {
        let Some(source) = slots.get(&jack.id.to_string()).and_then(|item| item.as_u64()) else {
            continue;
        };
        if source > u64::from(NOTHING) {
            continue;
        }
        wanted[jack.id as usize] = source as u8;
    }
    apply_slots(app, snap, &wanted, "Routing loaded.");
}

fn send(app: &mut DeskApp, snap: &Snap, cmds: Vec<Cmd>) {
    if cmds.is_empty() || !app.writable(snap) {
        return;
    }
    app.act_all(&cmds);
    app.arm = None;
    app.landed = true;
}

const PC_BASE: u16 = 200;

fn remember_programs(app: &mut DeskApp, snap: &Snap) {
    app.pc_ids.clear();
    for program in &snap.programs {
        if program.role == Role::Playback {
            app.pc_ids.push(program.id.clone());
        }
    }
    for program in &snap.programs {
        if program.role == Role::Capture {
            app.pc_ids.push(program.id.clone());
        }
    }
}

fn pc_pick(ids: &[String], program_id: &str) -> Option<u16> {
    let index = ids.iter().position(|id| id == program_id)?;
    PC_BASE.checked_add(u16::try_from(index).ok()?)
}

fn pc_key(app: &DeskApp, pick: u16) -> Option<String> {
    let index = pick.checked_sub(PC_BASE)? as usize;
    app.pc_ids.get(index).cloned()
}

fn pc_anchor(app: &DeskApp, pick: u16) -> Option<egui::Pos2> {
    let key = pc_key(app, pick)?;
    app.pc_pos.get(&key).copied()
}

fn send_playback(app: &mut DeskApp, snap: &Snap, program_id: &str, slot: u16) {
    if crate::dummy::is_in(slot) {
        deliver_program_to_dummy(app, snap, program_id, slot);
        return;
    }
    let Some(target) = topology::playback_pairs().into_iter().find(|pair| pair.slots.contains(&slot)).map(|pair| pair.id.to_string()) else {
        app.note("That output is not a program route.");
        return;
    };
    let Some(program) = snap.programs.iter().find(|item| item.id == program_id).cloned() else {
        app.note("That program is not in the list.");
        return;
    };
    request_route(app, snap, &program, &target);
}

fn send_capture(app: &mut DeskApp, snap: &Snap, program_id: &str, source: u16) {
    if crate::dummy::is_out(source) {
        deliver_listen(app, snap, program_id, source);
        return;
    }
    let Some(source) = u8::try_from(source).ok() else {
        app.note("That input is not a program route.");
        return;
    };
    let Some(target) = topology::capture_devices()
        .into_iter()
        .find(|device| device.code_len > 0 && device.codes[..usize::from(device.code_len)].contains(&source))
        .map(|device| device.id.to_string())
    else {
        app.note("That input is not a program route.");
        return;
    };
    let Some(program) = snap.programs.iter().find(|item| item.id == program_id).cloned() else {
        app.note("That program is not in the list.");
        return;
    };
    request_route(app, snap, &program, &target);
}

/// Jacks, the route menu, and a click-to-arm all stop here. Send it is the
/// only control that calls the move.
fn request_route(app: &mut DeskApp, snap: &Snap, program: &Program, target: &str) {
    let locked = snap.program_locks.iter().any(|item| item.label == program.label && item.role == program.role.token());
    if locked {
        app.note("That program route is locked.");
        return;
    }
    if !app.writable(snap) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    if let Some(slug) = target.strip_prefix("dummy:") {
        if let Some(host) = snap.dummies.iter().find(|item| item.slug == slug) {
            if let Some(text) = crate::dummy::program_warning(&program.label, &program.target, host, &snap.desk.patch) {
                app.route_ask = Some(super::RouteAsk {
                    text,
                    job: super::RouteJob::Program { id: program.id.clone(), target: target.to_string() },
                });
                app.note("");
                return;
            }
        }
    }
    let custom = output_alias(snap, target);
    if let Some(text) = crate::pipewire::route_warning_named(&program.label, &program.target, target, &snap.desk.patch, &custom) {
        app.route_ask = Some(super::RouteAsk {
            text,
            job: super::RouteJob::Program { id: program.id.clone(), target: target.to_string() },
        });
        app.note("");
        return;
    }
    app.route_ask = None;
    send_program(app, snap, &program.id, target);
}

fn output_alias(snap: &Snap, target: &str) -> String {
    let Some(indexes) = output_name_indexes(target) else { return String::new() };
    let mut names = Vec::new();
    for index in indexes {
        let text = snap
            .desk
            .gear
            .out_names
            .get(index)
            .map(|item| item.trim_matches('\0').trim())
            .unwrap_or("");
        if !text.is_empty() && names.iter().all(|have: &String| have != text) {
            names.push(text.to_string());
        }
    }
    names.join(" / ")
}

fn output_name_indexes(target: &str) -> Option<[usize; 2]> {
    Some(match target {
        "Line1" => [0, 1],
        "Line2" => [2, 3],
        "Line3" => [4, 5],
        "Line4" => [6, 7],
        "Line5" => [8, 9],
        "Headphones1" => [10, 11],
        "Headphones2" => [12, 13],
        _ => return None,
    })
}

fn consent_card(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    let Some(ask) = app.route_ask.clone() else { return };
    let frame = egui::Frame::NONE
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, WARN))
        .inner_margin(10.0)
        .corner_radius(6.0);
    frame.show(ui, |ui| {
        ui.label(egui::RichText::new(&ask.text).font(face("NotoSans", 13.0)).color(WHITE));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if super::chip(ui, "Send it", true, egui::vec2(84.0, 26.0)).clicked() {
                app.route_ask = None;
                commit_job(app, snap, ask.job);
            }
            if super::chip(ui, "Cancel", false, egui::vec2(78.0, 26.0)).clicked() {
                app.route_ask = None;
            }
        });
    });
    ui.add_space(6.0);
}

fn send_program(app: &mut DeskApp, snap: &Snap, program_id: &str, target: &str) {
    if !app.writable(snap) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    let failed = {
        let mut engine = app.hub.lock();
        if let Err(err) = engine.route_program(program_id, target) {
            engine.notice = err;
            true
        } else {
            false
        }
    };
    if !failed {
        app.arm = None;
        app.landed = true;
    }
}

fn unroute_program_cable(app: &mut DeskApp, snap: &Snap, id: &str) {
    let Some(program) = snap.programs.iter().find(|item| item.id == id) else { return };
    let locked = snap.program_locks.iter().any(|item| item.label == program.label && item.role == program.role.token());
    if locked {
        app.note("That program route is locked.");
        return;
    }
    if !app.writable(snap) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    if program.target.is_empty() {
        return;
    }
    let mut engine = app.hub.lock();
    if let Err(err) = engine.unroute_program(id) {
        engine.notice = err;
    }
}

fn paint_program_cables(app: &mut DeskApp, ui: &egui::Ui, snap: &Snap, painter: &egui::Painter, clip: egui::Rect) {
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let mut hits = Vec::new();
    for program in &snap.programs {
        let Some(&origin) = app.pc_pos.get(&program.id) else { continue };
        if program.target.is_empty() {
            continue;
        }
        let playback = program.role == Role::Playback;
        let ends: Vec<egui::Pos2> = if let Some(slug) = program.target.strip_prefix("dummy:") {
            let id = snap.dummies.iter().position(|host| host.slug == slug).and_then(|index| {
                if playback { crate::dummy::in_id(index) } else { crate::dummy::out_id(index) }
            });
            let pos = id.and_then(|id| if playback { app.dst_pos.get(&id) } else { app.src_pos.get(&id) });
            pos.copied().into_iter().collect()
        } else if playback {
            topology::playback_pairs()
                .into_iter()
                .find(|pair| pair.id == program.target)
                .map(|pair| pair.slots.into_iter().filter_map(|slot| app.dst_pos.get(&slot).copied()).collect())
                .unwrap_or_default()
        } else {
            topology::capture_devices()
                .into_iter()
                .find(|device| device.id == program.target)
                .map(|device| {
                    device.codes[..usize::from(device.code_len)]
                        .iter()
                        .filter_map(|code| app.src_pos.get(&u16::from(*code)).copied())
                        .collect()
                })
                .unwrap_or_default()
        };
        for end in ends {
            let (from, to) = if playback { (origin, end) } else { (end, origin) };
            if !clip.contains(from) || !clip.contains(to) {
                continue;
            }
            let near = pointer.is_some_and(|pos| cable_body_distance(from, to, pos) <= 12.0);
            let color = if near { egui::Color32::from_rgb(186, 244, 255) } else { CYAN };
            cable(painter, from, to, color);
            hits.push((program.id.clone(), from, to));
            if near {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    }
    app.pc_cable_hits = hits;
}

fn jack_dot(ui: &mut egui::Ui, hot: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
    let color = if hot { CYAN } else { egui::Color32::from_rgb(170, 174, 178) };
    ui.painter().circle_filled(rect.center(), 7.0, egui::Color32::from_rgb(24, 26, 28));
    ui.painter().circle_stroke(rect.center(), 7.0, egui::Stroke::new(1.6, color));
    ui.painter().circle_filled(rect.center(), 2.2, color);
}

fn row_frame(armed: bool) -> egui::Frame {
    // An unstyled drop zone paints egui's button fill. Match the card unless the jack is armed.
    let fill = if armed { egui::Color32::from_rgb(16, 40, 46) } else { CARD };
    egui::Frame::NONE.inner_margin(1.0).fill(fill)
}

fn face(name: &str, size: f32) -> egui::FontId {
    egui::FontId { size, family: egui::FontFamily::Name(name.into()) }
}

fn paint(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, left: egui::Rect, right: egui::Rect) -> Vec<(u16, egui::Pos2, egui::Pos2)> {
    let clip = left.union(right);
    let painter = ui.painter().with_clip_rect(clip);
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let held = held_slots(snap);
    let mut cables = Vec::new();
    for jack in topology::destinations() {
        if held.contains(&jack.id) {
            continue;
        }
        let source = snap.desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING);
        if source >= NOTHING {
            continue;
        }
        let (Some(from), Some(to)) = (app.src_pos.get(&u16::from(source)), app.dst_pos.get(&jack.id)) else {
            continue;
        };
        if !clip.contains(*from) || !clip.contains(*to) {
            continue;
        }
        let gold = (jack.id == 58 || jack.id == 59) && !snap.loop_text.is_empty();
        let near = pointer.is_some_and(|pos| cable_body_distance(*from, *to, pos) <= 12.0);
        let color = if gold {
            WARN
        } else if near {
            egui::Color32::from_rgb(186, 244, 255)
        } else {
            CYAN
        };
        cable(&painter, *from, *to, color);
        cables.push((jack.id, *from, *to));
        if near {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    app.cable_hits = cables.clone();
    paint_program_cables(app, ui, snap, &painter, clip);
    paint_dummy_cables(app, ui, snap, &painter, clip);
    if let Some(payload) = egui::DragAndDrop::payload::<Pick>(ui.ctx()) {
        let pick_id = payload.id;
        let from_dest = payload.from_dest;
        let from = pc_anchor(app, pick_id).or_else(|| {
            if from_dest { app.dst_pos.get(&pick_id).copied() } else { app.src_pos.get(&pick_id).copied() }
        });
        if let (Some(from), Some(pointer)) = (from, ui.ctx().pointer_interact_pos()) {
            cable(&painter, from, pointer, CYAN);
            app.drag_from = Some(from);
            app.drag_to = Some(pointer);
            app.was_drag = true;
        }
    }
    if let Some(retract) = &app.retract {
        let now = ui.input(|input| input.time);
        let t = (((now - retract.started) / 0.22).clamp(0.0, 1.0)) as f32;
        let tip = retract.to.lerp(retract.from, t * t);
        cable(&painter, retract.from, tip, CYAN);
        if t < 1.0 {
            ui.ctx().request_repaint();
        }
    }
    cables
}

/// The cord itself, not the plug seated in either jack.
fn closest_cable<T: Clone>(cables: &[(T, egui::Pos2, egui::Pos2)], pointer: egui::Pos2) -> Option<(T, f32)> {
    let mut best: Option<(T, f32)> = None;
    for (id, from, to) in cables {
        let dist = cable_body_distance(*from, *to, pointer);
        if dist <= 12.0 && best.as_ref().is_none_or(|(_, have)| dist < *have) {
            best = Some((id.clone(), dist));
        }
    }
    best
}

fn cable_body_distance(from: egui::Pos2, to: egui::Pos2, pointer: egui::Pos2) -> f32 {
    let curve = super::cable_curve(from, to);
    let steps = 32;
    let mut best = f32::MAX;
    for step in 1..steps {
        let t = step as f32 / steps as f32;
        let point = cubic(curve, t);
        if point.distance(from) < 16.0 || point.distance(to) < 16.0 {
            continue;
        }
        best = best.min(point.distance(pointer));
    }
    best
}

fn cubic(points: [egui::Pos2; 4], t: f32) -> egui::Pos2 {
    let u = 1.0 - t;
    let weight = [
        u * u * u,
        3.0 * u * u * t,
        3.0 * u * t * t,
        t * t * t,
    ];
    let mut x = 0.0;
    let mut y = 0.0;
    for index in 0..4 {
        x += points[index].x * weight[index];
        y += points[index].y * weight[index];
    }
    egui::pos2(x, y)
}

fn finish_drag(app: &mut DeskApp, ui: &mut egui::Ui) {
    let dragging = egui::DragAndDrop::payload::<Pick>(ui.ctx()).is_some();
    if app.was_drag && !dragging && !app.landed {
        if let (Some(from), Some(to)) = (app.drag_from, app.drag_to) {
            app.retract = Some(Retract { from, to, started: ui.input(|input| input.time) });
        }
    }
    if !dragging {
        app.was_drag = false;
    }
    if let Some(retract) = &app.retract {
        let now = ui.input(|input| input.time);
        if now - retract.started >= 0.22 {
            app.retract = None;
        }
    }
}

fn pc_programs(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, capture: bool) {
    let want = if capture { Role::Capture } else { Role::Playback };
    let heading = if capture { "RECORDING" } else { "PLAYING" };
    let empty = if capture { "Nothing is recording." } else { "Nothing is playing." };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(heading).font(face("NotoSansBold", 11.0)).color(DIM));
        if super::chip(ui, "Refresh", false, egui::vec2(72.0, 22.0)).clicked() {
            let mut engine = app.hub.lock();
            if let Err(err) = engine.refresh_programs() {
                engine.notice = err;
            }
        }
    });
    ui.add_space(4.0);
    let rows: Vec<Program> = snap.programs.iter().filter(|program| program.role == want).cloned().collect();
    if rows.is_empty() {
        ui.label(egui::RichText::new(empty).font(face("NotoSans", 13.0)).color(DIM));
        return;
    }
    for program in &rows {
        program_row(app, ui, snap, program);
        ui.add_space(4.0);
    }
}

fn program_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, program: &Program) {
    let capture = program.role == Role::Capture;
    let locked = snap.program_locks.iter().any(|item| item.label == program.label && item.role == program.role.token());
    let program_id = program.id.clone();
    let Some(pick_id) = pc_pick(&app.pc_ids, &program_id) else { return };
    let armed = matches!(app.arm, Some(Pick { from_dest, id }) if from_dest == capture && id == pick_id);
    ui.push_id(&program_id, |ui| {
        let frame = row_frame(armed);
        let (_zone, dropped) = ui.dnd_drop_zone::<Pick, _>(frame, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let full = ui.available_width();
                let jack_w = 22.0;
                let lock_w = 18.0;
                let gaps = 18.0;
                let route_w = 112.0_f32.min((full - jack_w - lock_w - gaps - 48.0).max(88.0));
                let name_w = (full - jack_w - lock_w - route_w - gaps).max(36.0);
                if capture {
                    program_jack(app, ui, snap, &program_id, pick_id, capture, locked);
                }
                let (lock_rect, lock) = ui.allocate_exact_size(egui::vec2(lock_w, lock_w), egui::Sense::click());
                super::paint_padlock(ui.painter(), lock_rect, locked);
                let lock = lock
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(if locked { "Locked. This route stays." } else { "Lock this route." });
                if lock.clicked() {
                    let mut engine = app.hub.lock();
                    if let Err(err) = engine.set_program_lock(&program.label, program.role.token(), &program.target, !locked) {
                        engine.notice = err;
                    }
                }
                ui.add_sized(
                    [name_w, 22.0],
                    egui::Label::new(egui::RichText::new(&program.label).font(face("NotoSansReg", 13.0)).color(if armed { CYAN } else { WHITE })).truncate(),
                );
                program_route(app, ui, snap, program, route_w, locked);
                if !capture {
                    program_jack(app, ui, snap, &program_id, pick_id, capture, locked);
                }
            });
        });
        if let Some(payload) = dropped {
            if pc_key(app, payload.id).is_none() {
                if capture && !payload.from_dest {
                    send_capture(app, snap, &program_id, payload.id);
                } else if !capture && payload.from_dest {
                    send_playback(app, snap, &program_id, payload.id);
                }
            }
        }
    });
}

fn program_route(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, program: &Program, route_w: f32, locked: bool) {
    let caption = crate::pipewire::target_label(program.role, &program.target);
    let routed = !program.target.is_empty();
    let accent = if routed { CYAN } else { DIM };
    let (route_rect, route) = ui.allocate_exact_size(egui::vec2(route_w, 24.0), egui::Sense::click());
    ui.painter().rect_stroke(route_rect, 4.0, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    let text_w = (route_rect.width() - 16.0).max(24.0);
    let text_rect = egui::Rect::from_min_size(
        route_rect.left_top() + egui::vec2(6.0, 2.0),
        egui::vec2(text_w, route_rect.height() - 4.0),
    );
    let painter = ui.painter().with_clip_rect(text_rect);
    let galley = painter.layout_no_wrap(caption, face("NotoSansReg", 12.0), accent);
    painter.galley(
        egui::pos2(text_rect.left(), text_rect.center().y - galley.size().y * 0.5),
        galley,
        accent,
    );
    let tip = route_rect.right_center() + egui::vec2(-8.0, 1.0);
    ui.painter().line_segment([tip + egui::vec2(-3.0, -2.0), tip], egui::Stroke::new(1.2, accent));
    ui.painter().line_segment([tip, tip + egui::vec2(3.0, -2.0)], egui::Stroke::new(1.2, accent));
    let route = route.on_hover_cursor(egui::CursorIcon::PointingHand);
    if locked {
        if route.on_hover_text("Locked. This route stays.").clicked() {
            app.note("That program route is locked.");
        }
        return;
    }
    let route = route.on_hover_text("Change this route.");
    let choices = route_choices(snap, program);
    let program_id = program.id.clone();
    let current = program.target.clone();
    let program = program.clone();
    egui::Popup::menu(&route).show(|ui| {
        for (id, label) in &choices {
            let selected = if id == "default" { current.is_empty() } else { current == *id };
            if ui.selectable_label(selected, label).clicked() && !selected {
                if id == "default" {
                    if !app.writable(snap) {
                        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
                    } else {
                        let mut engine = app.hub.lock();
                        if let Err(err) = engine.unroute_program(&program_id) {
                            engine.notice = err;
                        }
                    }
                } else {
                    request_route(app, snap, &program, id);
                }
            }
        }
    });
}

fn route_choices(snap: &Snap, program: &Program) -> Vec<(String, String)> {
    if program.role == Role::Capture {
        return crate::pipewire::capture_choices().into_iter().map(|(id, label)| (id.to_string(), label.to_string())).collect();
    }
    let mut choices: Vec<(String, String)> = crate::pipewire::playback_choices()
        .into_iter()
        .map(|(id, label)| (id.to_string(), label.to_string()))
        .collect();
    let at = choices.iter().position(|(id, _)| id == "desk").unwrap_or(choices.len());
    let mut seen = std::collections::HashSet::new();
    let mut extras = Vec::new();
    for host in &snap.dummies {
        extras.push((format!("dummy:{}", host.slug), host.name.clone()));
    }
    for other in snap.programs.iter().filter(|item| item.role == Role::Capture) {
        if other.label == program.label {
            continue;
        }
        let slug = crate::pipewire::sink_slug(&other.label);
        if slug.is_empty() || slug == "desk" || !seen.insert(slug.clone()) {
            continue;
        }
        extras.push((format!("hear:{slug}"), other.label.clone()));
    }
    for (offset, extra) in extras.into_iter().enumerate() {
        choices.insert(at + offset, extra);
    }
    choices
}

fn program_jack(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, program_id: &str, pick_id: u16, capture: bool, locked: bool) {
    let dragged_id = egui::Id::unique(("pc-jack", program_id));
    let armed = matches!(app.arm, Some(Pick { from_dest, id }) if from_dest == capture && id == pick_id);
    let hot = armed || ui.ctx().is_being_dragged(dragged_id);
    let pick = Pick { from_dest: capture, id: pick_id };
    let drag = ui.dnd_drag_source(dragged_id, pick, |ui| {
        jack_dot(ui, hot);
    });
    if drag.response.rect.intersects(ui.clip_rect()) {
        app.pc_pos.insert(program_id.to_string(), drag.response.rect.center());
    } else {
        app.pc_pos.remove(program_id);
    }
    if drag.response.clicked() && app.cable_click.is_none() && app.pc_cable_click.is_none() {
        if locked {
            app.note("That program route is locked.");
            return;
        }
        let armed_pick = app.arm;
        match armed_pick {
            Some(Pick { from_dest, id }) if from_dest != capture && pc_key(app, id).is_none() => {
                if capture {
                    send_capture(app, snap, program_id, id);
                } else {
                    send_playback(app, snap, program_id, id);
                }
                app.arm = None;
                app.landed = true;
            }
            Some(Pick { from_dest, id }) if from_dest == capture && id == pick_id => app.arm = None,
            _ => app.arm = Some(pick),
        }
    }
}

fn visible_feeds(snap: &Snap, id: u16) -> usize {
    let held = held_slots(snap);
    topology::destinations()
        .into_iter()
        .filter(|jack| !held.contains(&jack.id) && u16::from(snap.desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING)) == id)
        .count()
}

fn held_slots(snap: &Snap) -> Vec<u16> {
    snap.dummies.iter().flat_map(|host| host.outs.iter().map(|out| out.slot)).collect()
}

fn slot_caption(snap: &Snap, slot: u16, source: u8) -> (String, bool) {
    let names: Vec<&str> = snap.dummies.iter().filter(|host| host.outs.iter().any(|out| out.slot == slot)).map(|host| host.name.as_str()).collect();
    if names.is_empty() {
        (topology::source_label(source), source < NOTHING)
    } else {
        (names.join(" + "), true)
    }
}

fn dummy_outs(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    if snap.dummies.is_empty() {
        ui.label(egui::RichText::new("None yet.").font(face("NotoSans", 12.5)).color(DIM));
        return;
    }
    for (index, host) in snap.dummies.iter().enumerate() {
        let Some(id) = crate::dummy::out_id(index) else { continue };
        dummy_out_row(app, ui, snap, &host.slug, &host.name, host.outs.len(), id);
        ui.add_space(4.0);
    }
    ui.add_space(6.0);
}

fn dummy_ins(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap) {
    if snap.dummies.is_empty() {
        ui.label(egui::RichText::new("None yet.").font(face("NotoSans", 12.5)).color(DIM));
        return;
    }
    for (index, host) in snap.dummies.iter().enumerate() {
        let Some(id) = crate::dummy::in_id(index) else { continue };
        let caption = dummy_in_caption(snap, &host.slug);
        dummy_in_row(app, ui, snap, &host.slug, &host.name, &caption, id);
        ui.add_space(4.0);
    }
    ui.add_space(6.0);
}

fn dummy_in_caption(snap: &Snap, slug: &str) -> String {
    let mut names = Vec::new();
    for program in snap.programs.iter().filter(|program| program.role == Role::Playback && program.target == format!("dummy:{slug}")) {
        names.push(program.label.clone());
    }
    if let Some(host) = snap.dummies.iter().find(|host| host.slug == slug) {
        for device_id in &host.captures {
            let label = topology::capture_devices().into_iter().find(|device| device.id == device_id).map(|device| device.label.to_string()).unwrap_or_else(|| device_id.clone());
            names.push(label);
        }
        for from in &host.from_dummies {
            let label = snap.dummies.iter().find(|host| host.slug == *from).map(|host| host.name.clone()).unwrap_or_else(|| from.clone());
            names.push(label);
        }
    }
    match names.len() {
        0 => "Empty".into(),
        1 => names.remove(0),
        n => format!("{n} in"),
    }
}

fn dummy_out_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, slug: &str, name: &str, feeds: usize, id: u16) {
    let pick = Pick { from_dest: false, id };
    let armed = matches!(app.arm, Some(Pick { from_dest: false, id: armed_id }) if armed_id == id);
    let dragged_id = egui::Id::unique(("dummy-out", id));
    let mut clicked = false;
    let mut remove = false;
    let frame = row_frame(armed);
    let slug = slug.to_string();
    let (_zone, dropped) = ui.dnd_drop_zone::<Pick, _>(frame, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let name_w = (ui.available_width() - 120.0).clamp(36.0, 180.0);
            ui.add_sized([name_w, 22.0], egui::Label::new(egui::RichText::new(format!("{name} out")).font(face("NotoSansReg", 13.0)).color(if armed { CYAN } else { WHITE })).truncate());
            if feeds > 0 {
                let (badge, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().circle_filled(badge.center(), 8.0, CYAN);
                ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, feeds.to_string(), face("NotoSansBold", 10.0), egui::Color32::from_rgb(8, 12, 14));
            }
            if super::chip(ui, "Remove", false, egui::vec2(68.0, 22.0)).clicked() {
                remove = true;
            }
            let drag = ui.dnd_drag_source(dragged_id, pick, |ui| jack_dot(ui, armed || ui.ctx().is_being_dragged(dragged_id)));
            if drag.response.rect.intersects(ui.clip_rect()) {
                app.src_pos.insert(id, drag.response.rect.center());
            } else {
                app.src_pos.remove(&id);
            }
            if drag.response.clicked() && app.cable_click.is_none() && app.dummy_cable_click.is_none() {
                clicked = true;
            }
        });
    });
    if remove {
        ask_remove(app, snap, &slug);
    }
    if let Some(payload) = dropped {
        if payload.from_dest {
            if crate::dummy::is_in(payload.id) {
                deliver_dummy_loop(app, snap, id, payload.id);
            } else if pc_key(app, payload.id).is_some() {
                if let Some(program_id) = pc_key(app, payload.id) {
                    deliver_listen(app, snap, &program_id, id);
                }
            } else {
                deliver_dummy_out(app, snap, id, payload.id);
            }
        }
    }
    if clicked {
        on_click(app, snap, false, id);
    }
}

fn dummy_in_row(app: &mut DeskApp, ui: &mut egui::Ui, snap: &Snap, slug: &str, name: &str, caption: &str, id: u16) {
    let pick = Pick { from_dest: true, id };
    let armed = matches!(app.arm, Some(Pick { from_dest: true, id: armed_id }) if armed_id == id);
    let dragged_id = egui::Id::unique(("dummy-in", id));
    let mut clicked = false;
    let mut remove = false;
    let frame = row_frame(armed);
    let slug = slug.to_string();
    let (_zone, dropped) = ui.dnd_drop_zone::<Pick, _>(frame, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let drag = ui.dnd_drag_source(dragged_id, pick, |ui| jack_dot(ui, armed || ui.ctx().is_being_dragged(dragged_id)));
            if drag.response.rect.intersects(ui.clip_rect()) {
                app.dst_pos.insert(id, drag.response.rect.center());
            } else {
                app.dst_pos.remove(&id);
            }
            if drag.response.clicked() && app.cable_click.is_none() && app.dummy_cable_click.is_none() {
                clicked = true;
            }
            let name_w = (ui.available_width() - 180.0).max(72.0);
            ui.add_sized([name_w, 22.0], egui::Label::new(egui::RichText::new(format!("{name} in")).font(face("NotoSansReg", 13.0)).color(WHITE)).truncate());
            ui.add_sized([100.0, 22.0], egui::Label::new(egui::RichText::new(caption).font(face("NotoSansReg", 12.0)).color(if caption == "Empty" { DIM } else { CYAN })).truncate());
            if super::chip(ui, "Remove", false, egui::vec2(68.0, 22.0)).clicked() {
                remove = true;
            }
        });
    });
    if remove {
        ask_remove(app, snap, &slug);
    }
    if let Some(payload) = dropped {
        if !payload.from_dest {
            if let Some(program_id) = pc_key(app, payload.id) {
                deliver_program_to_dummy(app, snap, &program_id, id);
            } else if crate::dummy::is_out(payload.id) {
                deliver_dummy_loop(app, snap, payload.id, id);
            } else {
                deliver_source_to_dummy(app, snap, payload.id, id);
            }
        }
    }
    if clicked {
        on_click(app, snap, true, id);
    }
}

fn click_dummy(app: &mut DeskApp, snap: &Snap, from_dest: bool, id: u16) {
    let Some(pick) = app.arm else {
        app.arm = Some(Pick { from_dest, id });
        return;
    };
    if pick.from_dest == from_dest {
        app.arm = if pick.id == id { None } else { Some(Pick { from_dest, id }) };
        return;
    }
    let (source, dest) = if from_dest { (pick.id, id) } else { (id, pick.id) };
    app.arm = None;
    app.landed = true;
    if let Some(program_id) = pc_key(app, source) {
        if crate::dummy::is_in(dest) {
            deliver_program_to_dummy(app, snap, &program_id, dest);
        } else {
            send_playback(app, snap, &program_id, dest);
        }
        return;
    }
    if let Some(program_id) = pc_key(app, dest) {
        deliver_listen(app, snap, &program_id, source);
        return;
    }
    if crate::dummy::is_out(source) && crate::dummy::is_in(dest) {
        deliver_dummy_loop(app, snap, source, dest);
    } else if crate::dummy::is_out(source) {
        deliver_dummy_out(app, snap, source, dest);
    } else if crate::dummy::is_in(dest) {
        deliver_source_to_dummy(app, snap, source, dest);
    } else if crate::dummy::is_in(source) && !from_dest {
        app.note("Cable Dummy out, not Dummy in, to an output.");
    }
}

fn deliver_program_to_dummy(app: &mut DeskApp, snap: &Snap, program_id: &str, dest: u16) {
    let Some(index) = crate::dummy::index_of(dest) else { return };
    let Some(host) = snap.dummies.get(index) else { return };
    let Some(program) = snap.programs.iter().find(|item| item.id == program_id).cloned() else {
        app.note("That program is not in the list.");
        return;
    };
    if program.role != Role::Playback {
        app.note("That program records audio, so it needs an input.");
        return;
    }
    request_route(app, snap, &program, &format!("dummy:{}", host.slug));
}

fn deliver_listen(app: &mut DeskApp, snap: &Snap, program_id: &str, source: u16) {
    let Some(index) = crate::dummy::index_of(source) else { return };
    let Some(host) = snap.dummies.get(index) else { return };
    let Some(program) = snap.programs.iter().find(|item| item.id == program_id).cloned() else {
        app.note("That program is not in the list.");
        return;
    };
    if program.role != Role::Capture {
        app.note("That program plays audio, so it needs an output.");
        return;
    }
    let locked = snap.program_locks.iter().any(|item| item.label == program.label && item.role == program.role.token());
    if locked {
        app.note("That program route is locked.");
        return;
    }
    app.route_ask = Some(super::RouteAsk {
        text: format!("Send {} to {}? {} will hear {} instead of its current input.", host.name, program.label, program.label, host.name),
        job: super::RouteJob::DummyListen { id: program.id, slug: host.slug.clone() },
    });
    app.note("");
}

fn deliver_dummy_out(app: &mut DeskApp, snap: &Snap, source: u16, slot: u16) {
    let Some(index) = crate::dummy::index_of(source) else { return };
    let Some(host) = snap.dummies.get(index) else { return };
    if !app.writable(snap) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    if crate::dummy::legs(slot, app.stereo).is_err() {
        app.note("That output is not a dummy route.");
        return;
    }
    let alias = slot_alias(snap, slot, app.stereo);
    if let Some(text) = crate::dummy::out_warning(&host.name, slot, app.stereo, &snap.desk.patch, &alias, &host.captures) {
        app.route_ask = Some(super::RouteAsk {
            text,
            job: super::RouteJob::DummyOut { slug: host.slug.clone(), slot, stereo: app.stereo },
        });
        app.note("");
        return;
    }
    let slug = host.slug.clone();
    let stereo = app.stereo;
    if run_dummy(app, |engine| engine.connect_dummy_out(&slug, slot, stereo)) {
        save_dummies(app);
        app.arm = None;
        app.landed = true;
    }
}

fn deliver_source_to_dummy(app: &mut DeskApp, snap: &Snap, source: u16, dest: u16) {
    let Some(index) = crate::dummy::index_of(dest) else { return };
    let Some(host) = snap.dummies.get(index) else { return };
    let plan = match crate::dummy::capture_plan(source, app.stereo) {
        Ok(plan) => plan,
        Err(err) => {
            app.note(err);
            return;
        }
    };
    if let Some(text) = crate::dummy::capture_warning(&host.name, plan.label, &host.outs) {
        app.route_ask = Some(super::RouteAsk {
            text,
            job: super::RouteJob::DummyCapture { slug: host.slug.clone(), source, stereo: app.stereo },
        });
        app.note("");
        return;
    }
    let slug = host.slug.clone();
    let stereo = app.stereo;
    if run_dummy(app, |engine| engine.feed_dummy_from_source(&slug, source, stereo)) {
        save_dummies(app);
        app.arm = None;
        app.landed = true;
    }
}

fn deliver_dummy_loop(app: &mut DeskApp, snap: &Snap, source: u16, dest: u16) {
    if crate::dummy::same_dummy(source, dest) {
        let name = crate::dummy::index_of(source).and_then(|index| snap.dummies.get(index)).map(|host| host.name.as_str()).unwrap_or("Dummy");
        app.note(format!("{name} would loop into itself."));
        return;
    }
    let Some(from_index) = crate::dummy::index_of(source) else { return };
    let Some(into_index) = crate::dummy::index_of(dest) else { return };
    let Some(from) = snap.dummies.get(from_index) else { return };
    let Some(into) = snap.dummies.get(into_index) else { return };
    if crate::dummy::would_cycle(&snap.dummies, &from.slug, &into.slug) {
        app.route_ask = Some(super::RouteAsk {
            text: crate::dummy::loop_warning(&from.name, &into.name),
            job: super::RouteJob::DummyLoop { from_slug: from.slug.clone(), into_slug: into.slug.clone(), stereo: app.stereo },
        });
        app.note("");
        return;
    }
    let from_slug = from.slug.clone();
    let into_slug = into.slug.clone();
    let stereo = app.stereo;
    if run_dummy(app, |engine| engine.feed_dummy_from_dummy(&into_slug, &from_slug, stereo)) {
        save_dummies(app);
        app.arm = None;
        app.landed = true;
    }
}

fn ask_remove(app: &mut DeskApp, snap: &Snap, slug: &str) {
    let Some(host) = snap.dummies.iter().find(|host| host.slug == slug) else { return };
    let programs: Vec<String> = snap
        .programs
        .iter()
        .filter(|program| program.target == format!("dummy:{slug}"))
        .map(|program| program.label.clone())
        .collect();
    if let Some(text) = crate::dummy::remove_warning(&host.name, &host.outs, &snap.desk.patch, &programs) {
        app.route_ask = Some(super::RouteAsk { text, job: super::RouteJob::RemoveDummy { slug: slug.to_string() } });
        app.note("");
        return;
    }
    let slug = slug.to_string();
    if run_dummy(app, |engine| engine.remove_dummy(&slug)) {
        save_dummies(app);
        app.note("");
    }
}

fn create_dummy(app: &mut DeskApp) {
    let name = {
        let mut engine = app.hub.lock();
        match engine.create_dummy() {
            Ok(name) => name,
            Err(err) => {
                engine.notice = err;
                return;
            }
        }
    };
    app.src_bank = SRC_DUMMY;
    app.dst_bank = DST_DUMMY;
    let save_error = app.hub.lock().settings.save().err();
    if let Some(err) = save_error {
        app.note(format!("{name} is ready, but it could not be saved. {err}"));
    } else {
        app.note(format!("{name} is ready. Cable apps or inputs into {name} in, then cable {name} out to the jacks."));
    }
}

fn commit_job(app: &mut DeskApp, snap: &Snap, job: super::RouteJob) {
    if !app.writable(snap) && matches!(job, super::RouteJob::DummyOut { .. }) {
        app.note(format!("{} is read-only on this unit.", crate::APP_NAME));
        return;
    }
    let save = !matches!(job, super::RouteJob::Program { .. } | super::RouteJob::DummyListen { .. });
    let ok = run_dummy(app, |engine| match job.clone() {
        super::RouteJob::Program { id, target } => engine.route_program(&id, &target),
        super::RouteJob::DummyOut { slug, slot, stereo } => engine.connect_dummy_out(&slug, slot, stereo),
        super::RouteJob::DummyCapture { slug, source, stereo } => engine.feed_dummy_from_source(&slug, source, stereo),
        super::RouteJob::DummyLoop { from_slug, into_slug, stereo } => engine.feed_dummy_from_dummy(&into_slug, &from_slug, stereo),
        super::RouteJob::DummyListen { id, slug } => engine.listen_to_dummy(&id, &slug),
        super::RouteJob::RemoveDummy { slug } => engine.remove_dummy(&slug),
    });
    if ok && save {
        save_dummies(app);
    }
    if ok {
        app.arm = None;
        app.landed = true;
    }
}

fn run_dummy(app: &mut DeskApp, body: impl FnOnce(&mut crate::engine::Engine) -> Result<(), String>) -> bool {
    let mut engine = app.hub.lock();
    match body(&mut engine) {
        Ok(()) => true,
        Err(err) => {
            engine.notice = err;
            false
        }
    }
}

fn save_dummies(app: &mut DeskApp) {
    let mut engine = app.hub.lock();
    if let Err(err) = engine.settings.save() {
        engine.notice = format!("The dummy is set, but it could not be saved. {err}");
    }
}

fn slot_alias(snap: &Snap, slot: u16, stereo: bool) -> String {
    let Some(pair) = topology::playback_pairs().into_iter().find(|pair| pair.slots.contains(&slot)) else {
        return String::new();
    };
    let Some(indexes) = output_name_indexes(pair.id) else { return String::new() };
    let chosen = if stereo { vec![indexes[0], indexes[1]] } else if pair.slots[0] == slot { vec![indexes[0]] } else { vec![indexes[1]] };
    let mut names = Vec::new();
    for index in chosen {
        let text = snap.desk.gear.out_names.get(index).map(|item| item.trim_matches('\0').trim()).unwrap_or("");
        if !text.is_empty() && names.iter().all(|have: &String| have != text) {
            names.push(text.to_string());
        }
    }
    names.join(" / ")
}

fn uncable_if_dummy(app: &mut DeskApp, snap: &Snap, slot: u16) -> bool {
    let Some(host) = snap.dummies.iter().find(|host| host.outs.iter().any(|out| out.slot == slot)) else {
        return false;
    };
    let slug = host.slug.clone();
    let stereo = app.stereo;
    if run_dummy(app, |engine| engine.uncable_dummy_out(&slug, slot, stereo)) {
        save_dummies(app);
    }
    true
}

fn pull_dummy_cable(app: &mut DeskApp, snap: &Snap, cable: &super::DummyCable) {
    let _ = snap;
    match cable {
        super::DummyCable::Out { slug, slot } => {
            let slug = slug.clone();
            let slot = *slot;
            let stereo = app.stereo;
            if run_dummy(app, |engine| engine.uncable_dummy_out(&slug, slot, stereo)) {
                save_dummies(app);
            }
        }
        super::DummyCable::Capture { slug, device } => {
            let slug = slug.clone();
            let device = device.clone();
            if run_dummy(app, |engine| engine.drop_dummy_capture(&slug, &device)) {
                save_dummies(app);
            }
        }
        super::DummyCable::Loop { into_slug, from_slug } => {
            let into_slug = into_slug.clone();
            let from_slug = from_slug.clone();
            if run_dummy(app, |engine| engine.drop_dummy_loop(&into_slug, &from_slug)) {
                save_dummies(app);
            }
        }
    }
}

fn paint_dummy_cables(app: &mut DeskApp, ui: &egui::Ui, snap: &Snap, painter: &egui::Painter, clip: egui::Rect) {
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let mut hits = Vec::new();
    for (index, host) in snap.dummies.iter().enumerate() {
        let Some(out_id) = crate::dummy::out_id(index) else { continue };
        let Some(in_id) = crate::dummy::in_id(index) else { continue };
        let from_out = app.src_pos.get(&out_id).copied();
        for out in &host.outs {
            let (Some(from), Some(to)) = (from_out, app.dst_pos.get(&out.slot).copied()) else { continue };
            remember_dummy(app, ui, painter, clip, pointer, &mut hits, super::DummyCable::Out { slug: host.slug.clone(), slot: out.slot }, from, to);
        }
        if let Some(into_at) = app.dst_pos.get(&in_id).copied() {
            for device_id in &host.captures {
                let Some(device) = topology::capture_devices().into_iter().find(|device| device.id == device_id) else { continue };
                for code in &device.codes[..usize::from(device.code_len)] {
                    let Some(from) = app.src_pos.get(&u16::from(*code)).copied() else { continue };
                    remember_dummy(
                        app,
                        ui,
                        painter,
                        clip,
                        pointer,
                        &mut hits,
                        super::DummyCable::Capture { slug: host.slug.clone(), device: device.id.to_string() },
                        from,
                        into_at,
                    );
                }
            }
            for from_slug in &host.from_dummies {
                let Some(from_index) = snap.dummies.iter().position(|item| item.slug == *from_slug) else { continue };
                let Some(from_id) = crate::dummy::out_id(from_index) else { continue };
                let Some(from) = app.src_pos.get(&from_id).copied() else { continue };
                remember_dummy(
                    app,
                    ui,
                    painter,
                    clip,
                    pointer,
                    &mut hits,
                    super::DummyCable::Loop { into_slug: host.slug.clone(), from_slug: from_slug.clone() },
                    from,
                    into_at,
                );
            }
        }
    }
    app.dummy_cable_hits = hits;
}

fn remember_dummy(
    app: &mut DeskApp,
    ui: &egui::Ui,
    painter: &egui::Painter,
    clip: egui::Rect,
    pointer: Option<egui::Pos2>,
    hits: &mut Vec<(super::DummyCable, egui::Pos2, egui::Pos2)>,
    cable_id: super::DummyCable,
    from: egui::Pos2,
    to: egui::Pos2,
) {
    let _ = app;
    if !clip.contains(from) || !clip.contains(to) {
        return;
    }
    let near = pointer.is_some_and(|pos| cable_body_distance(from, to, pos) <= 12.0);
    let color = if near { egui::Color32::from_rgb(186, 244, 255) } else { CYAN };
    cable(painter, from, to, color);
    hits.push((cable_id, from, to));
    if near {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

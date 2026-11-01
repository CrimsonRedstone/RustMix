//! A dummy is a mix on this PC, not a jack on the 828.
//! Apps and inputs that already reach the computer go into Dummy in.
//! Dummy out can feed one jack, or several, including a single side of a pair.
//! Nothing here writes the device. The engine links first, then patches.

use crate::settings::{DummyHost, DummyOut};
use crate::topology::{self, NOTHING};

/// Pick ids for dummy jacks. Program picks stay below this.
pub const BASE: u16 = 400;
pub const LIMIT: usize = 8;

pub fn out_id(index: usize) -> Option<u16> {
    (index < LIMIT).then_some(BASE + (index as u16) * 2)
}

pub fn in_id(index: usize) -> Option<u16> {
    (index < LIMIT).then_some(BASE + (index as u16) * 2 + 1)
}

pub fn is_out(id: u16) -> bool {
    index_of(id).is_some() && (id - BASE) % 2 == 0
}

pub fn is_in(id: u16) -> bool {
    index_of(id).is_some() && (id - BASE) % 2 == 1
}

pub fn index_of(id: u16) -> Option<usize> {
    let offset = id.checked_sub(BASE)? as usize;
    (offset < LIMIT * 2).then_some(offset / 2)
}

pub fn sink_name(slug: &str) -> String {
    format!("crimson_dummy_{slug}")
}

pub fn escaped_description(name: &str) -> String {
    name.replace(' ', "\\ ")
}

pub fn label_from_slug(slug: &str) -> String {
    slug.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn next_host(existing: &[DummyHost]) -> Result<DummyHost, String> {
    if existing.len() >= LIMIT {
        return Err("Eight dummies is the limit.".into());
    }
    if !existing.iter().any(|host| host.slug == "dummy") {
        return Ok(DummyHost::new("Dummy", "dummy"));
    }
    for number in 2..64 {
        let slug = format!("dummy_{number}");
        if !existing.iter().any(|host| host.slug == slug) {
            return Ok(DummyHost::new(format!("Dummy {number}"), slug));
        }
    }
    Err("Eight dummies is the limit.".into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leg {
    pub slot: u16,
    pub usb: u8,
    pub channel: &'static str,
    pub suffix: &'static str,
    pub pair_id: &'static str,
}

/// One output jack, or that jack and its partner when stereo is on.
/// A second call for another jack does not include the first.
pub fn legs(slot: u16, stereo: bool) -> Result<Vec<Leg>, String> {
    let mut slots = vec![slot];
    if stereo {
        if let Some(partner) = topology::dest_partner(slot) {
            if partner != slot && !slots.contains(&partner) && pair_of(partner).is_some() {
                slots.push(partner);
            }
        }
    }
    let mut legs = Vec::new();
    for slot in slots {
        let pair = pair_of(slot).ok_or_else(|| "That output is not a dummy route.".to_string())?;
        let index = if pair.slots[0] == slot { 0 } else { 1 };
        legs.push(Leg {
            slot,
            usb: pair.usb[index],
            channel: if index == 0 { "FL" } else { "FR" },
            suffix: pair.suffix,
            pair_id: pair.id,
        });
    }
    Ok(legs)
}

fn pair_of(slot: u16) -> Option<topology::PlayPair> {
    topology::playback_pairs().into_iter().find(|pair| pair.slots.contains(&slot))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortLink {
    pub source_port: &'static str,
    pub sink_port: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturePlan {
    pub device_id: &'static str,
    pub label: &'static str,
    pub suffix: &'static str,
    pub links: Vec<PortLink>,
}

pub fn capture_plan(source: u16, stereo: bool) -> Result<CapturePlan, String> {
    let source_u8 = u8::try_from(source).map_err(|_| reach_message())?;
    let device = topology::capture_devices()
        .into_iter()
        .find(|device| device.code_len > 0 && device.codes[..usize::from(device.code_len)].contains(&source_u8))
        .ok_or_else(reach_message)?;
    let mut links = Vec::new();
    push_capture(&mut links, &device, source_u8);
    if stereo {
        let mut added = false;
        if let Some(partner) = topology::source_partner(source) {
            if let Ok(partner) = u8::try_from(partner) {
                if device.codes[..usize::from(device.code_len)].contains(&partner) {
                    push_capture(&mut links, &device, partner);
                    added = true;
                }
            }
        }
        // A mono mic has no right port. Stereo puts it on both sides of the dummy.
        if !added && device.slot_len == 1 {
            links.push(PortLink { source_port: "capture_MONO", sink_port: "playback_FR" });
        }
    }
    Ok(CapturePlan { device_id: device.id, label: device.label, suffix: device.suffix, links })
}

fn push_capture(links: &mut Vec<PortLink>, device: &topology::CapDev, code: u8) {
    if device.slot_len == 1 {
        if !links.iter().any(|link| link.sink_port == "playback_FL") {
            links.push(PortLink { source_port: "capture_MONO", sink_port: "playback_FL" });
        }
        return;
    }
    let index = device.codes.iter().position(|item| *item == code).unwrap_or(0);
    let link = if index == 0 {
        PortLink { source_port: "capture_FL", sink_port: "playback_FL" }
    } else {
        PortLink { source_port: "capture_FR", sink_port: "playback_FR" }
    };
    if !links.iter().any(|have| have.sink_port == link.sink_port) {
        links.push(link);
    }
}

fn reach_message() -> String {
    "That source has to already reach the computer. A dummy can take a mic, a line input, or a program.".into()
}

pub fn same_dummy(source: u16, dest: u16) -> bool {
    is_out(source) && is_in(dest) && index_of(source) == index_of(dest)
}

/// True when `into` can already reach `from`, so adding from → into would cycle.
pub fn would_cycle(hosts: &[DummyHost], from_slug: &str, into_slug: &str) -> bool {
    if from_slug == into_slug {
        return true;
    }
    let mut stack = vec![into_slug.to_string()];
    let mut seen = Vec::new();
    while let Some(slug) = stack.pop() {
        if !seen.iter().any(|have: &String| have == &slug) {
            seen.push(slug.clone());
        } else {
            continue;
        }
        for other in hosts {
            if other.from_dummies.iter().any(|item| item == &slug) && !seen.iter().any(|have| have == &other.slug) {
                if other.slug == from_slug {
                    return true;
                }
                stack.push(other.slug.clone());
            }
        }
    }
    false
}

pub fn out_warning(name: &str, slot: u16, stereo: bool, patch: &[u8], alias: &str, captures: &[String]) -> Option<String> {
    let legs = legs(slot, stereo).ok()?;
    let mut changes = Vec::new();
    let mut heard = false;
    for leg in &legs {
        let have = patch.get(leg.slot as usize).copied().unwrap_or(NOTHING);
        if have != leg.usb && have < NOTHING {
            changes.push(format!("{} is playing {}", jack_phrase(leg.slot), topology::source_label(have)));
        }
        if matches!(leg.pair_id, "Line1" | "Headphones1" | "Headphones2" | "SPDIF1" | "SPDIF2") {
            heard = true;
        }
    }
    let howl = howl_names(captures);
    let alias = alias.trim();
    if changes.is_empty() && !heard && alias.is_empty() && howl.is_empty() {
        return None;
    }
    let place = place_label(&legs);
    let mut text = format!("Send {name} to {place}?");
    if changes.is_empty() {
        text.push_str(&format!(" That output will play {name}."));
    } else {
        text.push(' ');
        text.push_str(&changes.join(". "));
        text.push_str(". Those routes will change.");
    }
    if !alias.is_empty() && !text.contains(alias) {
        text.push_str(&format!(" That output is named {alias}."));
    }
    if !howl.is_empty() {
        let verb = if howl.contains(" and ") { "are" } else { "is" };
        text.push_str(&format!(" {howl} {verb} inside {name}, so this can howl."));
    }
    Some(text)
}

pub fn program_warning(label: &str, current: &str, host: &DummyHost, patch: &[u8]) -> Option<String> {
    let mut places = Vec::new();
    let mut replaces = false;
    for out in &host.outs {
        if out_warning(&host.name, out.slot, false, patch, "", &host.captures).is_some() {
            replaces = true;
        }
        if let Some(phrase) = legs(out.slot, false).ok().and_then(|items| items.into_iter().next()).map(|leg| jack_phrase(leg.slot)) {
            if !places.contains(&phrase) {
                places.push(phrase);
            }
        }
    }
    let howl = howl_names(&host.captures);
    let leaves_main = current == "Line1";
    if !replaces && howl.is_empty() && !leaves_main {
        return None;
    }
    let mut text = format!("Send {label} to {}?", host.name);
    if !places.is_empty() {
        text.push_str(&format!(" {} plays on {}.", host.name, places.join(" and ")));
    }
    if leaves_main {
        text.push_str(&format!(" {label} will leave Main."));
    }
    if !howl.is_empty() {
        let verb = if howl.contains(" and ") { "are" } else { "is" };
        text.push_str(&format!(" {howl} {verb} inside {}, so this can howl.", host.name));
    }
    Some(text)
}

pub fn capture_warning(dummy_name: &str, capture_label: &str, outs: &[DummyOut]) -> Option<String> {
    if outs.is_empty() {
        return None;
    }
    Some(format!("Send {capture_label} into {dummy_name}? {dummy_name} already plays out, so this can howl."))
}

pub fn loop_warning(from_name: &str, into_name: &str) -> String {
    format!("Send {from_name} into {into_name}? They would feed each other, so this can howl.")
}

pub fn remove_warning(name: &str, outs: &[DummyOut], patch: &[u8], programs: &[String]) -> Option<String> {
    let mut backs = Vec::new();
    for out in outs {
        let current = patch.get(out.slot as usize).copied().unwrap_or(NOTHING);
        let usb = legs(out.slot, false).ok().and_then(|items| items.into_iter().next()).map(|leg| leg.usb);
        if usb == Some(current) && out.previous != current {
            let back = if out.previous >= NOTHING { "nothing".to_string() } else { topology::source_label(out.previous) };
            backs.push(format!("{} goes back to {back}", jack_phrase(out.slot)));
        }
    }
    if backs.is_empty() && programs.is_empty() {
        return None;
    }
    let mut text = format!("Remove {name}?");
    if !backs.is_empty() {
        text.push(' ');
        text.push_str(&backs.join(". "));
        text.push('.');
    }
    if !programs.is_empty() {
        text.push_str(&format!(" {} will leave {name}.", programs.join(", ")));
    }
    Some(text)
}

fn howl_names(captures: &[String]) -> String {
    let mut names = Vec::new();
    for id in captures {
        if let Some(device) = topology::capture_devices().into_iter().find(|device| device.id == id) {
            if !names.contains(&device.label.to_string()) {
                names.push(device.label.to_string());
            }
        }
    }
    match names.len() {
        0 => String::new(),
        1 => names.remove(0),
        _ => {
            let last = names.pop().unwrap_or_default();
            format!("{} and {last}", names.join(", "))
        }
    }
}

fn place_label(legs: &[Leg]) -> String {
    if legs.len() == 1 {
        return jack_phrase(legs[0].slot);
    }
    if legs.len() == 2 && legs[0].pair_id == legs[1].pair_id {
        return topology::playback_pairs()
            .into_iter()
            .find(|pair| pair.id == legs[0].pair_id)
            .map(|pair| pair.label.to_string())
            .unwrap_or_else(|| jack_phrase(legs[0].slot));
    }
    legs.iter().map(|leg| jack_phrase(leg.slot)).collect::<Vec<_>>().join(" and ")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_jack_is_only_the_left_and_stereo_adds_the_partner() {
        let left = legs(topology::line_slot(2), false).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].slot, topology::line_slot(2));
        assert_eq!(left[0].usb, 2);
        assert_eq!(left[0].channel, "FL");
        let pair = legs(topology::line_slot(2), true).unwrap();
        assert_eq!(pair.len(), 2);
        assert_eq!(pair[1].slot, topology::line_slot(3));
        assert_eq!(pair[1].usb, 3);
        assert_eq!(pair[1].channel, "FR");
        let only_seven = legs(topology::line_slot(6), false).unwrap();
        assert_eq!(only_seven.len(), 1);
        assert_eq!(only_seven[0].usb, 6);
        assert_eq!(only_seven[0].channel, "FL");
        assert!(only_seven[0].suffix.contains("Line4"));
    }

    #[test]
    fn a_second_destination_is_its_own_leg() {
        let main = legs(topology::line_slot(0), false).unwrap();
        let line = legs(topology::line_slot(2), false).unwrap();
        assert_eq!(main[0].usb, 0);
        assert_eq!(line[0].usb, 2);
        assert_ne!(main[0].slot, line[0].slot);
    }

    #[test]
    fn a_self_link_is_the_same_dummy_and_a_cycle_is_named() {
        let out = out_id(0).unwrap();
        let into = in_id(0).unwrap();
        assert!(same_dummy(out, into));
        assert!(!same_dummy(out, in_id(1).unwrap()));
        let mut hosts = vec![DummyHost::new("Dummy", "dummy"), DummyHost::new("Dummy 2", "dummy_2")];
        assert!(!would_cycle(&hosts, "dummy", "dummy_2"));
        hosts[0].from_dummies.push("dummy_2".into());
        assert!(would_cycle(&hosts, "dummy", "dummy_2"));
        assert!(would_cycle(&hosts, "dummy", "dummy"));
    }

    #[test]
    fn a_live_mix_and_a_mic_inside_ask_before_the_cable() {
        let mut patch = vec![NOTHING; topology::PATCH_LEN];
        assert!(out_warning("Dummy", topology::line_slot(2), false, &patch, "", &[]).is_none());
        patch[topology::line_slot(2) as usize] = 78;
        let live = out_warning("Dummy", topology::line_slot(2), false, &patch, "", &[]).unwrap();
        assert!(live.contains("Line out 3"), "{live}");
        assert!(live.contains("Analog 1 mix"), "{live}");
        assert!(!live.contains("Line out 4"), "{live}");
        let howl = out_warning("Dummy", topology::line_slot(0), false, &patch, "", &["Mic1".into()]).unwrap();
        assert!(howl.contains("Main 1"), "{howl}");
        assert!(howl.contains("Mic 1"), "{howl}");
        assert!(howl.contains("howl"), "{howl}");
        let named = out_warning("Dummy", topology::line_slot(4), false, &vec![NOTHING; topology::PATCH_LEN], "I HEAR THIS", &[]).unwrap();
        assert!(named.contains("I HEAR THIS"), "{named}");
        assert!(named.contains("Line out 5"), "{named}");
    }

    #[test]
    fn a_mic_uses_the_mono_port_and_a_mix_cannot_be_taken() {
        let mic = capture_plan(u16::from(topology::local_source(0)), false).unwrap();
        assert_eq!(mic.device_id, "Mic1");
        assert_eq!(mic.links.len(), 1);
        assert_eq!(mic.links[0].source_port, "capture_MONO");
        assert_eq!(mic.links[0].sink_port, "playback_FL");
        let both = capture_plan(u16::from(topology::local_source(0)), true).unwrap();
        assert_eq!(both.links.len(), 2);
        assert!(both.links.iter().any(|link| link.sink_port == "playback_FR"));
        let mix = capture_plan(78, false);
        assert!(mix.is_err(), "{mix:?}");
        assert!(mix.unwrap_err().contains("reach the computer"));
    }

    #[test]
    fn an_empty_list_names_the_first_dummy() {
        let host = next_host(&[]).unwrap();
        assert_eq!(host.slug, "dummy");
        assert_eq!(sink_name(&host.slug), "crimson_dummy_dummy");
        let again = next_host(&[host]).unwrap();
        assert_eq!(again.name, "Dummy 2");
        assert_eq!(again.slug, "dummy_2");
    }
}

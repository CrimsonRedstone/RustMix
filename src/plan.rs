//! Every device write goes through here. The socket and the window share it.
//! Monitor, phones, and output trims move by one stored step, and only for the
//! person at the window.

use crate::desk::Desk;
use crate::gear::{self, preamp_max};
use crate::guard::{self, View};
use crate::protocol::{
    Write, PROP_A, PROP_AB, PROP_B, PROP_BUS_FADER, PROP_BUS_LINK, PROP_BUS_MUTE, PROP_CLOCK,
    PROP_COMP_ATT, PROP_COMP_BYPASS, PROP_COMP_RATIO, PROP_COMP_REL, PROP_COMP_THR, PROP_EQ_BYPASS_BUS,
    PROP_EQ_BYPASS_IN, PROP_EQ_FREQ_BUS, PROP_EQ_FREQ_IN, PROP_EQ_GAIN_BUS, PROP_EQ_GAIN_IN,
    PROP_EQ_MODE_BUS, PROP_EQ_MODE_IN, PROP_EQ_Q_BUS, PROP_EQ_Q_IN, PROP_EXPANDER, PROP_FADER,
    PROP_FOOT_DOWN, PROP_FOOT_EN, PROP_FOOT_UP, PROP_GATE_ATT, PROP_GATE_BYPASS, PROP_GATE_REL,
    PROP_GATE_THR, PROP_GROUP, PROP_HOST_MIX, PROP_IN_NAME, PROP_LOOP_FIRST, PROP_LOOP_SRC,
    PROP_MAKEUP, PROP_MIDI, PROP_MIX_LINK, PROP_MONO, PROP_MON_MUTE, PROP_MONITOR, PROP_MUTE,
    PROP_NAME, PROP_OPTICAL, PROP_OUT_NAME, PROP_PAD, PROP_PAN, PROP_PATCH, PROP_PHASE, PROP_PHANTOM,
    PROP_PREAMP, PROP_RATE, PROP_REV_DAMP, PROP_REV_DECAY, PROP_REV_PRE, PROP_REV_PRESET,
    PROP_REV_WIDTH, PROP_SOLO, PROP_TALK_DEST, PROP_TALK_DIM, PROP_TALK_EN, PROP_TALK_LATCH,
    PROP_TALK_LEVEL, PROP_TALK_SRC, PROP_TRIM, PROP_WORD,
};
use crate::settings::Locks;
use crate::topology::{cell_index, db_of, linear_of, NOTHING, PHONE_TRIMS, LINE_TRIMS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Person,
    Mcp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Mix {
        bus: u16,
        input: u16,
        gain: Option<f32>,
        db: Option<f32>,
        pan: Option<f32>,
        mute: Option<bool>,
        solo: Option<bool>,
    },
    Bus {
        bus: u16,
        gain: Option<f32>,
        db: Option<f32>,
        mute: Option<bool>,
    },
    Patch { slot: u16, source: u8 },
    InputLink { input: u16, linked: bool },
    BusLink { bus: u16, linked: bool },
    StepMonitor { louder: bool },
    StepPhones { pair: u8, louder: bool },
    StepTrim { which: u8, louder: bool },
    Eq {
        bus: bool,
        channel: u16,
        band: u8,
        on: Option<bool>,
        mode: Option<u8>,
        hz: Option<i32>,
        gain: Option<f32>,
        q: Option<f32>,
    },
    Gate {
        index: u16,
        on: Option<bool>,
        db: Option<f32>,
        attack: Option<i16>,
        release: Option<i16>,
    },
    Comp {
        index: u16,
        on: Option<bool>,
        db: Option<f32>,
        ratio: Option<f32>,
        attack: Option<i16>,
        release: Option<i16>,
        makeup: Option<f32>,
    },
    Reverb {
        decay: Option<u8>,
        damp: Option<u8>,
        pre: Option<u8>,
        width: Option<u8>,
        preset: Option<u8>,
    },
    Phase { index: u16, invert: bool },
    Pad { index: u16, on: bool },
    Phantom { index: u16, on: bool },
    StepPreamp { index: u16, louder: bool },
    MidiThru { on: bool },
    Clock { source: u8 },
    SampleRate { rate: i32 },
    LoopFirst { which: u8 },
    HostMix { mode: u8 },
    WordOut { out: bool },
    Expander { on: bool },
    Optical { index: u16, tos: bool },
    MonMute { on: bool },
    MonMono { on: bool },
    Group { mask: i16 },
    Ab { enable: u8, a: u8, b: u8 },
    LoopSource { left: u8, right: u8 },
    DeviceName { bytes: [u8; 32] },
    ChannelName { output: bool, index: u16, bytes: [u8; 32] },
    FootEnable { on: bool },
    FootKey { up: bool, bytes: [u8; 32] },
    Talk {
        latch: Option<bool>,
        enable: Option<bool>,
        source: Option<u8>,
        dest: Option<i32>,
        level: Option<f32>,
        dim: Option<f32>,
    },
}

pub fn plan(desk: &Desk, locks: &Locks, origin: Origin, writable: bool, cmds: &[Cmd]) -> Result<Vec<Write>, String> {
    if !writable {
        return Err(format!("{} is read-only on this unit.", crate::APP_NAME));
    }
    for cmd in cmds {
        if origin == Origin::Mcp && is_private_level(*cmd) {
            return Err("That control is not available here.".into());
        }
    }
    let mut after = desk.clone();
    let mut writes = Vec::new();
    for cmd in cmds {
        match *cmd {
            Cmd::Mix { bus, input, gain, db, pan, mute, solo } => {
                let index = cell_index(bus, input).ok_or("That channel is outside the mixer.")?;
                if gain.is_some() && db.is_some() {
                    return Err("The mix level was given twice.".into());
                }
                if let Some(linear) = resolve_gain(gain, db)? {
                    if locks.blocks_cell(bus, input) {
                        return Err("That level is locked.".into());
                    }
                    after.apply_write(PROP_FADER, index as u16, &crate::protocol::encode_f824(linear));
                    writes.push(Write::gain(PROP_FADER, index as u16, linear));
                }
                if let Some(pan) = pan {
                    if !(0.0..=1.0).contains(&pan) {
                        return Err("Pan stays between the left and the right.".into());
                    }
                    after.apply_write(PROP_PAN, index as u16, &crate::protocol::encode_f824(pan));
                    writes.push(Write::gain(PROP_PAN, index as u16, pan));
                }
                if let Some(mute) = mute {
                    let byte = if mute { 1 } else { 0 };
                    after.apply_write(PROP_MUTE, index as u16, &[byte]);
                    writes.push(Write::byte(PROP_MUTE, index as u16, byte));
                }
                if let Some(solo) = solo {
                    let byte = if solo { 1 } else { 0 };
                    after.apply_write(PROP_SOLO, index as u16, &[byte]);
                    writes.push(Write::byte(PROP_SOLO, index as u16, byte));
                }
            }
            Cmd::Bus { bus, gain, db, mute } => {
                if bus >= 16 {
                    return Err("That bus is outside the mixer.".into());
                }
                if let Some(linear) = resolve_gain(gain, db)? {
                    if locks.blocks_bus(bus) {
                        return Err("That level is locked.".into());
                    }
                    after.apply_write(PROP_BUS_FADER, bus, &crate::protocol::encode_f824(linear));
                    writes.push(Write::gain(PROP_BUS_FADER, bus, linear));
                }
                if let Some(mute) = mute {
                    let byte = if mute { 1 } else { 0 };
                    after.apply_write(PROP_BUS_MUTE, bus, &[byte]);
                    writes.push(Write::byte(PROP_BUS_MUTE, bus, byte));
                }
            }
            Cmd::Patch { slot, source } => {
                if slot >= 76 {
                    return Err("Mixer inputs are not destinations.".into());
                }
                if source > NOTHING {
                    return Err("That source is not on this unit.".into());
                }
                after.apply_write(PROP_PATCH, slot, &[source]);
                writes.push(Write::byte(PROP_PATCH, slot, source));
            }
            Cmd::InputLink { input, linked } => {
                if input >= 44 || input % 2 == 1 {
                    return Err("The link sits on the first channel of the pair.".into());
                }
                let byte = if linked { 1 } else { 0 };
                after.apply_write(PROP_MIX_LINK, input, &[byte]);
                writes.push(Write::byte(PROP_MIX_LINK, input, byte));
            }
            Cmd::BusLink { bus, linked } => {
                if bus >= 16 || bus % 2 == 1 {
                    return Err("The link sits on the first bus of the pair.".into());
                }
                let byte = if linked { 1 } else { 0 };
                after.apply_write(PROP_BUS_LINK, bus, &[byte]);
                writes.push(Write::byte(PROP_BUS_LINK, bus, byte));
            }
            Cmd::StepMonitor { louder } => {
                if locks.blocks_monitor() {
                    return Err("That level is locked.".into());
                }
                let Some(stored) = desk.monitor else {
                    return Err("That level has not arrived yet.".into());
                };
                if let Some(next) = step_stored(stored, louder) {
                    after.monitor = Some(next);
                    writes.push(Write::byte(PROP_MONITOR, 0, next));
                }
            }
            Cmd::StepPhones { pair, louder } => {
                if pair > 1 {
                    return Err("That headphone output is not on this unit.".into());
                }
                if locks.blocks_phones(pair) {
                    return Err("That level is locked.".into());
                }
                let left = pair as usize * 2;
                for side in 0..2 {
                    let Some(stored) = desk.phones[left + side] else {
                        return Err("That level has not arrived yet.".into());
                    };
                    if let Some(next) = step_stored(stored, louder) {
                        let index = PHONE_TRIMS[left + side];
                        after.phones[left + side] = Some(next);
                        writes.push(Write::byte(PROP_TRIM, index, next));
                    }
                }
            }
            Cmd::StepTrim { which, louder } => {
                if which >= 10 {
                    return Err("That output is not on this unit.".into());
                }
                if locks.blocks_trim(which) {
                    return Err("That level is locked.".into());
                }
                let Some(stored) = desk.trims[which as usize] else {
                    return Err("That level has not arrived yet.".into());
                };
                if let Some(next) = step_stored(stored, louder) {
                    after.trims[which as usize] = Some(next);
                    writes.push(Write::byte(PROP_TRIM, LINE_TRIMS[which as usize], next));
                }
            }
            other => {
                let extra = gear_writes(&after, other)?;
                for write in &extra {
                    after.apply_write(write.prop, write.index, &write.raw);
                }
                writes.extend(extra);
            }
        }
    }
    let verdict = guard::check(loop_view(desk), loop_view(&after));
    if !verdict.ok {
        return Err(verdict.hint);
    }
    Ok(writes)
}

fn is_private_level(cmd: Cmd) -> bool {
    match cmd {
        Cmd::StepMonitor { .. }
        | Cmd::StepPhones { .. }
        | Cmd::StepTrim { .. }
        | Cmd::Phantom { .. }
        | Cmd::Pad { .. }
        | Cmd::StepPreamp { .. }
        | Cmd::FootEnable { .. }
        | Cmd::SampleRate { .. }
        | Cmd::Clock { .. }
        | Cmd::LoopFirst { .. }
        | Cmd::MonMute { .. }
        | Cmd::Group { .. } => true,
        Cmd::Eq { gain, .. } => gain.is_some(),
        Cmd::Comp { makeup, .. } => makeup.is_some(),
        Cmd::Talk { level, dim, enable, .. } => level.is_some() || dim.is_some() || enable.is_some(),
        _ => false,
    }
}

fn resolve_gain(gain: Option<f32>, db: Option<f32>) -> Result<Option<f32>, String> {
    if let Some(db) = db {
        if !(-72.0..=12.0).contains(&db) {
            return Err("The mix level stays between −72 and +12 dB.".into());
        }
        return Ok(Some(linear_of(db)));
    }
    if let Some(gain) = gain {
        if gain == 0.0 {
            return Ok(Some(0.0));
        }
        if !(0.0..=4.0).contains(&gain) || db_of(gain) < -72.0 {
            return Err("The mix level stays between silence and +12 dB.".into());
        }
        return Ok(Some(gain));
    }
    Ok(None)
}

/// Louder is one stored unit down, because stored 0 is the loudest.
fn step_stored(stored: u8, louder: bool) -> Option<u8> {
    if louder {
        stored.checked_sub(1)
    } else if stored >= 100 {
        None
    } else {
        Some(stored + 1)
    }
}

fn gear_writes(desk: &Desk, cmd: Cmd) -> Result<Vec<Write>, String> {
    let mut writes = Vec::new();
    match cmd {
        Cmd::Eq { bus, channel, band, on, mode, hz, gain, q } => {
            let bands: u8 = if bus { 3 } else { 4 };
            let slots: u16 = if bus { 14 } else { 10 };
            if channel >= slots || band >= bands {
                return Err("That EQ band is not on this unit.".into());
            }
            if let Some(mode) = mode {
                let allowed: &[u8] = if band == 0 {
                    &[0, 1, 3]
                } else if band + 1 == bands {
                    &[0, 2]
                } else {
                    &[0]
                };
                if !allowed.contains(&mode) {
                    return Err("That EQ shape is not on this band.".into());
                }
            }
            if let Some(hz) = hz {
                if !(20..=20_000).contains(&hz) {
                    return Err("EQ frequency stays between 20 Hz and 20 kHz.".into());
                }
            }
            if let Some(gain) = gain {
                if !(-20.0..=20.0).contains(&gain) {
                    return Err("EQ gain stays between −20 and +20 dB.".into());
                }
            }
            if let Some(q) = q {
                if !(0.45..=10.0).contains(&q) {
                    return Err("EQ Q stays between 0.45 and 10.".into());
                }
            }
            let index = channel * u16::from(bands) + u16::from(band);
            let (mode_prop, byp_prop, hz_prop, gain_prop, q_prop) = if bus {
                (PROP_EQ_MODE_BUS, PROP_EQ_BYPASS_BUS, PROP_EQ_FREQ_BUS, PROP_EQ_GAIN_BUS, PROP_EQ_Q_BUS)
            } else {
                (PROP_EQ_MODE_IN, PROP_EQ_BYPASS_IN, PROP_EQ_FREQ_IN, PROP_EQ_GAIN_IN, PROP_EQ_Q_IN)
            };
            if let Some(mode) = mode {
                writes.push(Write::byte(mode_prop, index, mode));
            }
            if let Some(on) = on {
                writes.push(Write::byte(byp_prop, index, if on { 0 } else { 1 }));
            }
            if let Some(hz) = hz {
                writes.push(Write::i32(hz_prop, index, hz));
            }
            if let Some(gain) = gain {
                writes.push(Write::gain(gain_prop, index, gain));
            }
            if let Some(q) = q {
                writes.push(Write::gain(q_prop, index, q));
            }
        }
        Cmd::Gate { index, on, db, attack, release } => {
            if index > 1 {
                return Err("The gate is on the mic inputs.".into());
            }
            push_dyn(&mut writes, index, on, db, None, attack, release, None, true)?;
        }
        Cmd::Comp { index, on, db, ratio, attack, release, makeup } => {
            if index > 9 {
                return Err("That compressor is not on this unit.".into());
            }
            push_dyn(&mut writes, index, on, db, ratio, attack, release, makeup, false)?;
        }
        Cmd::Reverb { decay, damp, pre, width, preset } => {
            if let Some(byte) = decay {
                if byte > 100 {
                    return Err("Reverb decay stays within 0 to 100.".into());
                }
                writes.push(Write::byte(PROP_REV_DECAY, 0, byte));
            }
            for (value, prop, label) in [(damp, PROP_REV_DAMP, "damping"), (pre, PROP_REV_PRE, "pre-delay"), (width, PROP_REV_WIDTH, "width")] {
                if let Some(byte) = value {
                    if byte > 100 {
                        return Err(format!("Reverb {label} stays within 0 to 100."));
                    }
                    writes.push(Write::byte(prop, 0, byte));
                }
            }
            if let Some(preset) = preset {
                if preset > 2 {
                    return Err("That reverb preset is not on this unit.".into());
                }
                writes.push(Write::byte(PROP_REV_PRESET, 0, preset));
            }
        }
        Cmd::Phase { index, invert } => {
            if index > 9 {
                return Err("That polarity switch is not on this unit.".into());
            }
            if desk.gear.phase[index as usize].is_none() {
                return Err("That switch has not arrived yet.".into());
            }
            writes.push(Write::byte(PROP_PHASE, index, if invert { 1 } else { 0 }));
        }
        Cmd::Pad { index, on } => {
            if index > 1 || desk.gear.pad[index as usize].is_none() {
                return Err(if index > 1 { "That pad is not on this unit.".into() } else { "That switch has not arrived yet.".into() });
            }
            writes.push(Write::byte(PROP_PAD, index, if on { 1 } else { 0 }));
        }
        Cmd::Phantom { index, on } => {
            if index > 1 || desk.gear.phantom[index as usize].is_none() {
                return Err(if index > 1 { "That phantom switch is not on this unit.".into() } else { "That switch has not arrived yet.".into() });
            }
            writes.push(Write::byte(PROP_PHANTOM, index, if on { 1 } else { 0 }));
        }
        Cmd::StepPreamp { index, louder } => {
            let max = preamp_max(index).ok_or("That preamp is not on this unit.")?;
            let Some(current) = desk.gear.preamp.get(index as usize).copied().flatten() else {
                return Err("That preamp has not arrived yet.".into());
            };
            let next = if louder { current.saturating_add(1).min(max) } else { current.saturating_sub(1) };
            if next != current {
                writes.push(Write::byte(PROP_PREAMP, index, next));
            }
        }
        Cmd::MidiThru { on } => {
            if desk.gear.midi != Some(u8::from(on)) {
                writes.push(Write::byte(PROP_MIDI, 0, u8::from(on)));
            }
        }
        Cmd::Clock { source } => {
            if ![0, 5, 8, 12, 13].contains(&source) {
                return Err("That clock source is not on this unit.".into());
            }
            if desk.gear.clock != Some(source) {
                writes.push(Write::byte(PROP_CLOCK, 0, source));
            }
        }
        Cmd::SampleRate { rate } => {
            if ![44_100, 48_000, 88_200, 96_000, 176_400, 192_000].contains(&rate) {
                return Err("That sample rate is not on this unit.".into());
            }
            if desk.gear.sample_rate != rate {
                writes.push(Write::i32(PROP_RATE, 0, rate));
            }
        }
        Cmd::LoopFirst { which } => {
            if which > 1 {
                return Err("That loopback location is not on this unit.".into());
            }
            if desk.gear.loop_first != Some(which) {
                writes.push(Write::byte(PROP_LOOP_FIRST, 0, which));
            }
        }
        Cmd::HostMix { mode } => {
            if mode > 1 {
                return Err("That USB mix mode is not on this unit.".into());
            }
            writes.push(Write::byte(PROP_HOST_MIX, 0, mode));
        }
        Cmd::WordOut { out } => writes.push(Write::byte(PROP_WORD, 0, if out { 1 } else { 0 })),
        Cmd::Expander { on } => writes.push(Write::byte(PROP_EXPANDER, 0, if on { 1 } else { 0 })),
        Cmd::Optical { index, tos } => {
            if index > 1 {
                return Err("That optical bank is not on this unit.".into());
            }
            writes.push(Write::byte(PROP_OPTICAL, index, if tos { 1 } else { 0 }));
        }
        Cmd::MonMute { on } => writes.push(Write::byte(PROP_MON_MUTE, 0, if on { 1 } else { 0 })),
        Cmd::MonMono { on } => writes.push(Write::byte(PROP_MONO, 0, if on { 1 } else { 0 })),
        Cmd::Group { mask } => writes.push(Write::i16(PROP_GROUP, 0, mask)),
        Cmd::Ab { enable, a, b } => {
            for byte in [enable, a, b] {
                if byte > 1 {
                    return Err("That monitor switch is not on this unit.".into());
                }
            }
            writes.push(Write::byte(PROP_AB, 0, enable));
            writes.push(Write::byte(PROP_A, 0, a));
            writes.push(Write::byte(PROP_B, 0, b));
        }
        Cmd::LoopSource { left, right } => {
            if left > NOTHING || right > NOTHING {
                return Err("That loopback source is not on this unit.".into());
            }
            writes.push(Write::byte(PROP_LOOP_SRC, 0, left));
            writes.push(Write::byte(PROP_LOOP_SRC, 1, right));
        }
        Cmd::DeviceName { bytes } => writes.push(Write::text(PROP_NAME, 0, bytes)),
        Cmd::ChannelName { output, index, bytes } => {
            let limit = if output { 32 } else { 44 };
            if index >= limit {
                return Err("That name is not on this unit.".into());
            }
            writes.push(Write::text(if output { PROP_OUT_NAME } else { PROP_IN_NAME }, index, bytes));
        }
        Cmd::FootEnable { on } => {
            if desk.gear.foot_en != Some(u8::from(on)) {
                writes.push(Write::byte(PROP_FOOT_EN, 0, u8::from(on)));
            }
        }
        Cmd::FootKey { up, bytes } => {
            writes.push(Write::text(if up { PROP_FOOT_UP } else { PROP_FOOT_DOWN }, 0, bytes));
        }
        Cmd::Talk { latch, enable, source, dest, level, dim } => {
            if let Some(on) = latch {
                writes.push(Write::byte(PROP_TALK_LATCH, 0, if on { 1 } else { 0 }));
            }
            if let Some(on) = enable {
                writes.push(Write::byte(PROP_TALK_EN, 0, if on { 1 } else { 0 }));
            }
            if let Some(source) = source {
                if source != 255 && source > 9 {
                    return Err("That talkback source is not on this unit.".into());
                }
                writes.push(Write::byte(PROP_TALK_SRC, 0, source));
            }
            if let Some(dest) = dest {
                writes.push(Write::i32(PROP_TALK_DEST, 0, dest));
            }
            if let Some(db) = level {
                if !(-48.0..=12.0).contains(&db) {
                    return Err("Talkback level stays between −48 and +12 dB.".into());
                }
                writes.push(Write::gain(PROP_TALK_LEVEL, 0, gear::linear_of_db(db)));
            }
            if let Some(db) = dim {
                if !(-48.0..=0.0).contains(&db) {
                    return Err("Talkback dim stays between −48 and 0 dB.".into());
                }
                writes.push(Write::gain(PROP_TALK_DIM, 0, gear::linear_of_db(db)));
            }
        }
        Cmd::Mix { .. }
        | Cmd::Bus { .. }
        | Cmd::Patch { .. }
        | Cmd::InputLink { .. }
        | Cmd::BusLink { .. }
        | Cmd::StepMonitor { .. }
        | Cmd::StepPhones { .. }
        | Cmd::StepTrim { .. } => {}
    }
    Ok(writes)
}

fn push_dyn(
    writes: &mut Vec<Write>,
    index: u16,
    on: Option<bool>,
    db: Option<f32>,
    ratio: Option<f32>,
    attack: Option<i16>,
    release: Option<i16>,
    makeup: Option<f32>,
    gate: bool,
) -> Result<(), String> {
    if let Some(db) = db {
        if !(-72.0..=0.0).contains(&db) {
            return Err("That threshold stays between −72 and 0 dB.".into());
        }
    }
    if let Some(ratio) = ratio {
        if !(1.0..=10.0).contains(&ratio) {
            return Err("The ratio stays between 1 and 10.".into());
        }
    }
    if let Some(attack) = attack {
        if !(1..=100).contains(&attack) {
            return Err("Attack stays between 1 and 100 ms.".into());
        }
    }
    if let Some(release) = release {
        if !(50..=2000).contains(&release) {
            return Err("Release stays between 50 and 2000 ms.".into());
        }
    }
    if let Some(makeup) = makeup {
        if !(-20.0..=20.0).contains(&makeup) {
            return Err("Makeup gain stays between −20 and +20 dB.".into());
        }
    }
    let (byp, thr, att, rel, ratio_prop) = if gate {
        (PROP_GATE_BYPASS, PROP_GATE_THR, PROP_GATE_ATT, PROP_GATE_REL, 0)
    } else {
        (PROP_COMP_BYPASS, PROP_COMP_THR, PROP_COMP_ATT, PROP_COMP_REL, PROP_COMP_RATIO)
    };
    if let Some(on) = on {
        writes.push(Write::byte(byp, index, if on { 0 } else { 1 }));
    }
    if let Some(db) = db {
        writes.push(Write::gain(thr, index, db));
    }
    if let Some(attack) = attack {
        writes.push(Write::i16(att, index, attack));
    }
    if let Some(release) = release {
        writes.push(Write::i16(rel, index, release));
    }
    if let Some(ratio) = ratio {
        writes.push(Write::gain(ratio_prop, index, ratio));
    }
    if let Some(makeup) = makeup {
        writes.push(Write::gain(PROP_MAKEUP, index, gear::linear_of_db(makeup)));
    }
    Ok(())
}

pub fn loop_view(desk: &Desk) -> View<'_> {
    View {
        patch: &desk.patch,
        faders: desk.loop_faders(),
        mutes: if desk.mix_loaded { Some(desk.mutes.as_slice()) } else { None },
    }
}

pub fn loop_text(desk: &Desk) -> String {
    guard::present(loop_view(desk))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (Desk, Locks) {
        (Desk::demo(), Locks::default())
    }

    #[test]
    fn private_levels_and_ranges_are_refused() {
        let (desk, locks) = open();
        let step = [Cmd::StepMonitor { louder: true }];
        assert!(plan(&desk, &locks, Origin::Mcp, true, &step).unwrap_err().contains("not available"));
        assert!(plan(&desk, &locks, Origin::Mcp, true, &[Cmd::StepPhones { pair: 0, louder: false }]).is_err());
        assert!(plan(&desk, &locks, Origin::Mcp, true, &[Cmd::StepTrim { which: 0, louder: true }]).is_err());
        let high = [Cmd::Mix { bus: 0, input: 0, gain: None, db: Some(12.1), pan: None, mute: None, solo: None }];
        assert!(plan(&desk, &locks, Origin::Mcp, true, &high).is_err());
        let low = [Cmd::Mix { bus: 0, input: 0, gain: None, db: Some(-72.1), pan: None, mute: None, solo: None }];
        assert!(plan(&desk, &locks, Origin::Mcp, true, &low).is_err());
        let pan = [Cmd::Mix { bus: 0, input: 0, gain: None, db: None, pan: Some(1.2), mute: None, solo: None }];
        assert!(plan(&desk, &locks, Origin::Person, true, &pan).is_err());
        assert!(plan(&desk, &locks, Origin::Person, true, &[Cmd::Patch { slot: 76, source: 0 }]).unwrap_err().contains("not destinations"));
        assert!(plan(&desk, &locks, Origin::Person, false, &step).unwrap_err().contains("read-only"));
        for hint in [
            plan(&desk, &locks, Origin::Mcp, true, &step).unwrap_err(),
            plan(&desk, &locks, Origin::Person, true, &[Cmd::Patch { slot: 58, source: 58 }]).unwrap_err(),
        ] {
            assert!(!hint.contains("kMainTrim") && !hint.contains("koTrim") && !hint.contains("5011"), "{hint}");
        }
    }

    #[test]
    fn monitor_steps_one_and_locks_hold() {
        let (desk, mut locks) = open();
        let louder = plan(&desk, &locks, Origin::Person, true, &[Cmd::StepMonitor { louder: true }]).unwrap();
        assert_eq!(louder.len(), 1);
        assert_eq!(louder[0].raw, vec![39]);
        assert_eq!(louder[0].prop, PROP_MONITOR);
        let quieter = plan(&desk, &locks, Origin::Person, true, &[Cmd::StepMonitor { louder: false }]).unwrap();
        assert_eq!(quieter[0].raw, vec![41]);
        let mut top = desk.clone();
        top.monitor = Some(0);
        assert!(plan(&top, &locks, Origin::Person, true, &[Cmd::StepMonitor { louder: true }]).unwrap().is_empty());
        top.monitor = Some(100);
        assert!(plan(&top, &locks, Origin::Person, true, &[Cmd::StepMonitor { louder: false }]).unwrap().is_empty());
        locks.monitor = true;
        assert!(plan(&desk, &locks, Origin::Person, true, &[Cmd::StepMonitor { louder: true }]).is_err());
        locks.all = true;
        locks.monitor = false;
        assert!(plan(&desk, &locks, Origin::Person, true, &[Cmd::StepMonitor { louder: true }]).is_err());
        let cell = [Cmd::Mix { bus: 0, input: 2, gain: None, db: Some(-6.0), pan: None, mute: None, solo: None }];
        assert!(plan(&desk, &locks, Origin::Person, true, &cell).is_err());
        locks.all = false;
        locks.cells.push((0, 2));
        assert!(plan(&desk, &locks, Origin::Person, true, &cell).is_err());
        let mute = [Cmd::Mix { bus: 0, input: 2, gain: None, db: None, pan: None, mute: Some(true), solo: None }];
        assert!(plan(&desk, &locks, Origin::Person, true, &mute).is_ok());
    }

    #[test]
    fn one_cell_and_a_loop_blocks_the_fader() {
        let (mut desk, locks) = open();
        let writes = plan(&desk, &locks, Origin::Person, true, &[Cmd::Mix {
            bus: 0, input: 2, gain: None, db: Some(-6.0), pan: Some(0.25), mute: None, solo: None,
        }]).unwrap();
        assert_eq!(writes.len(), 2);
        assert!(writes.iter().all(|write| write.index == 2));
        desk.patch[86] = 58;
        desk.patch[58] = 76;
        if let Some(faders) = desk.faders.as_mut() {
            faders[10] = 0.0;
        }
        let open_path = [Cmd::Mix { bus: 0, input: 10, gain: Some(1.0), db: None, pan: None, mute: None, solo: None }];
        let err = plan(&desk, &locks, Origin::Person, true, &open_path).unwrap_err();
        assert!(err.contains("stays off"), "{err}");
        assert!(plan(&desk, &locks, Origin::Person, true, &[Cmd::Patch { slot: 58, source: NOTHING }]).is_ok());
    }

    #[test]
    fn phones_and_trims_use_their_own_indices() {
        let (desk, locks) = open();
        let writes = plan(&desk, &locks, Origin::Person, true, &[Cmd::StepPhones { pair: 0, louder: true }]).unwrap();
        let indexes: Vec<u16> = writes.iter().map(|write| write.index).collect();
        assert_eq!(indexes, vec![PHONE_TRIMS[0], PHONE_TRIMS[1]]);
        assert!(writes.iter().all(|write| write.raw == vec![39] && write.prop == PROP_TRIM));
        let trim = plan(&desk, &locks, Origin::Person, true, &[Cmd::StepTrim { which: 0, louder: false }]).unwrap();
        assert_eq!(trim[0].index, LINE_TRIMS[0]);
        assert_eq!(trim[0].raw, vec![21]);
    }

    #[test]
    fn the_factory_patch_restores_a_cleared_output() {
        let (mut desk, locks) = open();
        let slot = crate::topology::line_slot(2);
        desk.patch[slot as usize] = NOTHING;
        let factory = crate::desk::Desk::factory_patch();
        let cmds: Vec<Cmd> = crate::topology::destinations()
            .into_iter()
            .filter_map(|jack| {
                let source = factory.get(jack.id as usize).copied().unwrap_or(NOTHING);
                if desk.patch.get(jack.id as usize).copied().unwrap_or(NOTHING) == source {
                    None
                } else {
                    Some(Cmd::Patch { slot: jack.id, source })
                }
            })
            .collect();
        let writes = plan(&desk, &locks, Origin::Person, true, &cmds).unwrap();
        assert!(writes.iter().any(|write| write.index == slot && write.raw == vec![factory[slot as usize]]));
    }

    #[test]
    fn effects_names_and_rates_stay_in_range_and_off_the_hearing_props() {
        let (desk, locks) = open();
        let hz = plan(&desk, &locks, Origin::Person, true, &[Cmd::Eq {
            bus: false, channel: 2, band: 1, on: Some(false), mode: None, hz: Some(1000), gain: None, q: None,
        }]).unwrap();
        assert_eq!(hz.len(), 2);
        assert!(hz.iter().any(|write| write.prop == crate::protocol::PROP_EQ_FREQ_IN && write.index == 9 && write.raw == crate::protocol::encode_i32(1000).to_vec()));
        assert!(hz.iter().all(|write| write.prop != PROP_TRIM && write.prop != PROP_MONITOR));
        let bad = plan(&desk, &locks, Origin::Person, true, &[Cmd::Eq {
            bus: false, channel: 2, band: 1, on: None, mode: None, hz: Some(10), gain: None, q: None,
        }]);
        assert!(bad.is_err());
        let gain = [Cmd::Eq {
            bus: false, channel: 2, band: 0, on: None, mode: None, hz: None, gain: Some(6.0), q: None,
        }];
        assert!(plan(&desk, &locks, Origin::Mcp, true, &gain).unwrap_err().contains("not available"));
        assert!(plan(&desk, &locks, Origin::Mcp, true, &[Cmd::Phantom { index: 0, on: true }]).is_err());
        assert!(plan(&desk, &locks, Origin::Mcp, true, &[Cmd::StepPreamp { index: 0, louder: true }]).is_err());
        assert!(plan(&desk, &locks, Origin::Mcp, true, &[Cmd::Comp {
            index: 2, on: None, db: None, ratio: None, attack: None, release: None, makeup: Some(3.0),
        }]).is_err());
        assert!(plan(&desk, &locks, Origin::Mcp, true, &[Cmd::Talk {
            latch: None, enable: Some(true), source: None, dest: None, level: None, dim: None,
        }]).is_err());
        let same = plan(&desk, &locks, Origin::Person, true, &[Cmd::SampleRate { rate: 48_000 }]).unwrap();
        assert!(same.is_empty(), "picking the current sample rate must not reset the unit");
        let rate = plan(&desk, &locks, Origin::Person, true, &[Cmd::SampleRate { rate: 44_100 }]).unwrap();
        assert_eq!(rate.len(), 1);
        assert_eq!(rate[0].prop, crate::protocol::PROP_RATE);
        assert!(rate.iter().all(|write| write.prop != PROP_TRIM && write.prop != PROP_MONITOR && write.prop != 5000 && write.prop != 5011));
        let name = plan(&desk, &locks, Origin::Person, true, &[Cmd::ChannelName {
            output: true, index: 5, bytes: crate::protocol::text_bytes("TEMP"),
        }]).unwrap();
        assert_eq!(name[0].raw.len(), 32);
        assert_eq!(&name[0].raw[..4], b"TEMP");
        assert_eq!(name[0].prop, crate::protocol::PROP_OUT_NAME);
        let phase = plan(&desk, &locks, Origin::Mcp, true, &[Cmd::Phase { index: 4, invert: true }]).unwrap();
        assert_eq!(phase[0].prop, crate::protocol::PROP_PHASE);
        assert!(plan(&desk, &locks, Origin::Person, true, &[Cmd::Phase { index: 4, invert: true }]).is_ok());
    }
}

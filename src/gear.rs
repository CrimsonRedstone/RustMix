//! Front panel, effects, names, and meters. Reports land here.
//! Monitor trim and output trim stay on the desk, not in this module.

use crate::protocol::{
    decode_f824, decode_i16, decode_i32, decode_str, PROP_A, PROP_AB, PROP_B, PROP_CLOCK,
    PROP_COMP_ATT, PROP_COMP_BYPASS, PROP_COMP_RATIO, PROP_COMP_REL, PROP_COMP_THR, PROP_EQ_BYPASS_BUS,
    PROP_EQ_BYPASS_IN, PROP_EQ_FREQ_BUS, PROP_EQ_FREQ_IN, PROP_EQ_GAIN_BUS, PROP_EQ_GAIN_IN,
    PROP_EQ_MODE_BUS, PROP_EQ_MODE_IN, PROP_EQ_Q_BUS, PROP_EQ_Q_IN, PROP_EXPANDER, PROP_FIRMWARE,
    PROP_FOOT_DOWN, PROP_FOOT_EN, PROP_FOOT_UP, PROP_GATE_ATT, PROP_GATE_BYPASS, PROP_GATE_REL,
    PROP_GATE_THR, PROP_GROUP, PROP_HOST_MIX, PROP_IN_NAME, PROP_LOOP_FIRST, PROP_LOOP_SRC,
    PROP_MAKEUP, PROP_METERS, PROP_MIDI, PROP_MONO, PROP_MON_MUTE, PROP_OPTICAL, PROP_OUT_NAME,
    PROP_PAD, PROP_PHASE, PROP_PHANTOM, PROP_PREAMP, PROP_RATE, PROP_REV_DAMP, PROP_REV_DECAY,
    PROP_REV_PRE, PROP_REV_PRESET, PROP_REV_WIDTH, PROP_TALK_DEST, PROP_TALK_DIM, PROP_TALK_EN,
    PROP_TALK_LATCH, PROP_TALK_LEVEL, PROP_TALK_SRC, PROP_WORD,
};

pub const IN_EQ: usize = 40;
pub const BUS_EQ: usize = 42;
pub const IN_CH: usize = 10;
pub const BUS_SLOTS: usize = 14;

#[derive(Clone, Debug)]
pub struct Gear {
    pub firmware: String,
    pub sample_rate: i32,
    pub clock: Option<u8>,
    pub midi: Option<u8>,
    pub loop_first: Option<u8>,
    pub host_mix: Option<u8>,
    pub in_names: [String; 44],
    pub out_names: [String; 32],
    pub optical: [Option<u8>; 2],
    pub word: Option<u8>,
    pub expander: Option<u8>,
    pub phase: [Option<u8>; 10],
    pub pad: [Option<u8>; 2],
    pub phantom: [Option<u8>; 2],
    pub preamp: [Option<u8>; 12],
    pub mon_mute: Option<u8>,
    pub mono: Option<u8>,
    pub ab: Option<u8>,
    pub a_on: Option<u8>,
    pub b_on: Option<u8>,
    pub main_group: Option<i16>,
    pub loop_l: Option<u8>,
    pub loop_r: Option<u8>,
    pub foot_en: Option<u8>,
    pub foot_down: String,
    pub foot_up: String,
    pub talk_latch: Option<u8>,
    pub talk_en: Option<u8>,
    pub talk_src: Option<u8>,
    pub talk_level: Option<f32>,
    pub talk_dim: Option<f32>,
    pub talk_dest: Option<i32>,
    pub meters: Vec<u8>,
    pub in_mode: [u8; IN_EQ],
    pub in_bypass: [u8; IN_EQ],
    pub in_hz: [i32; IN_EQ],
    pub in_gain: [f32; IN_EQ],
    pub in_q: [f32; IN_EQ],
    pub bus_mode: [u8; BUS_EQ],
    pub bus_bypass: [u8; BUS_EQ],
    pub bus_hz: [i32; BUS_EQ],
    pub bus_gain: [f32; BUS_EQ],
    pub bus_q: [f32; BUS_EQ],
    pub gate_byp: [u8; 2],
    pub gate_thr: [f32; 2],
    pub gate_att: [i16; 2],
    pub gate_rel: [i16; 2],
    pub comp_byp: [u8; 10],
    pub comp_thr: [f32; 10],
    pub comp_att: [i16; 10],
    pub comp_rel: [i16; 10],
    pub comp_ratio: [f32; 10],
    pub comp_makeup: [f32; 10],
    pub rev_decay: u8,
    pub rev_damp: u8,
    pub rev_pre: u8,
    pub rev_width: u8,
    pub rev_preset: u8,
}

fn blank_names<const N: usize>() -> [String; N] {
    std::array::from_fn(|_| String::new())
}

impl Gear {
    pub fn blank() -> Self {
        let mut gear = Self {
            firmware: String::new(),
            sample_rate: 0,
            clock: None,
            midi: None,
            loop_first: None,
            host_mix: None,
            in_names: blank_names(),
            out_names: blank_names(),
            optical: [None; 2],
            word: None,
            expander: None,
            phase: [None; 10],
            pad: [None; 2],
            phantom: [None; 2],
            preamp: [None; 12],
            mon_mute: None,
            mono: None,
            ab: None,
            a_on: None,
            b_on: None,
            main_group: None,
            loop_l: None,
            loop_r: None,
            foot_en: None,
            foot_down: String::new(),
            foot_up: String::new(),
            talk_latch: None,
            talk_en: None,
            talk_src: None,
            talk_level: None,
            talk_dim: None,
            talk_dest: None,
            meters: Vec::new(),
            in_mode: [0; IN_EQ],
            in_bypass: [1; IN_EQ],
            in_hz: [80; IN_EQ],
            in_gain: [0.0; IN_EQ],
            in_q: [1.0; IN_EQ],
            bus_mode: [0; BUS_EQ],
            bus_bypass: [1; BUS_EQ],
            bus_hz: [40; BUS_EQ],
            bus_gain: [0.0; BUS_EQ],
            bus_q: [1.0; BUS_EQ],
            gate_byp: [1; 2],
            gate_thr: [-46.0; 2],
            gate_att: [10; 2],
            gate_rel: [1000; 2],
            comp_byp: [1; 10],
            comp_thr: [-10.0; 10],
            comp_att: [10; 10],
            comp_rel: [1000; 10],
            comp_ratio: [2.0; 10],
            comp_makeup: [1.0; 10],
            rev_decay: 55,
            rev_damp: 50,
            rev_pre: 25,
            rev_width: 50,
            rev_preset: 1,
        };
        for ch in 0..IN_CH {
            let base = ch * 4;
            gear.in_hz[base] = 80;
            gear.in_hz[base + 1] = 400;
            gear.in_hz[base + 2] = 2000;
            gear.in_hz[base + 3] = 8000;
            gear.in_mode[base + 3] = 2;
        }
        for slot in 0..BUS_SLOTS {
            let base = slot * 3;
            gear.bus_mode[base] = 3;
            gear.bus_hz[base] = 40;
            gear.bus_hz[base + 1] = 1000;
            gear.bus_mode[base + 2] = 2;
            gear.bus_hz[base + 2] = 8000;
        }
        gear
    }

    /// Demo front panel. Effects match the on-screen plate so opening the mixer does not jump.
    pub fn quiet() -> Self {
        let mut gear = Self::blank();
        gear.sample_rate = 48_000;
        gear.clock = Some(0);
        gear.midi = Some(0);
        gear.loop_first = Some(0);
        gear.host_mix = Some(0);
        gear.optical = [Some(0); 2];
        gear.word = Some(0);
        gear.expander = Some(0);
        gear.phase = [Some(0); 10];
        gear.pad = [Some(0); 2];
        gear.phantom = [Some(0); 2];
        gear.preamp = [Some(0); 12];
        gear.mon_mute = Some(0);
        gear.mono = Some(0);
        gear.ab = Some(0);
        gear.a_on = Some(0);
        gear.b_on = Some(0);
        gear.main_group = Some(0);
        gear.loop_l = Some(76);
        gear.loop_r = Some(77);
        gear.foot_en = Some(0);
        gear.talk_latch = Some(1);
        gear.talk_en = Some(0);
        gear.talk_src = Some(255);
        gear.talk_level = Some(0.0);
        gear.talk_dim = Some(1.0);
        gear.talk_dest = Some(0);
        gear
    }

    pub fn meter_db(&self, index: usize) -> Option<f32> {
        let byte = *self.meters.get(index)?;
        if byte >= 255 {
            None
        } else {
            Some(-(byte as f32) * 0.5)
        }
    }

    pub fn apply(&mut self, prop: u16, index: u16, raw: &[u8]) {
        let at = index as usize;
        match prop {
            PROP_FIRMWARE => {
                let text = decode_str(raw);
                if index == 0 || (index == 1 && self.firmware.is_empty()) {
                    self.firmware = text;
                }
            }
            PROP_RATE => {
                if index == 0 {
                    if let Some(rate) = decode_i32(raw) {
                        self.sample_rate = rate;
                    }
                }
            }
            PROP_CLOCK => put_opt(&mut self.clock, raw),
            PROP_MIDI => put_opt(&mut self.midi, raw),
            PROP_LOOP_FIRST => put_opt(&mut self.loop_first, raw),
            PROP_HOST_MIX => put_opt(&mut self.host_mix, raw),
            PROP_IN_NAME => put_name(&mut self.in_names, at, raw),
            PROP_OUT_NAME => put_name(&mut self.out_names, at, raw),
            PROP_OPTICAL => {
                if at < 2 {
                    put_opt(&mut self.optical[at], raw);
                }
            }
            PROP_WORD => put_opt(&mut self.word, raw),
            PROP_EXPANDER => put_opt(&mut self.expander, raw),
            PROP_PHASE => put_at(&mut self.phase, at, raw),
            PROP_PAD => put_at(&mut self.pad, at, raw),
            PROP_PHANTOM => put_at(&mut self.phantom, at, raw),
            PROP_PREAMP => put_at(&mut self.preamp, at, raw),
            PROP_MON_MUTE => put_opt(&mut self.mon_mute, raw),
            PROP_MONO => put_opt(&mut self.mono, raw),
            PROP_AB => put_opt(&mut self.ab, raw),
            PROP_A => put_opt(&mut self.a_on, raw),
            PROP_B => put_opt(&mut self.b_on, raw),
            PROP_GROUP => {
                if let Some(mask) = decode_i16(raw) {
                    self.main_group = Some(mask);
                }
            }
            PROP_LOOP_SRC => match index {
                0 => put_opt(&mut self.loop_l, raw),
                1 => put_opt(&mut self.loop_r, raw),
                _ => {}
            },
            PROP_FOOT_EN => put_opt(&mut self.foot_en, raw),
            PROP_FOOT_DOWN => self.foot_down = decode_str(raw),
            PROP_FOOT_UP => self.foot_up = decode_str(raw),
            PROP_TALK_LATCH => put_opt(&mut self.talk_latch, raw),
            PROP_TALK_EN => put_opt(&mut self.talk_en, raw),
            PROP_TALK_SRC => put_opt(&mut self.talk_src, raw),
            PROP_TALK_LEVEL => {
                if let Some(value) = decode_f824(raw) {
                    self.talk_level = Some(value);
                }
            }
            PROP_TALK_DIM => {
                if let Some(value) = decode_f824(raw) {
                    self.talk_dim = Some(value);
                }
            }
            PROP_TALK_DEST => {
                if let Some(mask) = decode_i32(raw) {
                    self.talk_dest = Some(mask);
                }
            }
            PROP_METERS => self.note_meter(index, raw),
            PROP_EQ_MODE_IN => put_byte(&mut self.in_mode, at, raw),
            PROP_EQ_BYPASS_IN => put_byte(&mut self.in_bypass, at, raw),
            PROP_EQ_FREQ_IN => put_i32(&mut self.in_hz, at, raw),
            PROP_EQ_GAIN_IN => put_f32(&mut self.in_gain, at, raw),
            PROP_EQ_Q_IN => put_f32(&mut self.in_q, at, raw),
            PROP_EQ_MODE_BUS => put_byte(&mut self.bus_mode, at, raw),
            PROP_EQ_BYPASS_BUS => put_byte(&mut self.bus_bypass, at, raw),
            PROP_EQ_FREQ_BUS => put_i32(&mut self.bus_hz, at, raw),
            PROP_EQ_GAIN_BUS => put_f32(&mut self.bus_gain, at, raw),
            PROP_EQ_Q_BUS => put_f32(&mut self.bus_q, at, raw),
            PROP_GATE_BYPASS => put_byte(&mut self.gate_byp, at, raw),
            PROP_GATE_THR => put_f32(&mut self.gate_thr, at, raw),
            PROP_GATE_ATT => put_i16(&mut self.gate_att, at, raw),
            PROP_GATE_REL => put_i16(&mut self.gate_rel, at, raw),
            PROP_COMP_BYPASS => put_byte(&mut self.comp_byp, at, raw),
            PROP_COMP_THR => put_f32(&mut self.comp_thr, at, raw),
            PROP_COMP_ATT => put_i16(&mut self.comp_att, at, raw),
            PROP_COMP_REL => put_i16(&mut self.comp_rel, at, raw),
            PROP_COMP_RATIO => put_f32(&mut self.comp_ratio, at, raw),
            PROP_MAKEUP => put_f32(&mut self.comp_makeup, at, raw),
            PROP_REV_DECAY => put_u8(&mut self.rev_decay, raw),
            PROP_REV_DAMP => put_u8(&mut self.rev_damp, raw),
            PROP_REV_PRE => put_u8(&mut self.rev_pre, raw),
            PROP_REV_WIDTH => put_u8(&mut self.rev_width, raw),
            PROP_REV_PRESET => put_u8(&mut self.rev_preset, raw),
            _ => {}
        }
    }

    fn note_meter(&mut self, index: u16, raw: &[u8]) {
        if full_meter(index, raw) {
            self.meters.clear();
            self.meters.extend_from_slice(raw);
            return;
        }
        let Some(&byte) = raw.first() else { return };
        let at = index as usize;
        if self.meters.len() <= at {
            self.meters.resize(at + 1, 255);
        }
        self.meters[at] = byte;
    }
}

/// A whole peak picture. Later bytes of the same picture replace it.
/// A one-byte update is a single lamp and has to be kept in order.
pub fn full_meter(index: u16, raw: &[u8]) -> bool {
    index == 0 && raw.len() > 1
}

fn put_opt(slot: &mut Option<u8>, raw: &[u8]) {
    if let Some(&byte) = raw.first() {
        *slot = Some(byte);
    }
}

fn put_at(slots: &mut [Option<u8>], index: usize, raw: &[u8]) {
    if let Some(slot) = slots.get_mut(index) {
        put_opt(slot, raw);
    }
}

fn put_name(names: &mut [String], index: usize, raw: &[u8]) {
    if let Some(slot) = names.get_mut(index) {
        *slot = decode_str(raw);
    }
}

fn put_byte(slots: &mut [u8], index: usize, raw: &[u8]) {
    if let (Some(slot), Some(&byte)) = (slots.get_mut(index), raw.first()) {
        *slot = byte;
    }
}

fn put_i32(slots: &mut [i32], index: usize, raw: &[u8]) {
    if let (Some(slot), Some(value)) = (slots.get_mut(index), decode_i32(raw)) {
        *slot = value;
    }
}

fn put_i16(slots: &mut [i16], index: usize, raw: &[u8]) {
    if let (Some(slot), Some(value)) = (slots.get_mut(index), decode_i16(raw)) {
        *slot = value;
    }
}

fn put_f32(slots: &mut [f32], index: usize, raw: &[u8]) {
    if let (Some(slot), Some(value)) = (slots.get_mut(index), decode_f824(raw)) {
        *slot = value;
    }
}

fn put_u8(slot: &mut u8, raw: &[u8]) {
    if let Some(&byte) = raw.first() {
        *slot = byte;
    }
}

pub fn linear_of_db(db: f32) -> f32 {
    if db <= -48.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

pub fn db_of_linear(linear: f32) -> f32 {
    if linear <= 1.0e-6 {
        -48.0
    } else {
        20.0 * linear.log10()
    }
}

pub fn preamp_max(index: u16) -> Option<u8> {
    match index {
        0 | 1 => Some(65),
        2..=11 => Some(20),
        _ => None,
    }
}

/// Bus EQ slot. Reverb shares the last slot. A linked pair uses the even bus.
pub fn bus_eq_slot(bus: u16, linked: bool) -> Option<u16> {
    if bus >= 16 {
        return None;
    }
    let slot = if bus >= 14 {
        13
    } else if linked {
        bus & !1
    } else {
        bus
    };
    Some(slot)
}

pub fn flag(byte: Option<u8>) -> bool {
    byte.unwrap_or(0) != 0
}

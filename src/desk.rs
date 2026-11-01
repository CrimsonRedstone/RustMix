//! The desk the window and the socket both read. Device reports replace it;
//! they are never mixed back onto a demo the user was editing.

use crate::gear::Gear;
use crate::protocol::{
    decode_f824, decode_i16, decode_str, PROP_API, PROP_BUS_FADER, PROP_BUS_LINK, PROP_BUS_MUTE,
    PROP_FADER, PROP_MIX_LINK, PROP_MODEL, PROP_MONITOR, PROP_MUTE, PROP_NAME, PROP_PAN,
    PROP_PATCH, PROP_SOLO, PROP_TRIM, PROP_UID,
};
use crate::topology::{
    cell_index, BUSES, CELLS, INPUTS, NOTHING, PATCH_LEN, PHONE_TRIMS,
};

#[derive(Clone, Debug)]
pub struct Desk {
    pub model: String,
    pub api: i16,
    pub name: String,
    pub uid: String,
    pub saw_model: bool,
    pub saw_api: bool,
    pub saw_name: bool,
    pub saw_uid: bool,
    pub patch: Vec<u8>,
    pub faders: Option<Vec<f32>>,
    pub pans: Vec<f32>,
    pub solos: Vec<u8>,
    pub mutes: Vec<u8>,
    pub mix_loaded: bool,
    fader_seen: [u64; 11],
    pub in_link: Vec<u8>,
    pub bus_link: Vec<u8>,
    pub bus_fader: Vec<f32>,
    pub bus_mute: Vec<u8>,
    pub monitor: Option<u8>,
    pub phones: [Option<u8>; 4],
    pub trims: [Option<u8>; 10],
    pub gear: Gear,
}

impl Desk {
    pub fn blank() -> Self {
        Self {
            model: String::new(),
            api: 0,
            name: String::new(),
            uid: String::new(),
            saw_model: false,
            saw_api: false,
            saw_name: false,
            saw_uid: false,
            patch: vec![NOTHING; PATCH_LEN],
            faders: None,
            pans: vec![0.5; CELLS],
            solos: vec![0; CELLS],
            mutes: vec![0; CELLS],
            mix_loaded: false,
            fader_seen: [0; 11],
            in_link: vec![0; INPUTS],
            bus_link: vec![0; BUSES],
            bus_fader: vec![1.0; BUSES],
            bus_mute: vec![0; BUSES],
            monitor: None,
            phones: [None; 4],
            trims: [None; 10],
            gear: Gear::blank(),
        }
    }

    /// A quiet pretend 828. Monitor and phones start well below full scale.
    /// Nothing is patched from S/PDIF in onto S/PDIF out.
    pub fn demo() -> Self {
        let mut desk = Self::blank();
        desk.model = "828E".into();
        desk.api = 9;
        desk.name = "828".into();
        desk.uid = "demo".into();
        desk.saw_model = true;
        desk.saw_api = true;
        desk.saw_name = true;
        desk.saw_uid = true;
        desk.mix_loaded = true;
        desk.faders = Some(vec![0.0; CELLS]);
        desk.in_link[10] = 1;
        let links = [0usize, 2, 8, 10, 12, 14];
        for bus in links {
            desk.bus_link[bus] = 1;
        }
        desk.patch = Self::factory_patch();
        if let Some(faders) = desk.faders.as_mut() {
            faders[cell_index(0, 0).unwrap()] = 1.0;
            faders[cell_index(0, 28).unwrap()] = 1.0;
            faders[cell_index(1, 1).unwrap()] = 1.0;
            faders[cell_index(1, 29).unwrap()] = 1.0;
        }
        desk.monitor = Some(40);
        desk.phones = [Some(40); 4];
        desk.trims = [Some(20); 10];
        desk.gear = Gear::quiet();
        desk
    }

    /// The routing the desk opens with. Line outs come from their mixes, phones from
    /// the phone mixes, and nothing is patched from S/PDIF in onto S/PDIF out.
    pub fn factory_patch() -> Vec<u8> {
        let mut patch = vec![NOTHING; PATCH_LEN];
        for index in 0..10 {
            patch[76 + index] = crate::topology::local_source(index);
        }
        patch[86] = 58;
        patch[87] = 59;
        for index in 0..14 {
            patch[104 + index] = index as u8;
        }
        patch[118] = 90;
        patch[119] = 91;
        for index in 0..10 {
            patch[crate::topology::line_slot(index) as usize] = 76 + index as u8;
        }
        for index in 0..4 {
            patch[crate::topology::phone_slot(index) as usize] = 86 + index as u8;
        }
        patch
    }

    pub fn identity_ready(&self) -> bool {
        self.saw_model && self.saw_api && self.saw_name && self.saw_uid
    }

    pub fn contract_problem(&self) -> Option<String> {
        if self.model != "828E" || self.api != 9 {
            Some(format!(
                "This unit reports {} / API {}. {} is staying read-only.",
                if self.model.is_empty() { "an unknown model" } else { &self.model },
                self.api,
                crate::APP_NAME
            ))
        } else {
            None
        }
    }

    pub fn source_at(&self, slot: u16) -> Option<u8> {
        let value = *self.patch.get(slot as usize)?;
        if value < NOTHING { Some(value) } else { None }
    }

    pub fn gain_at(&self, bus: u16, input: u16) -> Option<f32> {
        let faders = self.faders.as_ref()?;
        let index = cell_index(bus, input)?;
        faders.get(index).copied()
    }

    pub fn loop_faders(&self) -> Option<&[f32]> {
        if self.mix_loaded { self.faders.as_deref() } else { None }
    }

    pub fn apply_report(&mut self, prop: u16, index: u16, raw: &[u8]) {
        match prop {
            PROP_MODEL => {
                self.model = decode_str(raw);
                self.saw_model = true;
            }
            PROP_API => {
                if let Some(api) = decode_i16(raw) {
                    self.api = api;
                    self.saw_api = true;
                }
            }
            PROP_NAME => {
                self.name = decode_str(raw);
                self.saw_name = true;
            }
            PROP_UID => {
                self.uid = decode_str(raw);
                self.saw_uid = true;
            }
            PROP_PATCH => {
                if (index as usize) < PATCH_LEN {
                    if let Some(byte) = raw.first() {
                        self.patch[index as usize] = *byte;
                    }
                }
            }
            PROP_FADER => {
                if self.faders.is_none() {
                    self.faders = Some(vec![0.0; CELLS]);
                }
                self.note_fader(index, raw);
            }
            PROP_PAN => {
                if let (Some(cell), Some(value)) = (self.cell(index), decode_f824(raw)) {
                    self.pans[cell] = value;
                }
            }
            PROP_SOLO => {
                if let (Some(cell), Some(byte)) = (self.cell(index), raw.first()) {
                    self.solos[cell] = *byte;
                }
            }
            PROP_MUTE => {
                if let (Some(cell), Some(byte)) = (self.cell(index), raw.first()) {
                    self.mutes[cell] = *byte;
                }
            }
            PROP_MIX_LINK => {
                if (index as usize) < INPUTS {
                    if let Some(byte) = raw.first() {
                        self.in_link[index as usize] = *byte;
                    }
                }
            }
            PROP_BUS_LINK => {
                if (index as usize) < BUSES {
                    if let Some(byte) = raw.first() {
                        self.bus_link[index as usize] = *byte;
                    }
                }
            }
            PROP_BUS_FADER => {
                if (index as usize) < BUSES {
                    if let Some(value) = decode_f824(raw) {
                        self.bus_fader[index as usize] = value;
                    }
                }
            }
            PROP_BUS_MUTE => {
                if (index as usize) < BUSES {
                    if let Some(byte) = raw.first() {
                        self.bus_mute[index as usize] = *byte;
                    }
                }
            }
            PROP_MONITOR => {
                if index == 0 {
                    if let Some(byte) = raw.first() {
                        self.monitor = Some(*byte);
                    }
                }
            }
            PROP_TRIM => self.note_trim(index, raw),
            _ => self.gear.apply(prop, index, raw),
        }
    }

    fn cell(&self, index: u16) -> Option<usize> {
        let index = index as usize;
        if index < CELLS { Some(index) } else { None }
    }

    fn note_fader(&mut self, index: u16, raw: &[u8]) {
        let Some(cell) = self.cell(index) else { return };
        let Some(value) = decode_f824(raw) else { return };
        if self.faders.is_none() {
            self.faders = Some(vec![0.0; CELLS]);
        }
        if let Some(faders) = self.faders.as_mut() {
            faders[cell] = value;
        }
        let word = cell / 64;
        let bit = cell % 64;
        self.fader_seen[word] |= 1u64 << bit;
        let seen = self.fader_seen.iter().map(|word| word.count_ones()).sum::<u32>();
        if seen >= CELLS as u32 {
            self.mix_loaded = true;
        }
    }

    fn note_trim(&mut self, index: u16, raw: &[u8]) {
        let Some(&byte) = raw.first() else { return };
        if let Some(pos) = PHONE_TRIMS.iter().position(|item| *item == index) {
            self.phones[pos] = Some(byte);
        }
        if let Some(pos) = crate::topology::LINE_TRIMS.iter().position(|item| *item == index) {
            self.trims[pos] = Some(byte);
        }
    }

    pub fn apply_write(&mut self, prop: u16, index: u16, raw: &[u8]) {
        if prop == PROP_FADER {
            // A write we authored updates that cell. It does not pretend the
            // rest of the mix has arrived.
            if self.faders.is_none() {
                self.faders = Some(vec![0.0; CELLS]);
            }
            if let Some(value) = decode_f824(raw) {
                if let Some(faders) = self.faders.as_mut() {
                    if (index as usize) < faders.len() {
                        faders[index as usize] = value;
                    }
                }
            }
            return;
        }
        self.apply_report(prop, index, raw);
    }
}

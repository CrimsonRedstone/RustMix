//! Channel map for one 828. Mixer-input slots 76–119 are not destinations.

pub const INPUTS: usize = 44;
pub const BUSES: usize = 16;
pub const CELLS: usize = BUSES * INPUTS;
pub const PATCH_LEN: usize = 128;
pub const NOTHING: u8 = 127;
pub const SPDIF_L: u8 = 58;
pub const SPDIF_R: u8 = 59;

pub const INPUT_NAMES: [&str; INPUTS] = [
    "Mic 1", "Mic 2", "Line 3", "Line 4", "Line 5", "Line 6", "Line 7", "Line 8", "Line 9",
    "Line 10", "S/PDIF L", "S/PDIF R", "ADAT A 1", "ADAT A 2", "ADAT A 3", "ADAT A 4",
    "ADAT A 5", "ADAT A 6", "ADAT A 7", "ADAT A 8", "ADAT B 1", "ADAT B 2", "ADAT B 3",
    "ADAT B 4", "ADAT B 5", "ADAT B 6", "ADAT B 7", "ADAT B 8", "Computer 1", "Computer 2",
    "Computer 3", "Computer 4", "Computer 5", "Computer 6", "Computer 7", "Computer 8",
    "Computer 9", "Computer 10", "Computer 11", "Computer 12", "Computer 13", "Computer 14",
    "Reverb return L", "Reverb return R",
];

pub const BUS_NAMES: [&str; BUSES] = [
    "Main 1", "Main 2", "Analog 1", "Analog 2", "Analog 3", "Analog 4", "Analog 5", "Analog 6",
    "Analog 7", "Analog 8", "Phones 1", "Phones 2", "Phones 3", "Phones 4", "Reverb 1", "Reverb 2",
];

const LOCAL_INPUTS: [u8; 10] = [15, 2, 18, 5, 14, 1, 13, 0, 17, 4];
const LINE_OFFSETS: [u8; 10] = [5, 7, 21, 24, 22, 19, 6, 9, 11, 8];
const PHONE_OFFSETS: [u8; 4] = [15, 25, 18, 20];

pub const LINE_TRIMS: [u16; 10] = [12, 8, 1, 3, 5, 7, 6, 4, 2, 0];
pub const PHONE_TRIMS: [u16; 4] = [15, 11, 13, 9];

pub const fn local_source(index: usize) -> u8 {
    32 + LOCAL_INPUTS[index]
}

pub const fn line_slot(index: usize) -> u16 {
    32 + LINE_OFFSETS[index] as u16
}

pub const fn phone_slot(index: usize) -> u16 {
    32 + PHONE_OFFSETS[index] as u16
}

const fn adat_bank(index: usize) -> usize {
    if index >= 4 { index + 4 } else { index }
}

const fn adat_index(index: usize) -> usize {
    if index >= 8 { adat_bank(index - 8) + 4 } else { adat_bank(index) }
}

pub const fn adat_code(index: usize) -> u8 {
    (58 + 2 + adat_index(index)) as u8
}

/// Post-FX and digital meter index for a mixer input. Computer channels use the USB index.
pub fn input_meter(input: usize) -> Option<usize> {
    match input {
        0..=9 => Some(92 + input),
        10 => Some(58),
        11 => Some(59),
        12..=27 => Some(adat_code(input - 12) as usize),
        28..=41 => Some(input - 28),
        42 | 43 => Some(56 + (input - 42)),
        _ => None,
    }
}

pub fn bus_meter(bus: usize) -> Option<usize> {
    match bus {
        0..=13 => Some(76 + bus),
        14 | 15 => Some(56 + (bus - 14)),
        _ => None,
    }
}

/// One physical output. Index 0 is Main 1, index 2 is Line 3.
pub fn output_jack_name(index: usize) -> String {
    match index {
        0 => "Main 1".into(),
        1 => "Main 2".into(),
        2..=9 => format!("Line {}", index + 1),
        _ => format!("Out {}", index + 1),
    }
}

/// Meter tap for the source patched to a jack. Source numbers and meter taps match.
/// Nothing has no tap. The volume knob is after this tap, so −∞ does not clear the bar.
pub fn jack_meter(source: u8) -> Option<usize> {
    if source >= NOTHING { None } else { Some(source as usize) }
}

/// Monitor-group bit for output numbers 3 through 10.
pub fn group_bit(number: u8) -> Option<i16> {
    if (3..=10).contains(&number) {
        Some(1i16 << u32::from(LINE_TRIMS[number as usize - 1]))
    } else {
        None
    }
}

/// Spread a bank source. Analog inputs are not contiguous channel codes.
pub fn spread_source(start: u8, offset: usize) -> u8 {
    if let Some(pos) = (0..10).find(|index| local_source(*index) == start) {
        let next = pos + offset;
        if next < 10 { local_source(next) } else { NOTHING }
    } else if start >= NOTHING {
        NOTHING
    } else {
        let value = start as usize + offset;
        if value >= usize::from(NOTHING) { NOTHING } else { value as u8 }
    }
}

pub fn optical_dest(bank: u8, index: usize) -> u16 {
    adat_code(bank as usize * 8 + index) as u16
}

#[derive(Clone, Copy, Debug)]
pub struct StereoSource {
    pub label: &'static str,
    pub left: u8,
    pub right: u8,
}

pub fn loop_sources(rate: i32) -> &'static [StereoSource] {
    const ALL: &[StereoSource] = &[
        StereoSource { label: "USB Out Main 1-2", left: 0, right: 1 },
        StereoSource { label: "USB Out Phones 1", left: 10, right: 11 },
        StereoSource { label: "USB Out Phones 2", left: 12, right: 13 },
        StereoSource { label: "USB Out Line 3-4", left: 2, right: 3 },
        StereoSource { label: "USB Out Line 5-6", left: 4, right: 5 },
        StereoSource { label: "USB Out Line 7-8", left: 6, right: 7 },
        StereoSource { label: "USB Out Line 9-10", left: 8, right: 9 },
        StereoSource { label: "USB Out SPDIF 1-2", left: 14, right: 15 },
        StereoSource { label: "USB Out Optical A 1-2", left: 16, right: 17 },
        StereoSource { label: "USB Out Optical A 3-4", left: 18, right: 19 },
        StereoSource { label: "USB Out Optical A 5-6", left: 20, right: 21 },
        StereoSource { label: "USB Out Optical A 7-8", left: 22, right: 23 },
        StereoSource { label: "USB Out Optical B 1-2", left: 24, right: 25 },
        StereoSource { label: "USB Out Optical B 3-4", left: 26, right: 27 },
        StereoSource { label: "USB Out Optical B 5-6", left: 28, right: 29 },
        StereoSource { label: "USB Out Optical B 7-8", left: 30, right: 31 },
        StereoSource { label: "Main 1-2 Mix", left: 76, right: 77 },
        StereoSource { label: "Line 3-4 Mix", left: 78, right: 79 },
        StereoSource { label: "Line 5-6 Mix", left: 80, right: 81 },
        StereoSource { label: "Line 7-8 Mix", left: 82, right: 83 },
        StereoSource { label: "Line 9-10 Mix", left: 84, right: 85 },
        StereoSource { label: "Phones 1 Mix", left: 86, right: 87 },
        StereoSource { label: "Phones 2 Mix", left: 88, right: 89 },
        StereoSource { label: "Reverb", left: 90, right: 91 },
        StereoSource { label: "Reverb return", left: 56, right: 57 },
    ];
    if rate > 96_000 { &ALL[..7] } else { ALL }
}

pub fn input_name(index: usize) -> &'static str {
    INPUT_NAMES.get(index).copied().unwrap_or("Input")
}

pub fn bus_name(index: usize) -> &'static str {
    BUS_NAMES.get(index).copied().unwrap_or("Bus")
}

pub fn cell_index(bus: u16, input: u16) -> Option<usize> {
    if (bus as usize) < BUSES && (input as usize) < INPUTS {
        Some(bus as usize * INPUTS + input as usize)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Spdif,
    Bus,
    Reverb,
    Post,
    Other,
}

pub fn classify(code: u8) -> (SourceKind, String) {
    if code == SPDIF_L {
        return (SourceKind::Spdif, "S/PDIF in L".into());
    }
    if code == SPDIF_R {
        return (SourceKind::Spdif, "S/PDIF in R".into());
    }
    if (76..90).contains(&code) {
        let bus = (code - 76) as usize;
        return (SourceKind::Bus, format!("{} mix", bus_name(bus)));
    }
    if code == 90 || code == 91 {
        return (
            SourceKind::Reverb,
            if code == 90 { "Reverb L" } else { "Reverb R" }.into(),
        );
    }
    if (92..=101).contains(&code) {
        let input = (code - 92) as usize;
        return (SourceKind::Post, format!("{} post-FX", input_name(input)));
    }
    (SourceKind::Other, source_label(code))
}

pub fn source_label(code: u8) -> String {
    if code >= 127 {
        return "Nothing".into();
    }
    if code < 32 {
        return format!("Computer {}", code + 1);
    }
    for index in 0..10 {
        if local_source(index) == code {
            return input_name(index).into();
        }
    }
    if code == SPDIF_L || code == SPDIF_R {
        return if code == SPDIF_L { "S/PDIF in L" } else { "S/PDIF in R" }.into();
    }
    for index in 0..16 {
        if adat_code(index) == code {
            let bank = if index < 8 { "A" } else { "B" };
            let number = if index < 8 { index + 1 } else { index - 7 };
            return format!("ADAT {bank} {number}");
        }
    }
    if (76..90).contains(&code) {
        return format!("{} mix", bus_name((code - 76) as usize));
    }
    if code == 90 {
        return "Reverb L".into();
    }
    if code == 91 {
        return "Reverb R".into();
    }
    if (92..=101).contains(&code) {
        return format!("{} post-FX", input_name((code - 92) as usize));
    }
    format!("Source {code}")
}

#[derive(Clone, Copy, Debug)]
pub struct Jack {
    pub id: u16,
    pub name: &'static str,
    pub group: &'static str,
}

pub fn sources() -> Vec<Jack> {
    let mut rows = vec![Jack { id: NOTHING as u16, name: "Nothing", group: "Clear" }];
    let locals: [&str; 10] = [
        "Mic 1", "Mic 2", "Line 3", "Line 4", "Line 5", "Line 6", "Line 7", "Line 8", "Line 9",
        "Line 10",
    ];
    for (index, name) in locals.iter().enumerate() {
        rows.push(Jack { id: local_source(index) as u16, name, group: "Inputs" });
    }
    rows.push(Jack { id: 58, name: "S/PDIF in L", group: "Inputs" });
    rows.push(Jack { id: 59, name: "S/PDIF in R", group: "Inputs" });
    for index in 0..8 {
        rows.push(Jack {
            id: adat_code(index) as u16,
            name: INPUT_NAMES[12 + index],
            group: "ADAT",
        });
    }
    for index in 8..16 {
        rows.push(Jack {
            id: adat_code(index) as u16,
            name: INPUT_NAMES[12 + index],
            group: "ADAT",
        });
    }
    for index in 0..18 {
        rows.push(Jack {
            id: index as u16,
            name: INPUT_NAMES[28 + index.min(13)],
            group: "Computer",
        });
    }
    // Computer 15–18 are past the mixer columns. Name them by USB channel.
    for row in rows.iter_mut().filter(|row| row.group == "Computer") {
        row.name = match row.id {
            0 => "Computer 1",
            1 => "Computer 2",
            2 => "Computer 3",
            3 => "Computer 4",
            4 => "Computer 5",
            5 => "Computer 6",
            6 => "Computer 7",
            7 => "Computer 8",
            8 => "Computer 9",
            9 => "Computer 10",
            10 => "Computer 11",
            11 => "Computer 12",
            12 => "Computer 13",
            13 => "Computer 14",
            14 => "Computer 15",
            15 => "Computer 16",
            16 => "Computer 17",
            _ => "Computer 18",
        };
    }
    let mixes = [
        (76, "Main 1 mix"),
        (77, "Main 2 mix"),
        (78, "Analog 1 mix"),
        (79, "Analog 2 mix"),
        (80, "Analog 3 mix"),
        (81, "Analog 4 mix"),
        (82, "Analog 5 mix"),
        (83, "Analog 6 mix"),
        (84, "Analog 7 mix"),
        (85, "Analog 8 mix"),
        (86, "Phones 1 mix"),
        (87, "Phones 2 mix"),
        (88, "Phones 3 mix"),
        (89, "Phones 4 mix"),
    ];
    for (id, name) in mixes {
        rows.push(Jack { id, name, group: "Mixes" });
    }
    rows.push(Jack { id: 90, name: "Reverb L", group: "Reverb" });
    rows.push(Jack { id: 91, name: "Reverb R", group: "Reverb" });
    for index in 0..10 {
        rows.push(Jack {
            id: 92 + index,
            name: INPUT_NAMES[index as usize],
            group: "Post-FX",
        });
    }
    rows
}

pub fn destinations() -> Vec<Jack> {
    let mut rows = Vec::new();
    let lines = [
        (0, "Main L"),
        (1, "Main R"),
        (2, "Line 3"),
        (3, "Line 4"),
        (4, "Line 5"),
        (5, "Line 6"),
        (6, "Line 7"),
        (7, "Line 8"),
        (8, "Line 9"),
        (9, "Line 10"),
    ];
    for (index, name) in lines {
        rows.push(Jack { id: line_slot(index), name, group: "Outputs" });
    }
    let phones = [(0, "Phones 1 L"), (1, "Phones 1 R"), (2, "Phones 2 L"), (3, "Phones 2 R")];
    for (index, name) in phones {
        rows.push(Jack { id: phone_slot(index), name, group: "Phones" });
    }
    rows.push(Jack { id: 58, name: "S/PDIF out L", group: "S/PDIF" });
    rows.push(Jack { id: 59, name: "S/PDIF out R", group: "S/PDIF" });
    for index in 0..8 {
        rows.push(Jack {
            id: adat_code(index) as u16,
            name: INPUT_NAMES[12 + index],
            group: "Optical out",
        });
    }
    for index in 8..16 {
        rows.push(Jack {
            id: adat_code(index) as u16,
            name: INPUT_NAMES[12 + index],
            group: "Optical out",
        });
    }
    for index in 0..18u16 {
        rows.push(Jack {
            id: index,
            name: match index {
                0 => "Computer 1",
                1 => "Computer 2",
                2 => "Computer 3",
                3 => "Computer 4",
                4 => "Computer 5",
                5 => "Computer 6",
                6 => "Computer 7",
                7 => "Computer 8",
                8 => "Computer 9",
                9 => "Computer 10",
                10 => "Computer 11",
                11 => "Computer 12",
                12 => "Computer 13",
                13 => "Computer 14",
                14 => "Computer 15",
                15 => "Computer 16",
                16 => "Computer 17",
                _ => "Computer 18",
            },
            group: "To the computer",
        });
    }
    rows
}

/// Partner source for a stereo connect. Source codes and destination slots share numbers
/// and mean different jacks, so this must not be used on a destination slot.
pub fn source_partner(code: u16) -> Option<u16> {
    if code < 18 {
        return Some(code ^ 1);
    }
    for index in (0..10).step_by(2) {
        let left = u16::from(local_source(index));
        let right = u16::from(local_source(index + 1));
        if code == left {
            return Some(right);
        }
        if code == right {
            return Some(left);
        }
    }
    if code == 58 || code == 59 {
        return Some(code ^ 1);
    }
    for index in (0..16).step_by(2) {
        let left = u16::from(adat_code(index));
        let right = u16::from(adat_code(index + 1));
        if code == left {
            return Some(right);
        }
        if code == right {
            return Some(left);
        }
    }
    if (76..90).contains(&code) || (92..=101).contains(&code) {
        return Some(code ^ 1);
    }
    if code == 90 || code == 91 {
        return Some(code ^ 1);
    }
    None
}

/// Partner destination for a stereo connect or a stereo detach.
pub fn dest_partner(slot: u16) -> Option<u16> {
    if slot < 18 {
        return Some(slot ^ 1);
    }
    for index in (0..10).step_by(2) {
        let left = line_slot(index);
        let right = line_slot(index + 1);
        if slot == left {
            return Some(right);
        }
        if slot == right {
            return Some(left);
        }
    }
    for index in (0..4).step_by(2) {
        let left = phone_slot(index);
        let right = phone_slot(index + 1);
        if slot == left {
            return Some(right);
        }
        if slot == right {
            return Some(left);
        }
    }
    if slot == 58 || slot == 59 {
        return Some(slot ^ 1);
    }
    for index in (0..16).step_by(2) {
        let left = u16::from(adat_code(index));
        let right = u16::from(adat_code(index + 1));
        if slot == left {
            return Some(right);
        }
        if slot == right {
            return Some(left);
        }
    }
    None
}

/// Destinations fed by this source. Mixer-input slots are stored in the same table and are not counted.
pub fn feed_count(patch: &[u8], source: u16) -> usize {
    destinations()
        .into_iter()
        .filter(|jack| u16::from(patch.get(jack.id as usize).copied().unwrap_or(NOTHING)) == source)
        .count()
}

pub fn destination_name(slot: u16) -> String {
    destinations()
        .into_iter()
        .find(|jack| jack.id == slot)
        .map(|jack| jack.name.to_string())
        .unwrap_or_else(|| format!("Output {slot}"))
}

/// Playback pairs the 828 HiFi profile exposes, and the patch slots they feed.
pub struct PlayPair {
    pub id: &'static str,
    pub label: &'static str,
    pub suffix: &'static str,
    pub slots: [u16; 2],
    pub usb: [u8; 2],
}

pub fn playback_pairs() -> [PlayPair; 9] {
    [
        PlayPair { id: "Line1", label: "Main", suffix: "HiFi__Line1__sink", slots: [line_slot(0), line_slot(1)], usb: [0, 1] },
        PlayPair { id: "Line2", label: "Line out 3-4", suffix: "HiFi__Line2__sink", slots: [line_slot(2), line_slot(3)], usb: [2, 3] },
        PlayPair { id: "Line3", label: "Line out 5-6", suffix: "HiFi__Line3__sink", slots: [line_slot(4), line_slot(5)], usb: [4, 5] },
        PlayPair { id: "Line4", label: "Line out 7-8", suffix: "HiFi__Line4__sink", slots: [line_slot(6), line_slot(7)], usb: [6, 7] },
        PlayPair { id: "Line5", label: "Line out 9-10", suffix: "HiFi__Line5__sink", slots: [line_slot(8), line_slot(9)], usb: [8, 9] },
        PlayPair { id: "Headphones1", label: "Phones 1", suffix: "HiFi__Headphones1__sink", slots: [phone_slot(0), phone_slot(1)], usb: [10, 11] },
        PlayPair { id: "Headphones2", label: "Phones 2", suffix: "HiFi__Headphones2__sink", slots: [phone_slot(2), phone_slot(3)], usb: [12, 13] },
        PlayPair { id: "SPDIF1", label: "S/PDIF", suffix: "HiFi__SPDIF1__sink", slots: [58, 59], usb: [14, 15] },
        PlayPair { id: "SPDIF2", label: "Optical 1-2", suffix: "HiFi__SPDIF2__sink", slots: [adat_code(0) as u16, adat_code(1) as u16], usb: [16, 17] },
    ]
}

pub struct CapDev {
    pub id: &'static str,
    pub label: &'static str,
    pub suffix: &'static str,
    pub slots: [u16; 2],
    pub slot_len: u8,
    pub codes: [u8; 2],
    /// Zero means the HiFi device is the loopback and the patch is left alone.
    pub code_len: u8,
}

pub fn capture_devices() -> [CapDev; 8] {
    [
        CapDev { id: "Mic1", label: "Mic 1", suffix: "HiFi__Mic1__source", slots: [0, 0], slot_len: 1, codes: [local_source(0), 0], code_len: 1 },
        CapDev { id: "Line6", label: "Line 3-4", suffix: "HiFi__Line6__source", slots: [2, 3], slot_len: 2, codes: [local_source(2), local_source(3)], code_len: 2 },
        CapDev { id: "Line7", label: "Line 5-6", suffix: "HiFi__Line7__source", slots: [4, 5], slot_len: 2, codes: [local_source(4), local_source(5)], code_len: 2 },
        CapDev { id: "Line8", label: "Line 7-8", suffix: "HiFi__Line8__source", slots: [6, 7], slot_len: 2, codes: [local_source(6), local_source(7)], code_len: 2 },
        CapDev { id: "Line9", label: "Line 9-10", suffix: "HiFi__Line9__source", slots: [8, 9], slot_len: 2, codes: [local_source(8), local_source(9)], code_len: 2 },
        // Default loopback location. USB In 1-2 is the other location, on the Device page.
        CapDev { id: "Line10", label: "USB In 11-12", suffix: "HiFi__Line10__source", slots: [10, 11], slot_len: 2, codes: [0, 0], code_len: 0 },
        CapDev { id: "SPDIF3", label: "S/PDIF in", suffix: "HiFi__SPDIF3__source", slots: [12, 13], slot_len: 2, codes: [58, 59], code_len: 2 },
        CapDev { id: "SPDIF4", label: "Optical in 1-2", suffix: "HiFi__SPDIF4__source", slots: [14, 15], slot_len: 2, codes: [adat_code(0), adat_code(1)], code_len: 2 },
    ]
}

pub fn db_of(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

pub fn linear_of(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn db_text(linear: f32) -> String {
    if linear <= 0.0 || !linear.is_finite() {
        return "−∞".into();
    }
    let db = db_of(linear);
    if db.abs() < 0.05 {
        return "0.0".into();
    }
    if db > 0.0 {
        format!("+{db:.1}")
    } else {
        format!("−{:.1}", -db)
    }
}

pub fn stored_text(stored: u8) -> String {
    if stored >= 100 {
        "−∞".into()
    } else if stored == 0 {
        "0 dB".into()
    } else {
        format!("−{stored} dB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_jacks() {
        assert_eq!(local_source(0), 47);
        assert_eq!(line_slot(0), 37);
        assert_eq!(line_slot(1), 39);
        assert_eq!(line_slot(9), 40);
        assert_eq!(phone_slot(0), 47);
        assert_eq!(phone_slot(1), 57);
        assert_eq!(adat_code(0), 60);
        assert_eq!(adat_code(4), 68);
        assert_eq!(adat_code(8), 64);
        assert_eq!(adat_code(15), 75);
        assert!(destinations().iter().all(|jack| jack.id < 76));
        assert!(sources().iter().any(|jack| jack.id == 127));
    }

    #[test]
    fn analog_mixes_are_written_out() {
        assert_eq!(bus_name(2), "Analog 1");
        assert_eq!(bus_name(9), "Analog 8");
        assert_eq!(source_label(78), "Analog 1 mix");
        assert_eq!(source_label(85), "Analog 8 mix");
        assert!(sources().iter().any(|jack| jack.name == "Analog 1 mix"));
        assert!(sources().iter().all(|jack| !jack.name.contains("Ana ")));
    }

    #[test]
    fn stereo_partners_stay_on_their_own_side() {
        assert_eq!(source_partner(u16::from(local_source(0))), Some(u16::from(local_source(1))));
        assert_eq!(source_partner(78), Some(79));
        assert_eq!(source_partner(79), Some(78));
        assert_eq!(source_partner(58), Some(59));
        assert_eq!(source_partner(17), Some(16));
        assert_eq!(source_partner(u16::from(adat_code(4))), Some(u16::from(adat_code(5))));
        assert_eq!(dest_partner(line_slot(0)), Some(line_slot(1)));
        assert_eq!(dest_partner(line_slot(1)), Some(line_slot(0)));
        assert_eq!(dest_partner(phone_slot(0)), Some(phone_slot(1)));
        assert_eq!(dest_partner(phone_slot(2)), Some(phone_slot(3)));
        // 47 is Mic 1 as a source and Phones 1 L as a destination.
        assert_eq!(source_partner(47), Some(u16::from(local_source(1))));
        assert_eq!(dest_partner(47), Some(phone_slot(1)));
        assert_ne!(source_partner(47), dest_partner(47));
    }

    #[test]
    fn a_jack_meter_follows_the_patched_source() {
        assert_eq!(jack_meter(0), Some(0));
        assert_eq!(loop_sources(48_000)[0].label, "USB Out Main 1-2");
        assert_eq!(capture_devices()[5].label, "USB In 11-12");
        assert_eq!(output_jack_name(0), "Main 1");
        assert_eq!(output_jack_name(4), "Line 5");
        assert_eq!(output_jack_name(9), "Line 10");
        assert_eq!(jack_meter(76), Some(76));
        assert_eq!(jack_meter(NOTHING), None);
        assert_eq!(jack_meter(255), None);
        // Phones 2's own mix is a different tap from the Main mix.
        assert_eq!(bus_meter(12), Some(88));
        assert_ne!(jack_meter(76), bus_meter(12));
    }

    #[test]
    fn feed_count_ignores_mixer_inputs() {
        let mut patch = vec![NOTHING; PATCH_LEN];
        patch[76] = local_source(0);
        patch[line_slot(2) as usize] = local_source(0);
        assert_eq!(feed_count(&patch, u16::from(local_source(0))), 1);
    }
}

//! MOTU 828 SysEx framing. The host and the device use different bodies:
//! device → host is property, index, value; host → device inserts a length.

pub const HEADER: [u8; 6] = [0xF0, 0x00, 0x00, 0x3B, 0x00, 0x01];
pub const SET: u8 = 0x00;
pub const PROBE: u8 = 0x01;
pub const ENABLE: u8 = 0x02;
pub const FROM_DEVICE: u8 = 0x40;

pub const PROP_MODEL: u16 = 21;
pub const PROP_API: u16 = 8;
pub const PROP_UID: u16 = 2;
pub const PROP_NAME: u16 = 3;
pub const PROP_MIX_LINK: u16 = 1000;
pub const PROP_BUS_LINK: u16 = 1001;
pub const PROP_FADER: u16 = 1016;
pub const PROP_PAN: u16 = 1017;
pub const PROP_SOLO: u16 = 1018;
pub const PROP_MUTE: u16 = 1019;
pub const PROP_BUS_FADER: u16 = 1027;
pub const PROP_BUS_MUTE: u16 = 1028;
pub const PROP_PATCH: u16 = 5010;
pub const PROP_TRIM: u16 = 5000;
pub const PROP_MONITOR: u16 = 5011;
pub const PROP_FIRMWARE: u16 = 4;
pub const PROP_IN_NAME: u16 = 6;
pub const PROP_OUT_NAME: u16 = 7;
pub const PROP_RATE: u16 = 10;
pub const PROP_CLOCK: u16 = 11;
pub const PROP_MIDI: u16 = 19;
pub const PROP_LOOP_FIRST: u16 = 20;
pub const PROP_EQ_MODE_IN: u16 = 1002;
pub const PROP_EQ_BYPASS_IN: u16 = 1003;
pub const PROP_EQ_FREQ_IN: u16 = 1004;
pub const PROP_EQ_GAIN_IN: u16 = 1005;
pub const PROP_EQ_Q_IN: u16 = 1006;
pub const PROP_GATE_BYPASS: u16 = 1007;
pub const PROP_GATE_THR: u16 = 1008;
pub const PROP_GATE_ATT: u16 = 1009;
pub const PROP_GATE_REL: u16 = 1010;
pub const PROP_COMP_BYPASS: u16 = 1011;
pub const PROP_COMP_THR: u16 = 1012;
pub const PROP_COMP_ATT: u16 = 1013;
pub const PROP_COMP_REL: u16 = 1014;
pub const PROP_COMP_RATIO: u16 = 1015;
pub const PROP_HOST_MIX: u16 = 1020;
pub const PROP_MAKEUP: u16 = 1021;
pub const PROP_EQ_MODE_BUS: u16 = 1022;
pub const PROP_EQ_BYPASS_BUS: u16 = 1023;
pub const PROP_EQ_FREQ_BUS: u16 = 1024;
pub const PROP_EQ_GAIN_BUS: u16 = 1025;
pub const PROP_EQ_Q_BUS: u16 = 1026;
pub const PROP_REV_DECAY: u16 = 1030;
pub const PROP_REV_DAMP: u16 = 1031;
pub const PROP_REV_PRE: u16 = 1032;
pub const PROP_REV_WIDTH: u16 = 1033;
pub const PROP_REV_PRESET: u16 = 1034;
pub const PROP_PREAMP: u16 = 5001;
pub const PROP_PHASE: u16 = 5002;
pub const PROP_PAD: u16 = 5003;
pub const PROP_PHANTOM: u16 = 5004;
pub const PROP_OPTICAL: u16 = 5006;
pub const PROP_GROUP: u16 = 5012;
pub const PROP_LOOP_SRC: u16 = 5014;
pub const PROP_AB: u16 = 5015;
pub const PROP_A: u16 = 5016;
pub const PROP_B: u16 = 5017;
pub const PROP_MONO: u16 = 5018;
pub const PROP_MON_MUTE: u16 = 5019;
pub const PROP_WORD: u16 = 5024;
pub const PROP_EXPANDER: u16 = 5025;
pub const PROP_TALK_LATCH: u16 = 5026;
pub const PROP_TALK_EN: u16 = 5027;
pub const PROP_TALK_LEVEL: u16 = 5028;
pub const PROP_TALK_DIM: u16 = 5029;
pub const PROP_TALK_SRC: u16 = 5030;
pub const PROP_TALK_DEST: u16 = 5031;
pub const PROP_FOOT_EN: u16 = 5032;
pub const PROP_FOOT_DOWN: u16 = 5033;
pub const PROP_FOOT_UP: u16 = 5034;
pub const PROP_METERS: u16 = 6000;

pub fn pack7(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + body.len() / 7 + 1);
    for group in body.chunks(7) {
        let mut msb = 0u8;
        for (k, byte) in group.iter().enumerate() {
            msb |= ((byte >> 7) & 1) << k;
        }
        out.push(msb);
        for byte in group {
            out.push(byte & 0x7F);
        }
    }
    out
}

pub fn unpack7(packed: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packed.len());
    let mut index = 0;
    while index < packed.len() {
        let msb = packed[index];
        index += 1;
        for bit in 0..7 {
            if index >= packed.len() {
                break;
            }
            out.push(packed[index] | (((msb >> bit) & 1) << 7));
            index += 1;
        }
    }
    out
}

pub fn probe_frame() -> Vec<u8> {
    frame(PROBE, &[])
}

pub fn probe_reply_frame() -> Vec<u8> {
    frame(FROM_DEVICE | PROBE, &[])
}

pub fn enable_frame() -> Vec<u8> {
    frame(ENABLE, &[])
}

pub fn encode_set(prop: u16, index: u16, value: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(6 + value.len());
    body.extend_from_slice(&prop.to_be_bytes());
    body.extend_from_slice(&index.to_be_bytes());
    body.extend_from_slice(&(value.len() as u16).to_be_bytes());
    body.extend_from_slice(value);
    frame(SET, &body)
}

/// A device → host report, used to build tests and to recognise the real stream.
pub fn encode_report(prop: u16, index: u16, value: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + value.len());
    body.extend_from_slice(&prop.to_be_bytes());
    body.extend_from_slice(&index.to_be_bytes());
    body.extend_from_slice(value);
    frame(FROM_DEVICE | SET, &body)
}

fn frame(request: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&HEADER);
    out.push(request);
    out.extend(pack7(body));
    out.push(0xF7);
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    ProbeReply,
    Report { prop: u16, index: u16, raw: Vec<u8> },
    Ignore,
}

pub fn decode(bytes: &[u8]) -> Incoming {
    if bytes.len() < 8 || !bytes.starts_with(&HEADER) || *bytes.last().unwrap_or(&0) != 0xF7 {
        return Incoming::Ignore;
    }
    let request = bytes[6];
    if request == FROM_DEVICE | PROBE && bytes.len() == 8 {
        return Incoming::ProbeReply;
    }
    if request == FROM_DEVICE | SET {
        let body = unpack7(&bytes[7..bytes.len() - 1]);
        if body.len() >= 4 {
            let prop = u16::from_be_bytes([body[0], body[1]]);
            let index = u16::from_be_bytes([body[2], body[3]]);
            return Incoming::Report {
                prop,
                index,
                raw: body[4..].to_vec(),
            };
        }
    }
    Incoming::Ignore
}

pub fn encode_f824(value: f32) -> [u8; 4] {
    let raw = (f64::from(value) * f64::from(1 << 24)).round() as i32;
    raw.to_be_bytes()
}

pub fn decode_f824(raw: &[u8]) -> Option<f32> {
    let bytes: [u8; 4] = raw.get(..4)?.try_into().ok()?;
    let number = i32::from_be_bytes(bytes);
    Some(number as f32 / (1 << 24) as f32)
}

pub fn encode_i16(value: i16) -> [u8; 2] {
    value.to_be_bytes()
}

pub fn decode_i16(raw: &[u8]) -> Option<i16> {
    let bytes: [u8; 2] = raw.get(..2)?.try_into().ok()?;
    Some(i16::from_be_bytes(bytes))
}

pub fn encode_i32(value: i32) -> [u8; 4] {
    value.to_be_bytes()
}

pub fn decode_i32(raw: &[u8]) -> Option<i32> {
    let bytes: [u8; 4] = raw.get(..4)?.try_into().ok()?;
    Some(i32::from_be_bytes(bytes))
}

/// Host name writes are 32 bytes, cut at 31 so a trailing NUL remains.
pub fn text_bytes(text: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    let bytes = text.as_bytes();
    let n = bytes.len().min(31);
    out[..n].copy_from_slice(&bytes[..n]);
    out
}

pub fn decode_str(raw: &[u8]) -> String {
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

/// One open SysEx frame. A frame that never ends is dropped here so the
/// buffer cannot grow for as long as the port stays open.
const FRAME_CAP: usize = 64 * 1024;

#[derive(Debug, Default)]
pub struct Splitter {
    buf: Vec<u8>,
    inside: bool,
}

impl Splitter {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &byte in chunk {
            if byte == 0xF0 {
                self.buf.clear();
                self.buf.push(byte);
                self.inside = true;
            } else if self.inside {
                if self.buf.len() >= FRAME_CAP {
                    self.buf.clear();
                    self.inside = false;
                    continue;
                }
                self.buf.push(byte);
                if byte == 0xF7 {
                    frames.push(std::mem::take(&mut self.buf));
                    self.inside = false;
                }
            }
        }
        frames
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Write {
    pub prop: u16,
    pub index: u16,
    pub raw: Vec<u8>,
}

impl Write {
    pub fn byte(prop: u16, index: u16, value: u8) -> Self {
        Self { prop, index, raw: vec![value] }
    }

    pub fn gain(prop: u16, index: u16, linear: f32) -> Self {
        Self { prop, index, raw: encode_f824(linear).to_vec() }
    }

    pub fn i16(prop: u16, index: u16, value: i16) -> Self {
        Self { prop, index, raw: encode_i16(value).to_vec() }
    }

    pub fn i32(prop: u16, index: u16, value: i32) -> Self {
        Self { prop, index, raw: encode_i32(value).to_vec() }
    }

    pub fn text(prop: u16, index: u16, bytes: [u8; 32]) -> Self {
        Self { prop, index, raw: bytes.to_vec() }
    }

    pub fn frame(&self) -> Vec<u8> {
        encode_set(self.prop, self.index, &self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_roundtrip_covers_group_edges() {
        for length in [0usize, 1, 6, 7, 8, 14, 15, 32] {
            let body: Vec<u8> = (0..length).map(|i| (i * 17 + 3) as u8).collect();
            assert_eq!(unpack7(&pack7(&body)), body, "length {length}");
        }
        let high: Vec<u8> = (0..20).map(|i| 0x80 | i).collect();
        assert_eq!(unpack7(&pack7(&high)), high);
    }

    #[test]
    fn w1_fixture_sets_cell_643() {
        let frame = encode_set(PROP_FADER, 643, &[0x00, 0x80, 0x4D, 0xCE]);
        let expect = [
            0xF0, 0x00, 0x00, 0x3B, 0x00, 0x01, 0x00, 0x0A, 0x03, 0x78, 0x02, 0x03, 0x00, 0x04,
            0x00, 0x05, 0x00, 0x4D, 0x4E, 0xF7,
        ];
        assert_eq!(frame, expect);
    }

    #[test]
    fn probe_and_enable_are_the_short_frames() {
        assert_eq!(probe_frame(), [0xF0, 0x00, 0x00, 0x3B, 0x00, 0x01, 0x01, 0xF7]);
        assert_eq!(enable_frame(), [0xF0, 0x00, 0x00, 0x3B, 0x00, 0x01, 0x02, 0xF7]);
    }

    #[test]
    fn device_report_roundtrip_has_no_length_field() {
        let frame = encode_report(PROP_MUTE, 3, &[1]);
        match decode(&frame) {
            Incoming::Report { prop, index, raw } => {
                assert_eq!((prop, index, raw.as_slice()), (PROP_MUTE, 3, [1].as_slice()));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(decode(&probe_reply()), Incoming::ProbeReply);
    }

    fn probe_reply() -> Vec<u8> {
        vec![0xF0, 0x00, 0x00, 0x3B, 0x00, 0x01, 0x41, 0xF7]
    }

    #[test]
    fn splitter_rejoins_and_drops_cut_frames() {
        let mut splitter = Splitter::default();
        let frame = encode_report(PROP_API, 0, &encode_i16(9));
        let mut chunked = vec![0x90, 0x40];
        chunked.extend_from_slice(&frame[..4]);
        assert!(splitter.push(&chunked).is_empty());
        let rest = splitter.push(&frame[4..]);
        assert_eq!(rest, vec![frame]);
    }

    #[test]
    fn an_unterminated_frame_stops_at_the_cap_and_the_next_frame_still_parses() {
        let mut splitter = Splitter::default();
        let mut chunk = vec![0xF0];
        chunk.extend(std::iter::repeat(0x01).take(70_000));
        assert!(splitter.push(&chunk).is_empty());
        assert!(splitter.buf.len() <= 64 * 1024);
        let frame = encode_report(PROP_API, 0, &encode_i16(9));
        assert_eq!(splitter.push(&frame), vec![frame]);
    }

    #[test]
    fn fixed_point_survives_a_known_gain() {
        let encoded = encode_f824(1.0);
        assert_eq!(encoded, [0x01, 0x00, 0x00, 0x00]);
        let back = decode_f824(&encoded).unwrap();
        assert!((back - 1.0).abs() < 0.000_01);
    }
}

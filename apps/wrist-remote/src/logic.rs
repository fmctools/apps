// Pure protocol and layout: host unit tests run this without the watch API.
use core::fmt::{self, Write};

pub const MSG_MAX: usize = 128;
pub const MAGIC: &[u8; 3] = b"WR1";

pub const PING: u8 = 0x01;
pub const LIST: u8 = 0x02;
pub const BTN: u8 = 0x03;
pub const KEY: u8 = 0x10;
pub const VOL: u8 = 0x11;
#[allow(dead_code)] // used by the companion side of the protocol
pub const MEDIA: u8 = 0x12;
#[allow(dead_code)]
pub const TEXT: u8 = 0x13;

pub const OK: u8 = 0;
#[allow(dead_code)]
pub const BAD: u8 = 1;
#[allow(dead_code)]
pub const NOT_FOUND: u8 = 2;
#[allow(dead_code)]
pub const UNAVAILABLE: u8 = 3;

pub const MAX_DEVICES: usize = 6;
pub const MAX_BUTTONS: usize = 4;
pub const NAME_MAX: usize = 16;
pub const LABEL_MAX: usize = 10;
pub const CMD_MAX: usize = 12;

pub const CENTRE: i16 = 233;
pub const BTN_X0: i16 = 28;
pub const BTN_X1: i16 = 248;
pub const BTN_Y0: i16 = 150;
pub const BTN_Y1: i16 = 230;
pub const BTN_W: u16 = 190;
pub const BTN_H: u16 = 64;

#[derive(Clone, Copy, Default)]
pub struct Name {
    pub b: [u8; NAME_MAX],
    pub n: u8,
}

impl Name {
    pub const fn from(s: &[u8]) -> Self {
        let mut b = [0; NAME_MAX];
        let mut i = 0;
        while i < s.len() && i < NAME_MAX {
            b[i] = s[i];
            i += 1;
        }
        Self { b, n: i as u8 }
    }

    pub fn set(&mut self, s: &[u8]) {
        *self = Self::from(s);
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.b[..self.n as usize]).unwrap_or("?")
    }
}

#[derive(Clone, Copy)]
pub struct Button {
    pub label: [u8; LABEL_MAX],
    pub label_len: u8,
    pub cmd: [u8; CMD_MAX],
    pub cmd_len: u8,
}

impl Button {
    pub const EMPTY: Self = Self {
        label: [0; LABEL_MAX],
        label_len: 0,
        cmd: [0; CMD_MAX],
        cmd_len: 0,
    };

    pub fn label(&self) -> &str {
        core::str::from_utf8(&self.label[..self.label_len as usize]).unwrap_or("?")
    }

    pub fn command(&self) -> &[u8] {
        &self.cmd[..self.cmd_len as usize]
    }
}

pub struct Reply<'a> {
    pub id: u8,
    pub status: u8,
    pub payload: &'a [u8],
}

/// `WR1 ‖ id ‖ cmd ‖ payload`
pub fn frame(buf: &mut [u8; MSG_MAX], id: u8, cmd: u8, payload: &[u8]) -> Option<usize> {
    let total = 5 + payload.len();
    if total > MSG_MAX {
        return None;
    }
    buf[..3].copy_from_slice(MAGIC);
    buf[3] = id;
    buf[4] = cmd;
    buf[5..total].copy_from_slice(payload);
    Some(total)
}

/// `WR1 ‖ id ‖ status ‖ payload`
pub fn parse(data: &[u8]) -> Option<Reply<'_>> {
    if data.len() < 5 || &data[..3] != MAGIC {
        return None;
    }
    Some(Reply {
        id: data[3],
        status: data[4],
        payload: &data[5..],
    })
}

/// `count ‖ (len ‖ name) × count`
pub fn decode_list(payload: &[u8], out: &mut [Name; MAX_DEVICES]) -> usize {
    let Some((&count, mut rest)) = payload.split_first() else {
        return 0;
    };
    let mut n = 0;
    for _ in 0..count.min(MAX_DEVICES as u8) {
        let Some((&len, tail)) = rest.split_first() else {
            break;
        };
        let len = len as usize;
        if len == 0 || len > NAME_MAX || tail.len() < len {
            break;
        }
        out[n].set(&tail[..len]);
        rest = &tail[len..];
        n += 1;
    }
    n
}

/// `count ‖ (label_len ‖ label ‖ cmd_len ‖ cmd) × count`
pub fn decode_buttons(payload: &[u8], out: &mut [Button; MAX_BUTTONS]) -> usize {
    let Some((&count, mut rest)) = payload.split_first() else {
        return 0;
    };
    let mut n = 0;
    for _ in 0..count.min(MAX_BUTTONS as u8) {
        let Some((&llen, tail)) = rest.split_first() else {
            break;
        };
        let llen = llen as usize;
        if llen > LABEL_MAX || tail.len() < llen + 1 {
            break;
        }
        let clen = tail[llen] as usize;
        let body = &tail[llen + 1..];
        if clen > CMD_MAX || body.len() < clen {
            break;
        }
        let mut b = Button::EMPTY;
        b.label[..llen].copy_from_slice(&tail[..llen]);
        b.label_len = llen as u8;
        b.cmd[..clen].copy_from_slice(&body[..clen]);
        b.cmd_len = clen as u8;
        out[n] = b;
        rest = &body[clen..];
        n += 1;
    }
    n
}

pub fn centred_x(chars: usize) -> i16 {
    CENTRE - 8 * chars as i16
}

pub fn row_hit(y: i16, top: i16, height: i16, rows: usize) -> Option<usize> {
    if y < top {
        return None;
    }
    let i = (y - top) / height;
    if i >= 0 && (i as usize) < rows {
        Some(i as usize)
    } else {
        None
    }
}

/// 2×2 grid, returns 0..3.
pub fn grid_hit(x: i16, y: i16) -> Option<usize> {
    for (i, &(bx, by)) in [
        (BTN_X0, BTN_Y0),
        (BTN_X1, BTN_Y0),
        (BTN_X0, BTN_Y1),
        (BTN_X1, BTN_Y1),
    ]
    .iter()
    .enumerate()
    {
        if x >= bx && x < bx + BTN_W as i16 && y >= by && y < by + BTN_H as i16 {
            return Some(i);
        }
    }
    None
}

pub struct Line {
    pub b: [u8; 32],
    pub n: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self { b: [0; 32], n: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.b[..self.n]).unwrap_or("?")
    }

    pub fn clear(&mut self) {
        self.n = 0;
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let n = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + n].copy_from_slice(&s.as_bytes()[..n]);
        self.n += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_and_parse() {
        let mut b = [0u8; MSG_MAX];
        let n = frame(&mut b, 7, KEY, &[1, 2, 0]).unwrap();
        assert_eq!(n, 8);
        let r = parse(&b[..n]).unwrap();
        assert_eq!(r.id, 7);
        assert_eq!(r.status, KEY);
        assert_eq!(r.payload, &[1, 2, 0]);
        assert!(parse(&b[..4]).is_none());
        assert!(parse(b"XYZ\0\0").is_none());
    }

    #[test]
    fn frame_rejects_oversize() {
        let mut b = [0u8; MSG_MAX];
        assert!(frame(&mut b, 0, TEXT, &[0; 124]).is_none());
        assert!(frame(&mut b, 0, TEXT, &[0; 123]).is_some());
    }

    #[test]
    fn list_roundtrip_and_truncation() {
        let payload = [3, 2, b'T', b'V', 2, b'P', b'C', 5, b'M', b'e', b'd', b'i', b'a'];
        let mut out = [Name::from(b""); MAX_DEVICES];
        assert_eq!(decode_list(&payload, &mut out), 3);
        assert_eq!(out[0].as_str(), "TV");
        assert_eq!(out[2].as_str(), "Media");
        assert_eq!(decode_list(&[2, 2, b'T', b'V'], &mut out), 1);
    }

    #[test]
    fn buttons_roundtrip() {
        let mut payload = vec![1u8, 3];
        payload.extend_from_slice(b"PWR");
        payload.push(5);
        payload.extend_from_slice(b"key:1");
        let mut out = [Button::EMPTY; MAX_BUTTONS];
        assert_eq!(decode_buttons(&payload, &mut out), 1);
        assert_eq!(out[0].label(), "PWR");
        assert_eq!(out[0].command(), b"key:1");
    }

    #[test]
    fn hit_tests() {
        assert_eq!(row_hit(112, 110, 52, 5), Some(0));
        assert_eq!(row_hit(162, 110, 52, 5), Some(1));
        assert_eq!(row_hit(100, 110, 52, 5), None);
        assert_eq!(grid_hit(40, 160), Some(0));
        assert_eq!(grid_hit(260, 250), Some(3));
        assert_eq!(grid_hit(233, 100), None);
    }

    #[test]
    fn line_is_bounded() {
        let mut l = Line::new();
        let _ = write!(l, "hello {}", 42);
        assert_eq!(l.as_str(), "hello 42");
    }
}

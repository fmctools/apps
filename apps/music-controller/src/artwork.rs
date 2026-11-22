//! Strict watch-side decoder for Music Companion artwork transfers.
//!
//! The decoder is independent of the watch `Api`: callers begin a request, feed it framed M1
//! replies one at a time, and query the committed image.  The receiver owns two fixed payload
//! buffers, so an instance can live in static storage and no allocation is required.

use crate::rpc::{self, Reply};

pub const WIDTH: u8 = 64;
pub const HEIGHT: u8 = 64;
pub const PAYLOAD_LEN: usize = 3072;
pub const MANIFEST_LEN: usize = 25;
pub const CHUNK_COUNT: usize = 27;
pub const RECORD_COUNT: u8 = 28;
pub const MANIFEST_VERSION: u8 = 1;
pub const ENCODING_RGB332_6BPP: u8 = 1;
pub const CHUNK_LEN: usize = 115;
pub const LAST_CHUNK_LEN: usize = 82;

/// The state of the latest artwork request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// No request is active.
    Idle,
    /// A reply is still expected.
    Pending,
    /// A complete image passed its CRC check and replaced the current image.
    Complete,
    /// The phone confirmed that the current image is unchanged.
    Unchanged,
    /// The phone has no artwork.
    Unavailable,
    /// The user has not granted artwork access yet.
    PermissionRequired,
    /// Artwork access was denied.
    Denied,
    /// The phone does not support the artwork method.
    Unsupported,
    /// The response violated the artwork protocol.
    Invalid,
    /// The request timed out before all records arrived.
    TimedOut,
}

/// The result of handling one reply.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The reply did not belong to the active request.
    Ignored,
    /// More records are required.
    Pending,
    /// A complete image was committed.
    Complete,
    /// The current image is unchanged.
    Unchanged,
    /// The phone has no artwork.
    Unavailable,
    /// Artwork permission is required.
    PermissionRequired,
    /// Artwork permission was denied.
    Denied,
    /// The phone does not support artwork.
    Unsupported,
    /// The active transfer was rejected.
    Invalid,
}

#[derive(Clone, Copy)]
struct Active {
    id: u8,
    nonce: u32,
}

#[derive(Clone, Copy)]
struct Manifest {
    generation: u64,
    crc32: u32,
}

/// A fixed-size artwork receiver.
///
/// `Receiver` is deliberately `const`-constructible so an app can keep it in static storage:
/// `static mut ARTWORK: artwork::Receiver = artwork::Receiver::new();`.
pub struct Receiver {
    /// Exactly two image buffers are kept.  The active buffer is selected by an index, so a
    /// successful commit is a single index change rather than a copy through the stack.
    buffers: [[u8; PAYLOAD_LEN]; 2],
    current_buffer: usize,
    target_buffer: usize,
    current_generation: Option<u64>,
    visible: bool,
    active: Option<Active>,
    manifest_seen: bool,
    manifest: Manifest,
    seen: [bool; CHUNK_COUNT],
    status: Status,
}

impl Receiver {
    /// Creates an empty receiver without allocating.
    pub const fn new() -> Self {
        Receiver {
            buffers: [[0; PAYLOAD_LEN]; 2],
            current_buffer: 0,
            target_buffer: 1,
            current_generation: None,
            visible: false,
            active: None,
            manifest_seen: false,
            manifest: Manifest {
                generation: 0,
                crc32: 0,
            },
            seen: [false; CHUNK_COUNT],
            status: Status::Idle,
        }
    }

    /// Starts a request and returns the exact bytes to send.
    ///
    /// The request advertises the currently committed generation, or zero when no image is
    /// committed.  Starting a new request cancels any incomplete earlier transfer.
    pub fn begin(&mut self, id: u8, nonce: u32) -> rpc::Request {
        self.clear_transfer();
        self.target_buffer = 1 - self.current_buffer;
        self.active = Some(Active { id, nonce });
        self.status = Status::Pending;
        rpc::artwork_request(id, nonce, self.current_generation.unwrap_or(0))
    }

    /// Handles one M1 reply.
    pub fn handle_reply(&mut self, reply: Reply<'_>) -> Outcome {
        let Some(active) = self.active else {
            return Outcome::Ignored;
        };
        if reply.id != active.id {
            return Outcome::Ignored;
        }

        match reply.status {
            rpc::OK => self.handle_ok(reply.index, reply.total, reply.record, active.nonce),
            rpc::UNAVAILABLE => {
                if is_empty_result(reply.index, reply.total, reply.record) {
                    self.finish(Status::Unavailable, Outcome::Unavailable)
                } else {
                    self.invalid()
                }
            }
            rpc::ASK => {
                if is_empty_result(reply.index, reply.total, reply.record) {
                    self.finish(Status::PermissionRequired, Outcome::PermissionRequired)
                } else {
                    self.invalid()
                }
            }
            rpc::DENIED => {
                if is_empty_result(reply.index, reply.total, reply.record) {
                    self.finish(Status::Denied, Outcome::Denied)
                } else {
                    self.invalid()
                }
            }
            rpc::BAD_REQUEST => {
                if is_empty_result(reply.index, reply.total, reply.record) {
                    self.finish(Status::Unsupported, Outcome::Unsupported)
                } else {
                    self.invalid()
                }
            }
            _ => self.invalid(),
        }
    }

    fn handle_ok(&mut self, index: u8, total: u8, record: &[u8], nonce: u32) -> Outcome {
        if self.manifest_seen {
            // Once a transfer has started, changing its total is a protocol error rather than
            // an "unchanged" answer.
            if total != RECORD_COUNT {
                return self.invalid();
            }
        } else if total == 0 {
            if index != 0 || !record.is_empty() {
                return self.invalid();
            }
            return self.finish(Status::Unchanged, Outcome::Unchanged);
        } else if total != RECORD_COUNT {
            return self.invalid();
        }

        if !self.manifest_seen {
            // The manifest must be the first record.  A data record before it is not accepted.
            if index != 0 {
                return self.invalid();
            }
            let Some(manifest) = decode_manifest(record, nonce) else {
                return self.invalid();
            };
            self.manifest = manifest;
            self.manifest_seen = true;
            // A manifest proves that this is a replacement transfer. Hide the stale cover while
            // the new image is assembled; the buffer and generation are swapped atomically only
            // after every chunk and the CRC have passed validation.
            self.visible = false;
            return Outcome::Pending;
        }

        // A second manifest is a duplicate index.
        if index == 0 {
            return self.invalid();
        }
        let chunk_index = usize::from(index);
        if chunk_index > CHUNK_COUNT {
            return self.invalid();
        }
        if self.seen[chunk_index - 1] {
            return self.invalid();
        }
        let Some((offset, data)) = decode_chunk(record, nonce, chunk_index) else {
            return self.invalid();
        };

        self.buffers[self.target_buffer][offset..offset + data.len()].copy_from_slice(data);
        self.seen[chunk_index - 1] = true;

        if !self.seen.iter().all(|seen| *seen) {
            return Outcome::Pending;
        }

        let payload = &self.buffers[self.target_buffer];
        if crc32(payload) != self.manifest.crc32 {
            return self.invalid();
        }

        // The old image remains selected until every chunk and the CRC have been checked.
        self.current_buffer = self.target_buffer;
        self.current_generation = Some(self.manifest.generation);
        self.visible = true;
        self.finish(Status::Complete, Outcome::Complete)
    }

    /// Whether a request is waiting for more replies.
    pub fn is_pending(&self) -> bool {
        self.active.is_some()
    }

    /// Whether a new image was completely decoded and committed.
    pub fn is_complete(&self) -> bool {
        self.status == Status::Complete
    }

    /// The status of the latest request.
    pub fn status(&self) -> Status {
        self.status
    }

    /// The generation of the committed image, if any.
    pub fn committed_generation(&self) -> Option<u64> {
        self.current_generation
    }

    /// Whether an image is committed.
    pub fn has_artwork(&self) -> bool {
        self.current_generation.is_some()
    }

    /// Whether the committed image may currently be displayed.
    pub fn is_visible(&self) -> bool {
        self.visible && self.current_generation.is_some()
    }

    /// The committed RGB332 payload.
    pub fn current_payload(&self) -> Option<&[u8; PAYLOAD_LEN]> {
        self.current_generation
            .map(|_| &self.buffers[self.current_buffer])
    }

    /// Looks up one committed pixel and returns its RGB565 colour.
    pub fn current_pixel(&self, x: u8, y: u8) -> Option<u16> {
        if x >= WIDTH || y >= HEIGHT || !self.is_visible() {
            return None;
        }
        let payload = self.current_payload()?;
        let pixel = usize::from(x) + usize::from(y) * usize::from(WIDTH);
        Some(decode_rgb565(unpack_rgb332(payload, pixel)))
    }

    /// Renders every committed pixel through a caller-provided callback.
    pub fn for_each_pixel(&self, mut render: impl FnMut(u8, u8, u16)) {
        if !self.is_visible() {
            return;
        }
        let Some(payload) = self.current_payload() else {
            return;
        };
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let pixel = usize::from(x) + usize::from(y) * usize::from(WIDTH);
                render(x, y, decode_rgb565(unpack_rgb332(payload, pixel)));
            }
        }
    }

    /// Ends an incomplete request after a timeout. The committed image and generation are left
    /// untouched for a complete retry, while any stale cover hidden by the failed transfer stays
    /// hidden.
    pub fn timeout(&mut self) -> bool {
        if self.active.is_none() {
            return false;
        }
        self.clear_transfer();
        self.status = Status::TimedOut;
        true
    }

    /// Cancels any pending transfer and returns to the idle state.
    ///
    /// This does not discard the committed image or its generation.
    pub fn reset(&mut self) {
        self.clear_transfer();
        self.status = Status::Idle;
    }

    /// Cancels any transfer and discards the committed image and generation.
    pub fn clear(&mut self) {
        self.reset();
        self.current_generation = None;
        self.visible = false;
    }

    fn finish(&mut self, status: Status, outcome: Outcome) -> Outcome {
        self.clear_transfer();
        match status {
            Status::Complete => self.visible = true,
            Status::Unchanged => self.visible = self.current_generation.is_some(),
            Status::Unavailable
            | Status::PermissionRequired
            | Status::Denied
            | Status::Unsupported => {
                // These outcomes mean there is no usable current image. Reset the generation so a
                // later successful request asks for the complete image instead of an empty reply.
                self.current_generation = None;
                self.visible = false;
            }
            _ => {}
        }
        self.status = status;
        outcome
    }

    fn invalid(&mut self) -> Outcome {
        self.clear_transfer();
        self.status = Status::Invalid;
        Outcome::Invalid
    }

    fn clear_transfer(&mut self) {
        self.active = None;
        self.manifest_seen = false;
        self.seen = [false; CHUNK_COUNT];
    }
}

impl Default for Receiver {
    fn default() -> Self {
        Self::new()
    }
}

fn is_empty_result(index: u8, total: u8, record: &[u8]) -> bool {
    index == 0 && total == 0 && record.is_empty()
}

fn decode_manifest(record: &[u8], expected_nonce: u32) -> Option<Manifest> {
    if record.len() != MANIFEST_LEN
        || record[0] != MANIFEST_VERSION
        || record[1] != ENCODING_RGB332_6BPP
        || record[2] != WIDTH
        || record[3] != HEIGHT
        || record[4] != 0
    {
        return None;
    }

    let nonce = u32::from_le_bytes([record[5], record[6], record[7], record[8]]);
    if nonce != expected_nonce {
        return None;
    }

    let generation = u64::from_le_bytes([
        record[9], record[10], record[11], record[12], record[13], record[14], record[15],
        record[16],
    ]);
    if generation == 0 {
        return None;
    }

    let payload_len = u32::from_le_bytes([record[17], record[18], record[19], record[20]]);
    if payload_len as usize != PAYLOAD_LEN {
        return None;
    }

    let crc32 = u32::from_le_bytes([record[21], record[22], record[23], record[24]]);
    Some(Manifest { generation, crc32 })
}

fn decode_chunk(record: &[u8], expected_nonce: u32, index: usize) -> Option<(usize, &[u8])> {
    if record.len() < 7 {
        return None;
    }
    let nonce = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
    if nonce != expected_nonce {
        return None;
    }

    let offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let expected_offset = (index - 1) * CHUNK_LEN;
    if offset != expected_offset {
        return None;
    }

    let expected_len = if index == CHUNK_COUNT {
        LAST_CHUNK_LEN
    } else {
        CHUNK_LEN
    };
    let data = &record[6..];
    if data.len() != expected_len {
        return None;
    }
    Some((offset, data))
}

/// Extracts one 6-bit RGB332 code from a continuously packed payload.
fn unpack_rgb332(payload: &[u8; PAYLOAD_LEN], pixel: usize) -> u8 {
    let bit_offset = pixel * 6;
    let mut code = 0_u8;
    for bit in 0..6 {
        let source = bit_offset + bit;
        let value = (payload[source / 8] >> (7 - (source % 8))) & 1;
        code = (code << 1) | value;
    }
    code
}

/// Decodes a six-bit RGB332-style code (3-bit red, 2-bit green, 1-bit blue).
pub fn decode_rgb565(code: u8) -> u16 {
    let red = (code >> 3) & 0x07;
    let green = (code >> 1) & 0x03;
    let blue = code & 0x01;

    let red = (u16::from(red) * 31 + 3) / 7;
    let green = (u16::from(green) * 63 + 1) / 3;
    let blue = if blue == 0 { 0 } else { 31 };
    ((red & 0x1f) << 11) | ((green & 0x3f) << 5) | (blue & 0x1f)
}

/// CRC-32/ISO-HDLC, matching `java.util.zip.CRC32`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    const ID: u8 = 0x2a;
    const NONCE: u32 = 0x1234_5678;
    const GENERATION: u64 = 0x0102_0304_0506_0708;

    fn ok(index: u8, total: u8, record: &[u8]) -> Reply<'_> {
        Reply {
            id: ID,
            status: rpc::OK,
            index,
            total,
            record,
        }
    }

    fn reply(status: u8, index: u8, total: u8, record: &[u8]) -> Reply<'_> {
        Reply {
            id: ID,
            status,
            index,
            total,
            record,
        }
    }

    fn payload(seed: u8) -> [u8; PAYLOAD_LEN] {
        let mut payload = [0_u8; PAYLOAD_LEN];
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = seed.wrapping_add(index as u8).wrapping_mul(17);
        }
        payload
    }

    fn manifest(nonce: u32, generation: u64, payload: &[u8]) -> [u8; MANIFEST_LEN] {
        let mut record = [0_u8; MANIFEST_LEN];
        record[0] = MANIFEST_VERSION;
        record[1] = ENCODING_RGB332_6BPP;
        record[2] = WIDTH;
        record[3] = HEIGHT;
        record[4] = 0;
        record[5..9].copy_from_slice(&nonce.to_le_bytes());
        record[9..17].copy_from_slice(&generation.to_le_bytes());
        record[17..21].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        record[21..25].copy_from_slice(&crc32(payload).to_le_bytes());
        record
    }

    fn chunk(nonce: u32, index: usize, payload: &[u8]) -> Vec<u8> {
        let offset = (index - 1) * CHUNK_LEN;
        let len = if index == CHUNK_COUNT {
            LAST_CHUNK_LEN
        } else {
            CHUNK_LEN
        };
        let mut record = Vec::with_capacity(6 + len);
        record.extend_from_slice(&nonce.to_le_bytes());
        record.extend_from_slice(&(offset as u16).to_le_bytes());
        record.extend_from_slice(&payload[offset..offset + len]);
        record
    }

    fn send_manifest(receiver: &mut Receiver, record: &[u8]) -> Outcome {
        receiver.handle_reply(ok(0, RECORD_COUNT, record))
    }

    fn send_chunk(receiver: &mut Receiver, index: usize, payload: &[u8]) -> Outcome {
        let record = chunk(NONCE, index, payload);
        receiver.handle_reply(ok(index as u8, RECORD_COUNT, &record))
    }

    fn commit(receiver: &mut Receiver, payload: &[u8], generation: u64) -> Outcome {
        let record = manifest(NONCE, generation, payload);
        assert_eq!(send_manifest(receiver, &record), Outcome::Pending);
        for index in 1..CHUNK_COUNT {
            assert_eq!(send_chunk(receiver, index, payload), Outcome::Pending);
        }
        send_chunk(receiver, CHUNK_COUNT, payload)
    }

    #[test]
    fn failed_replacement_transfer_forces_a_complete_retry() {
        let payload = payload(11);
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(commit(&mut receiver, &payload, GENERATION), Outcome::Complete);
        assert!(receiver.is_visible());

        receiver.begin(ID, NONCE + 1);
        let record = manifest(NONCE + 1, GENERATION + 1, &payload);
        assert_eq!(send_manifest(&mut receiver, &record), Outcome::Pending);
        assert!(!receiver.is_visible());
        assert!(receiver.timeout());

        let request = receiver.begin(ID, NONCE + 2);
        assert_eq!(&request.bytes()[9..], &GENERATION.to_le_bytes());
        assert_eq!(
            receiver.handle_reply(ok(0, 0, &[])),
            Outcome::Unchanged
        );
        assert!(receiver.is_visible());
    }

    #[test]
    fn request_bytes_are_exact_and_advertise_the_committed_generation() {
        let mut receiver = Receiver::new();
        let request = receiver.begin(ID, NONCE);
        assert_eq!(
            request.bytes(),
            &[
                b'M',
                b'1',
                b'Q',
                ID,
                rpc::MUSIC_ARTWORK,
                0x78,
                0x56,
                0x34,
                0x12,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
        );

        let payload = payload(3);
        assert_eq!(
            commit(&mut receiver, &payload, GENERATION),
            Outcome::Complete
        );
        let request = receiver.begin(ID, NONCE + 1);
        assert_eq!(&request.bytes()[9..], &GENERATION.to_le_bytes());
    }

    #[test]
    fn manifest_fields_are_strictly_validated() {
        let payload = payload(9);
        let valid = manifest(NONCE, GENERATION, &payload);
        let mutations: [(usize, u8); 7] =
            [(0, 2), (1, 2), (2, 63), (3, 63), (4, 1), (5, 0), (18, 1)];

        for (position, value) in mutations {
            let mut receiver = Receiver::new();
            receiver.begin(ID, NONCE);
            let mut record = valid;
            record[position] = value;
            assert_eq!(send_manifest(&mut receiver, &record), Outcome::Invalid);
            assert_eq!(receiver.status(), Status::Invalid);
            assert!(!receiver.has_artwork());
        }

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        let mut zero_generation = valid;
        zero_generation[9..17].fill(0);
        assert_eq!(
            send_manifest(&mut receiver, &zero_generation),
            Outcome::Invalid
        );

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            send_manifest(&mut receiver, &valid[..MANIFEST_LEN - 1]),
            Outcome::Invalid
        );
        let mut too_long = Vec::from(valid);
        too_long.push(0);
        receiver.begin(ID, NONCE);
        assert_eq!(send_manifest(&mut receiver, &too_long), Outcome::Invalid);
    }

    #[test]
    fn in_order_and_out_of_order_chunks_commit_atomically() {
        let payload = payload(21);
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        let record = manifest(NONCE, GENERATION, &payload);
        assert_eq!(send_manifest(&mut receiver, &record), Outcome::Pending);

        for index in 1..CHUNK_COUNT {
            assert_eq!(send_chunk(&mut receiver, index, &payload), Outcome::Pending);
        }
        assert_eq!(
            send_chunk(&mut receiver, CHUNK_COUNT, &payload),
            Outcome::Complete
        );
        assert_eq!(receiver.status(), Status::Complete);

        // The protocol has no separate end marker: the last received chunk completes the set.
        // Feed a fresh transfer in a deliberately scrambled order.
        receiver.begin(ID, NONCE + 1);
        let record = manifest(NONCE + 1, GENERATION + 1, &payload);
        assert_eq!(send_manifest(&mut receiver, &record), Outcome::Pending);
        for index in [
            5, 1, 27, 2, 26, 3, 25, 4, 24, 6, 23, 7, 22, 8, 21, 9, 20, 10, 19, 11, 18, 12, 17, 13,
            16, 14, 15,
        ] {
            let offset = (index - 1) * CHUNK_LEN;
            let len = if index == CHUNK_COUNT {
                LAST_CHUNK_LEN
            } else {
                CHUNK_LEN
            };
            let mut record = Vec::new();
            record.extend_from_slice(&(NONCE + 1).to_le_bytes());
            record.extend_from_slice(&(offset as u16).to_le_bytes());
            record.extend_from_slice(&payload[offset..offset + len]);
            let outcome = receiver.handle_reply(ok(index as u8, RECORD_COUNT, &record));
            if index == 15 {
                assert_eq!(outcome, Outcome::Complete);
            } else {
                assert_eq!(outcome, Outcome::Pending);
            }
        }
        assert_eq!(receiver.status(), Status::Complete);
        assert_eq!(receiver.committed_generation(), Some(GENERATION + 1));
        assert_eq!(receiver.current_payload(), Some(&payload));
    }

    #[test]
    fn duplicate_index_invalidates_the_transfer() {
        let payload = payload(31);
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        let record = manifest(NONCE, GENERATION, &payload);
        assert_eq!(send_manifest(&mut receiver, &record), Outcome::Pending);
        assert_eq!(send_chunk(&mut receiver, 1, &payload), Outcome::Pending);
        assert_eq!(send_chunk(&mut receiver, 1, &payload), Outcome::Invalid);
        assert_eq!(receiver.status(), Status::Invalid);
        assert!(!receiver.has_artwork());
    }

    #[test]
    fn wrong_nonce_offset_length_total_and_index_are_rejected() {
        let payload = payload(41);
        let valid_manifest = manifest(NONCE, GENERATION, &payload);

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        let mut wrong_nonce = valid_manifest;
        wrong_nonce[5] ^= 1;
        assert_eq!(send_manifest(&mut receiver, &wrong_nonce), Outcome::Invalid);

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            send_manifest(&mut receiver, &valid_manifest),
            Outcome::Pending
        );
        let mut record = chunk(NONCE, 1, &payload);
        record[0] ^= 1;
        assert_eq!(
            receiver.handle_reply(ok(1, RECORD_COUNT, &record)),
            Outcome::Invalid
        );

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            send_manifest(&mut receiver, &valid_manifest),
            Outcome::Pending
        );
        let mut record = chunk(NONCE, 1, &payload);
        record[4] ^= 1;
        assert_eq!(
            receiver.handle_reply(ok(1, RECORD_COUNT, &record)),
            Outcome::Invalid
        );

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            send_manifest(&mut receiver, &valid_manifest),
            Outcome::Pending
        );
        let mut record = chunk(NONCE, 1, &payload);
        record.pop();
        assert_eq!(
            receiver.handle_reply(ok(1, RECORD_COUNT, &record)),
            Outcome::Invalid
        );

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            send_manifest(&mut receiver, &valid_manifest),
            Outcome::Pending
        );
        assert_eq!(
            receiver.handle_reply(ok(1, RECORD_COUNT - 1, &chunk(NONCE, 1, &payload))),
            Outcome::Invalid
        );

        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            send_manifest(&mut receiver, &valid_manifest),
            Outcome::Pending
        );
        assert_eq!(
            receiver.handle_reply(ok(28, RECORD_COUNT, &chunk(NONCE, 27, &payload))),
            Outcome::Invalid
        );
    }

    #[test]
    fn a_missing_final_chunk_times_out_without_changing_the_image() {
        let old_payload = payload(51);
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            commit(&mut receiver, &old_payload, GENERATION),
            Outcome::Complete
        );

        let new_payload = payload(52);
        receiver.begin(ID, NONCE + 1);
        let record = manifest(NONCE + 1, GENERATION + 1, &new_payload);
        assert_eq!(send_manifest(&mut receiver, &record), Outcome::Pending);
        assert!(!receiver.is_visible());
        for index in 1..CHUNK_COUNT {
            let offset = (index - 1) * CHUNK_LEN;
            let mut chunk = Vec::new();
            chunk.extend_from_slice(&(NONCE + 1).to_le_bytes());
            chunk.extend_from_slice(&(offset as u16).to_le_bytes());
            chunk.extend_from_slice(&new_payload[offset..offset + CHUNK_LEN]);
            assert_eq!(
                receiver.handle_reply(ok(index as u8, RECORD_COUNT, &chunk)),
                Outcome::Pending
            );
        }
        assert!(receiver.is_pending());
        assert!(receiver.timeout());
        assert_eq!(receiver.status(), Status::TimedOut);
        assert_eq!(receiver.committed_generation(), Some(GENERATION));
        assert_eq!(receiver.current_payload(), Some(&old_payload));
        assert!(!receiver.is_visible());
    }

    #[test]
    fn crc_failure_keeps_the_old_image_and_a_valid_transfer_replaces_it() {
        let old_payload = payload(61);
        let new_payload = payload(62);
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(
            commit(&mut receiver, &old_payload, GENERATION),
            Outcome::Complete
        );

        receiver.begin(ID, NONCE + 1);
        let mut bad_manifest = manifest(NONCE + 1, GENERATION + 1, &new_payload);
        bad_manifest[21] ^= 1;
        assert_eq!(
            send_manifest(&mut receiver, &bad_manifest),
            Outcome::Pending
        );
        assert!(!receiver.is_visible());
        for index in 1..CHUNK_COUNT {
            let offset = (index - 1) * CHUNK_LEN;
            let len = CHUNK_LEN;
            let mut record = Vec::new();
            record.extend_from_slice(&(NONCE + 1).to_le_bytes());
            record.extend_from_slice(&(offset as u16).to_le_bytes());
            record.extend_from_slice(&new_payload[offset..offset + len]);
            assert_eq!(
                receiver.handle_reply(ok(index as u8, RECORD_COUNT, &record)),
                Outcome::Pending
            );
        }
        let offset = (CHUNK_COUNT - 1) * CHUNK_LEN;
        let mut record = Vec::new();
        record.extend_from_slice(&(NONCE + 1).to_le_bytes());
        record.extend_from_slice(&(offset as u16).to_le_bytes());
        record.extend_from_slice(&new_payload[offset..offset + LAST_CHUNK_LEN]);
        assert_eq!(
            receiver.handle_reply(ok(CHUNK_COUNT as u8, RECORD_COUNT, &record)),
            Outcome::Invalid
        );
        assert_eq!(receiver.status(), Status::Invalid);
        assert_eq!(receiver.committed_generation(), Some(GENERATION));
        assert_eq!(receiver.current_payload(), Some(&old_payload));
        assert!(!receiver.is_visible());

        receiver.begin(ID, NONCE + 2);
        let record = manifest(NONCE + 2, GENERATION + 2, &new_payload);
        assert_eq!(send_manifest(&mut receiver, &record), Outcome::Pending);
        for index in 1..=CHUNK_COUNT {
            let offset = (index - 1) * CHUNK_LEN;
            let len = if index == CHUNK_COUNT {
                LAST_CHUNK_LEN
            } else {
                CHUNK_LEN
            };
            let mut record = Vec::new();
            record.extend_from_slice(&(NONCE + 2).to_le_bytes());
            record.extend_from_slice(&(offset as u16).to_le_bytes());
            record.extend_from_slice(&new_payload[offset..offset + len]);
            receiver.handle_reply(ok(index as u8, RECORD_COUNT, &record));
        }
        assert_eq!(receiver.status(), Status::Complete);
        assert_eq!(receiver.committed_generation(), Some(GENERATION + 2));
        assert_eq!(receiver.current_payload(), Some(&new_payload));
        assert!(receiver.is_visible());
    }

    #[test]
    fn unchanged_unavailable_permission_and_unsupported_results_are_distinct() {
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        assert_eq!(receiver.handle_reply(ok(0, 0, &[])), Outcome::Unchanged);
        assert_eq!(receiver.status(), Status::Unchanged);
        assert!(!receiver.has_artwork());

        receiver.begin(ID, NONCE + 1);
        assert_eq!(
            receiver.handle_reply(reply(rpc::UNAVAILABLE, 0, 0, &[])),
            Outcome::Unavailable
        );
        assert_eq!(receiver.status(), Status::Unavailable);

        receiver.begin(ID, NONCE + 2);
        assert_eq!(
            receiver.handle_reply(reply(rpc::ASK, 0, 0, &[])),
            Outcome::PermissionRequired
        );
        assert_eq!(receiver.status(), Status::PermissionRequired);

        receiver.begin(ID, NONCE + 3);
        assert_eq!(
            receiver.handle_reply(reply(rpc::DENIED, 0, 0, &[])),
            Outcome::Denied
        );
        assert_eq!(receiver.status(), Status::Denied);

        receiver.begin(ID, NONCE + 4);
        assert_eq!(
            receiver.handle_reply(reply(rpc::BAD_REQUEST, 0, 0, &[])),
            Outcome::Unsupported
        );
        assert_eq!(receiver.status(), Status::Unsupported);
    }

    #[test]
    fn wrong_id_is_ignored_without_changing_pending_state() {
        let mut receiver = Receiver::new();
        receiver.begin(ID, NONCE);
        let response = Reply {
            id: ID + 1,
            status: rpc::OK,
            index: 0,
            total: 0,
            record: &[],
        };
        assert_eq!(receiver.handle_reply(response), Outcome::Ignored);
        assert!(receiver.is_pending());
        assert_eq!(receiver.status(), Status::Pending);
    }

    #[test]
    fn rgb332_golden_decode_handles_continuous_msb_first_packing() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(decode_rgb565(0x00), 0x0000);
        assert_eq!(decode_rgb565(0x06), 0x07e0);
        assert_eq!(decode_rgb565(0x38), 0xf800);
        assert_eq!(decode_rgb565(0x01), 0x001f);
        assert_eq!(decode_rgb565(0x3f), 0xffff);

        let mut packed = [0_u8; PAYLOAD_LEN];
        packed[..3].copy_from_slice(&[0x00, 0x1f, 0xe0]);
        assert_eq!(unpack_rgb332(&packed, 0), 0x00);
        assert_eq!(unpack_rgb332(&packed, 1), 0x01);
        assert_eq!(unpack_rgb332(&packed, 2), 0x3f);
        assert_eq!(unpack_rgb332(&packed, 3), 0x20);
    }
}

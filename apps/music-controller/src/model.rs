//! The phone-music state machine: requests, optimistic commands and the local cache.
//!
//! One RPC request is in flight at a time. Commands tapped while a snapshot is in flight wait in a
//! small local queue. The watch accepts the current title, artist, and a low-resolution artwork
//! thumbnail; those values stay in memory and are never written to the local cache.

use crate::artwork;
use crate::rpc::{self, Metadata};
use crate::track;

pub const POLL_MS: u32 = 2_000;
pub const RETRY_MS: u32 = 3_000;
pub const ASK_AGAIN_MS: u32 = 10_000;
pub const COMMAND_REFRESH_MS: u32 = 250;
pub const SNAPSHOT_TIMEOUT_MS: u32 = 10_000;
pub const COMMAND_TIMEOUT_MS: u32 = 8_000;
pub const TRACK_TIMEOUT_MS: u32 = 10_000;
pub const ARTWORK_TIMEOUT_MS: u32 = 30_000;
pub const TRACK_POLL_MS: u32 = 2_000;
pub const ARTWORK_POLL_MS: u32 = 2_000;
pub const ARTWORK_NO_IMAGE_MS: u32 = 10_000;
pub const ARTWORK_UNSUPPORTED_MS: u32 = 30_000;
pub const MAX_BACKOFF_MS: u32 = 30_000;

const COMMAND_QUEUE_MAX: usize = 8;
/// Maximum printable-ASCII bytes retained for the title and artist.
pub const TRACK_TEXT_MAX: usize = 60;
const MAGIC: &[u8; 2] = b"M2";
/// Magic, fetched wall minute, then the four-byte metadata record.
pub const BLOB_MAX: usize = 2 + 4 + 4;

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Status {
    Waiting,
    Ok,
    Ask,
    Denied,
    Unavailable,
    /// The latest request got no answer.
    Silent,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Got {
    Nothing,
    /// The screen changed.
    Changed,
    /// Metadata changed and should be written to the watch's local storage.
    Save,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
enum Active {
    Snapshot {
        id: u8,
        started: u32,
    },
    Track {
        id: u8,
        started: u32,
    },
    Artwork {
        id: u8,
        nonce: u32,
        started: u32,
    },
    Command {
        id: u8,
        command: u8,
        value: u8,
        before: Metadata,
        started: u32,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
struct Command {
    code: u8,
    value: u8,
    before: Metadata,
}

pub struct Model {
    pub metadata: Metadata,
    pub has_snapshot: bool,
    pub status: Status,
    /// Wall minute of the last committed snapshot.
    pub fetched: Option<i32>,
    /// Bumped on every visible change.
    pub rev: u32,

    title: [u8; TRACK_TEXT_MAX],
    title_len: u8,
    artist: [u8; TRACK_TEXT_MAX],
    artist_len: u8,

    id: u8,
    next_poll: u32,
    next_command: u32,
    next_track: u32,
    next_artwork: u32,
    artwork_nonce: u32,
    artwork_backoff: u32,
    backoff: u32,
    active: Option<Active>,
    artwork: artwork::Receiver,
    pending_title: [u8; TRACK_TEXT_MAX],
    pending_title_len: u8,
    pending_artist: [u8; TRACK_TEXT_MAX],
    pending_artist_len: u8,
    track_seen: u8,
    commands: [Command; COMMAND_QUEUE_MAX],
    command_head: usize,
    /// Pending commands, including the active command while it is at the queue head.
    command_len: usize,
    cache_dirty: bool,
}

impl Model {
    pub const fn new() -> Self {
        Model {
            metadata: Metadata::EMPTY,
            has_snapshot: false,
            status: Status::Waiting,
            fetched: None,
            rev: 0,
            title: [0; TRACK_TEXT_MAX],
            title_len: 0,
            artist: [0; TRACK_TEXT_MAX],
            artist_len: 0,
            id: 0,
            next_poll: 0,
            next_command: 0,
            next_track: TRACK_POLL_MS,
            next_artwork: ARTWORK_POLL_MS,
            artwork_nonce: 0,
            artwork_backoff: 1_000,
            backoff: RETRY_MS,
            active: None,
            artwork: artwork::Receiver::new(),
            pending_title: [0; TRACK_TEXT_MAX],
            pending_title_len: 0,
            pending_artist: [0; TRACK_TEXT_MAX],
            pending_artist_len: 0,
            track_seen: 0,
            commands: [Command {
                code: 0,
                value: 0,
                before: Metadata::EMPTY,
            }; COMMAND_QUEUE_MAX],
            command_head: 0,
            command_len: 0,
            cache_dirty: false,
        }
    }

    /// Ask at once (start or RESUME). A reply from the previous run has a different id and is ignored.
    pub fn ask_now(&mut self, now: u32) {
        self.active = None;
        self.artwork.reset();
        self.backoff = RETRY_MS;
        self.next_poll = now;
        self.next_track = now.wrapping_add(TRACK_POLL_MS);
        self.next_artwork = now.wrapping_add(ARTWORK_POLL_MS);
        if self.command_len > 0 {
            self.next_command = now;
        }
        self.rev += 1;
    }

    /// Whether the current metadata represents an active media session.
    pub fn active(&self) -> bool {
        self.metadata.active()
    }

    /// The current music stream volume.
    pub fn volume(&self) -> u8 {
        self.metadata.volume
    }

    /// The maximum music stream volume.
    pub fn max_volume(&self) -> u8 {
        self.metadata.max_volume
    }

    /// Whether the current media session is playing.
    pub fn playing(&self) -> bool {
        self.metadata.playing()
    }

    /// The current track title, kept only in memory.
    pub fn track_title(&self) -> &str {
        core::str::from_utf8(&self.title[..self.title_len as usize]).unwrap_or("")
    }

    /// The current track artist, kept only in memory.
    pub fn track_artist(&self) -> &str {
        core::str::from_utf8(&self.artist[..self.artist_len as usize]).unwrap_or("")
    }

    /// Replace the in-memory title and artist. Returns whether the visible text changed.
    pub fn set_track_text(&mut self, title: &[u8], artist: &[u8]) -> bool {
        let title_len = title.len().min(TRACK_TEXT_MAX);
        let artist_len = artist.len().min(TRACK_TEXT_MAX);
        let changed = self.title_len as usize != title_len
            || self.artist_len as usize != artist_len
            || self.title[..title_len] != title[..title_len]
            || self.artist[..artist_len] != artist[..artist_len];
        if changed {
            self.title[..title_len].copy_from_slice(&title[..title_len]);
            self.artist[..artist_len].copy_from_slice(&artist[..artist_len]);
            self.title_len = title_len as u8;
            self.artist_len = artist_len as u8;
            self.rev += 1;
        }
        changed
    }

    /// Whether a complete artwork image may currently be displayed.
    pub fn artwork_visible(&self) -> bool {
        self.artwork.is_visible()
    }

    /// The committed artwork generation, or zero when there is none.
    pub fn artwork_generation(&self) -> u64 {
        self.artwork.committed_generation().unwrap_or(0)
    }

    /// One artwork pixel in RGB565.
    pub fn artwork_pixel(&self, x: u8, y: u8) -> Option<u16> {
        self.artwork.current_pixel(x, y)
    }

    /// Transport controls are live only while the phone has confirmed an active session.
    pub fn controls_enabled(&self) -> bool {
        self.status == Status::Ok && self.active()
    }

    /// Volume controls require a live session and a known non-zero maximum.
    pub fn volume_enabled(&self) -> bool {
        self.controls_enabled() && self.max_volume() > 0
    }

    /// The next request to send, if any.
    pub fn poll(&mut self, now: u32) -> Option<rpc::Request> {
        if let Some(active) = self.active {
            if now.wrapping_sub(started_at(active)) >= timeout_for(active) {
                match active {
                    Active::Command { .. } => self.send_failed(now),
                    Active::Snapshot { .. } => {
                        self.active = None;
                        self.status = Status::Silent;
                        self.backoff = (self.backoff * 2).min(MAX_BACKOFF_MS);
                        self.next_poll = now.wrapping_add(self.backoff);
                        self.rev += 1;
                    }
                    Active::Track { .. } => {
                        self.active = None;
                        self.next_track = now.wrapping_add(TRACK_POLL_MS);
                    }
                    Active::Artwork { .. } => {
                        let was_visible = self.artwork.is_visible();
                        self.active = None;
                        self.artwork.timeout();
                        self.next_artwork = now.wrapping_add(self.artwork_backoff);
                        self.artwork_backoff = (self.artwork_backoff * 2).min(MAX_BACKOFF_MS);
                        if was_visible != self.artwork.is_visible() {
                            self.rev += 1;
                        }
                    }
                }
            }
            return None;
        }

        if self.command_len > 0 && due(now, self.next_command) {
            // Keep the active command at the queue head until a valid reply completes it. If it
            // times out or cannot be sent, the same slot is retried before later commands.
            let command = self.commands[self.command_head];
            self.id = self.id.wrapping_add(1);
            self.active = Some(Active::Command {
                id: self.id,
                command: command.code,
                value: command.value,
                before: command.before,
                started: now,
            });
            return Some(rpc::command_request(self.id, command.code, command.value));
        }

        if due(now, self.next_poll) {
            self.id = self.id.wrapping_add(1);
            self.active = Some(Active::Snapshot {
                id: self.id,
                started: now,
            });
            return Some(rpc::snapshot_request(self.id));
        }

        // Track text and artwork are meaningful only while a confirmed media session exists.
        // A no-media snapshot therefore also suppresses both follow-up requests.
        if self.has_snapshot && self.status == Status::Ok && self.active() && due(now, self.next_track) {
            self.id = self.id.wrapping_add(1);
            self.active = Some(Active::Track {
                id: self.id,
                started: now,
            });
            self.track_seen = 0;
            self.pending_title_len = 0;
            self.pending_artist_len = 0;
            return Some(rpc::track_request(self.id));
        }

        if self.has_snapshot && self.status == Status::Ok && self.active() && due(now, self.next_artwork) {
            self.id = self.id.wrapping_add(1);
            self.artwork_nonce = self.artwork_nonce.wrapping_add(1).max(1);
            self.active = Some(Active::Artwork {
                id: self.id,
                nonce: self.artwork_nonce,
                started: now,
            });
            return Some(self.artwork.begin(self.id, self.artwork_nonce));
        }

        None
    }

    /// Queue a playback or volume command and update the visible state immediately.
    pub fn command(&mut self, command: u8, value: u8) -> bool {
        // The active command remains in the queue, so this bounds all pending commands rather
        // than allowing an in-flight command plus a full queue of later commands.
        if self.command_len == COMMAND_QUEUE_MAX
            || !matches!(
                command,
                rpc::PLAY
                    | rpc::PAUSE
                    | rpc::TOGGLE
                    | rpc::PREVIOUS
                    | rpc::NEXT
                    | rpc::VOLUME_UP
                    | rpc::VOLUME_DOWN
                    | rpc::SET_VOLUME
            )
        {
            return false;
        }

        let old = self.metadata;
        let before = old;
        apply_command(&mut self.metadata, command, value);

        let at = (self.command_head + self.command_len) % COMMAND_QUEUE_MAX;
        self.commands[at] = Command {
            code: command,
            value,
            before,
        };
        if self.command_len == 0 {
            self.next_command = 0;
        }
        self.command_len += 1;

        if !old.same_data(&self.metadata) {
            self.cache_dirty = true;
            self.rev += 1;
        }
        true
    }

    /// Recover immediately when the loader could not queue a request. An active command stays at
    /// the front of the bounded queue so a transient outbox or host failure does not drop it or
    /// move it behind commands that were entered later.
    pub fn send_failed(&mut self, now: u32) {
        let Some(active) = self.active.take() else {
            return;
        };
        match active {
            Active::Snapshot { .. } => {
                self.next_poll = now.wrapping_add(self.backoff);
                self.status = Status::Silent;
                self.backoff = (self.backoff * 2).min(MAX_BACKOFF_MS);
                self.rev += 1;
            }
            Active::Command { .. } => {
                self.next_command = now.wrapping_add(self.backoff);
                self.next_poll = now.wrapping_add(self.backoff);
                self.status = Status::Silent;
                self.backoff = (self.backoff * 2).min(MAX_BACKOFF_MS);
                self.rev += 1;
            }
            Active::Track { .. } => {
                self.next_track = now.wrapping_add(TRACK_POLL_MS);
            }
            Active::Artwork { .. } => {
                let was_visible = self.artwork.is_visible();
                self.artwork.timeout();
                self.next_artwork = now.wrapping_add(self.artwork_backoff);
                self.artwork_backoff = (self.artwork_backoff * 2).min(MAX_BACKOFF_MS);
                if was_visible != self.artwork.is_visible() {
                    self.rev += 1;
                }
            }
        }
    }

    /// Handle one host message.
    pub fn reply(&mut self, message: &[u8], now: u32, wall: i32) -> Got {
        let Some(reply) = rpc::reply(message) else {
            return Got::Nothing;
        };
        let Some(active) = self.active else {
            return Got::Nothing;
        };
        match active {
            Active::Command { id, before, .. } if id == reply.id => {
                self.active = None;
                self.remove_active_command();
                self.next_poll = now.wrapping_add(COMMAND_REFRESH_MS);
                self.next_command = now.wrapping_add(COMMAND_REFRESH_MS);

                let malformed = !reply.is_command_result();
                self.status = if malformed {
                    Status::Unavailable
                } else if reply.status == rpc::OK {
                    Status::Ok
                } else {
                    status_of(reply.status)
                };
                // A malformed result is not a successful command. Treat it like a rejected
                // command so the optimistic effect cannot survive without a valid acknowledgement.
                let rolled_back =
                    (malformed || reply.status != rpc::OK) && self.rollback_command(before);
                self.rev += 1;
                if rolled_back {
                    self.cache_dirty = true;
                    Got::Save
                } else {
                    Got::Changed
                }
            }
            Active::Snapshot { id, .. } if id == reply.id => self.snapshot(reply, now, wall),
            Active::Track { id, .. } if id == reply.id => self.track(reply, now),
            Active::Artwork { id, .. } if id == reply.id => self.artwork_reply(reply, now),
            _ => Got::Nothing,
        }
    }

    /// Remove the command currently at the queue head after a terminal reply.
    fn remove_active_command(&mut self) {
        debug_assert!(self.command_len > 0);
        self.command_head = (self.command_head + 1) % COMMAND_QUEUE_MAX;
        self.command_len -= 1;
    }

    /// Restore the state before a denied command, then reapply commands still queued behind it.
    fn rollback_command(&mut self, before: Metadata) -> bool {
        let old = self.metadata;
        self.metadata = before;
        self.reapply_queued_commands();
        !old.same_data(&self.metadata)
    }

    /// Reapply queued optimistic commands and record each command's new rollback baseline.
    fn reapply_queued_commands(&mut self) {
        let mut state = self.metadata;
        for offset in 0..self.command_len {
            let index = (self.command_head + offset) % COMMAND_QUEUE_MAX;
            let mut command = self.commands[index];
            command.before = state;
            apply_command(&mut state, command.code, command.value);
            self.commands[index] = command;
        }
        self.metadata = state;
    }

    fn track(&mut self, reply: rpc::Reply<'_>, now: u32) -> Got {
        if reply.status != rpc::OK {
            // Error replies have one exact empty shape. Any other shape is malformed and must not
            // be interpreted as a meaningful permission or compatibility response.
            if reply.index != 0 || reply.total != 0 || !reply.record.is_empty() {
                return self.reject_track(now);
            }
            self.active = None;
            let retry = match reply.status {
                rpc::BAD_REQUEST => ARTWORK_UNSUPPORTED_MS,
                rpc::ASK | rpc::DENIED => ASK_AGAIN_MS,
                _ => RETRY_MS,
            };
            self.next_track = now.wrapping_add(retry);
            self.clear_track_text();
            return Got::Changed;
        }
        if reply.total != track::TRACK_TOTAL || reply.index >= track::TRACK_TOTAL {
            return self.reject_track(now);
        }
        let Some((field, text)) = track::decode(reply.record) else {
            return self.reject_track(now);
        };
        let expected = if reply.index == 0 {
            track::Field::Title
        } else {
            track::Field::Artist
        };
        if field != expected {
            return self.reject_track(now);
        }

        let bit = if field == track::Field::Title { 1 } else { 2 };
        if self.track_seen & bit != 0 {
            return self.reject_track(now);
        }
        if field == track::Field::Title {
            self.pending_title[..text.len()].copy_from_slice(text);
            self.pending_title_len = text.len() as u8;
        } else {
            self.pending_artist[..text.len()].copy_from_slice(text);
            self.pending_artist_len = text.len() as u8;
        }
        self.track_seen |= bit;

        if self.track_seen != 3 {
            return Got::Nothing;
        }

        self.active = None;
        self.next_track = now.wrapping_add(TRACK_POLL_MS);
        let mut title = [0_u8; TRACK_TEXT_MAX];
        let mut artist = [0_u8; TRACK_TEXT_MAX];
        title[..self.pending_title_len as usize]
            .copy_from_slice(&self.pending_title[..self.pending_title_len as usize]);
        artist[..self.pending_artist_len as usize]
            .copy_from_slice(&self.pending_artist[..self.pending_artist_len as usize]);
        self.set_track_text(
            &title[..self.pending_title_len as usize],
            &artist[..self.pending_artist_len as usize],
        );
        Got::Changed
    }

    fn reject_track(&mut self, now: u32) -> Got {
        self.active = None;
        self.next_track = now.wrapping_add(TRACK_POLL_MS);
        self.clear_track_text();
        Got::Changed
    }

    fn clear_track_text(&mut self) {
        self.set_track_text(&[], &[]);
    }

    fn artwork_reply(&mut self, reply: rpc::Reply<'_>, now: u32) -> Got {
        let was_visible = self.artwork.is_visible();
        let outcome = self.artwork.handle_reply(reply);
        let visible_changed = was_visible != self.artwork.is_visible();
        match outcome {
            artwork::Outcome::Ignored | artwork::Outcome::Pending => {
                if visible_changed {
                    self.rev += 1;
                    Got::Changed
                } else {
                    Got::Nothing
                }
            }
            artwork::Outcome::Complete => {
                self.active = None;
                self.next_artwork = now.wrapping_add(ARTWORK_POLL_MS);
                self.artwork_backoff = 1_000;
                self.rev += 1;
                Got::Changed
            }
            artwork::Outcome::Unchanged => {
                self.active = None;
                self.next_artwork = now.wrapping_add(ARTWORK_POLL_MS);
                self.artwork_backoff = 1_000;
                if visible_changed {
                    self.rev += 1;
                    Got::Changed
                } else {
                    Got::Nothing
                }
            }
            artwork::Outcome::Unavailable => {
                self.active = None;
                self.next_artwork = now.wrapping_add(ARTWORK_NO_IMAGE_MS);
                self.artwork_backoff = 1_000;
                if visible_changed {
                    self.rev += 1;
                    Got::Changed
                } else {
                    Got::Nothing
                }
            }
            artwork::Outcome::PermissionRequired | artwork::Outcome::Denied => {
                self.active = None;
                self.next_artwork = now.wrapping_add(ARTWORK_NO_IMAGE_MS);
                self.artwork_backoff = 1_000;
                if visible_changed {
                    self.rev += 1;
                    Got::Changed
                } else {
                    Got::Nothing
                }
            }
            artwork::Outcome::Unsupported => {
                self.active = None;
                self.next_artwork = now.wrapping_add(ARTWORK_UNSUPPORTED_MS);
                self.artwork_backoff = 1_000;
                if visible_changed {
                    self.rev += 1;
                    Got::Changed
                } else {
                    Got::Nothing
                }
            }
            artwork::Outcome::Invalid => {
                self.active = None;
                self.next_artwork = now.wrapping_add(self.artwork_backoff);
                self.artwork_backoff = (self.artwork_backoff * 2).min(MAX_BACKOFF_MS);
                if visible_changed {
                    self.rev += 1;
                    Got::Changed
                } else {
                    Got::Nothing
                }
            }
        }
    }

    fn snapshot(&mut self, reply: rpc::Reply<'_>, now: u32, wall: i32) -> Got {
        if reply.status != rpc::OK {
            self.active = None;
            self.status = status_of(reply.status);
            self.next_poll = now.wrapping_add(if self.status == Status::Ask {
                ASK_AGAIN_MS
            } else {
                RETRY_MS
            });
            self.rev += 1;
            return Got::Changed;
        }
        if reply.total != 1 || reply.index != 0 {
            return self.reject_snapshot(now);
        }
        let Some(metadata) = Metadata::decode(reply.record) else {
            return self.reject_snapshot(now);
        };
        self.commit(metadata, now, wall)
    }

    fn reject_snapshot(&mut self, now: u32) -> Got {
        self.active = None;
        self.status = Status::Unavailable;
        self.next_poll = now.wrapping_add(RETRY_MS);
        self.rev += 1;
        Got::Changed
    }

    fn commit(&mut self, metadata: Metadata, now: u32, wall: i32) -> Got {
        let old = self.metadata;
        self.metadata = metadata;
        self.reapply_queued_commands();
        self.has_snapshot = true;
        self.fetched = Some(wall);
        self.status = Status::Ok;
        self.backoff = RETRY_MS;
        self.next_poll = now.wrapping_add(POLL_MS);
        if !due(now, self.next_track) {
            self.next_track = now.wrapping_add(TRACK_POLL_MS);
        }
        if !due(now, self.next_artwork) {
            self.next_artwork = now.wrapping_add(ARTWORK_POLL_MS);
        }
        self.active = None;

        // A confirmed no-media state must not retain a previous cover or track label.
        if !self.active() {
            self.clear_track_text();
            self.artwork.clear();
        }

        // Compare the effective state after queued optimistic commands are reapplied. The raw
        // snapshot can equal the old optimistic value even though the final visible state differs.
        if !old.same_data(&self.metadata) {
            self.rev += 1;
            self.cache_dirty = true;
            Got::Save
        } else {
            Got::Changed
        }
    }

    /// The cache blob. It holds playback state and volume only; title, artist, and artwork stay in RAM.
    pub fn save(&mut self, out: &mut [u8]) -> usize {
        if !self.has_snapshot {
            return 0;
        }
        let Some(wall) = self.fetched else { return 0 };
        out[..2].copy_from_slice(MAGIC);
        out[2..6].copy_from_slice(&wall.to_le_bytes());
        let Some(len) = self.metadata.encode(&mut out[6..]) else {
            return 0;
        };
        6 + len
    }

    /// Whether metadata has changed since the last successful storage write.
    pub fn cache_dirty(&self) -> bool {
        self.cache_dirty
    }

    /// Mark the current metadata as durably stored.
    pub fn cache_saved(&mut self) {
        self.cache_dirty = false;
    }

    /// Restore cached playback state; transferred title and artwork are never cached.
    pub fn load(&mut self, blob: &[u8]) {
        if blob.len() < 10 || &blob[..2] != MAGIC {
            return;
        }
        let Some(metadata) = Metadata::decode(&blob[6..10]) else {
            return;
        };
        self.metadata = metadata;
        self.has_snapshot = true;
        self.fetched = Some(i32::from_le_bytes([blob[2], blob[3], blob[4], blob[5]]));
        self.cache_dirty = false;
        self.status = Status::Waiting;
        self.rev += 1;
    }

    /// Ms until the next request or timeout.
    pub fn wait(&self, now: u32) -> u32 {
        if let Some(active) = self.active {
            return deadline_delay(started_at(active).wrapping_add(timeout_for(active)), now)
                .min(timeout_for(active));
        }

        let mut delay = deadline_delay(self.next_poll, now);
        if self.command_len > 0 {
            delay = delay.min(deadline_delay(self.next_command, now));
        }
        // Track and artwork polls are eligible only after a successful snapshot confirms an
        // active media session. Otherwise their deadlines must not wake the event loop.
        if self.has_snapshot && self.status == Status::Ok && self.active() {
            delay = delay.min(deadline_delay(self.next_track, now));
            delay = delay.min(deadline_delay(self.next_artwork, now));
        }
        delay
    }
}

fn apply_command(metadata: &mut Metadata, command: u8, value: u8) {
    match command {
        rpc::PLAY => {
            metadata.flags |= rpc::RESPONSE_PLAYING | rpc::RESPONSE_ACTIVE;
        }
        rpc::PAUSE => metadata.flags &= !rpc::RESPONSE_PLAYING,
        rpc::TOGGLE => {
            if metadata.active() && metadata.playing() {
                metadata.flags &= !rpc::RESPONSE_PLAYING;
            } else {
                metadata.flags |= rpc::RESPONSE_PLAYING | rpc::RESPONSE_ACTIVE;
            }
        }
        rpc::VOLUME_UP => {
            let max = metadata.max_volume;
            metadata.volume = metadata.volume.saturating_add(1).min(max);
        }
        rpc::VOLUME_DOWN => metadata.volume = metadata.volume.saturating_sub(1),
        rpc::SET_VOLUME => metadata.volume = value.min(metadata.max_volume),
        rpc::PREVIOUS | rpc::NEXT => {}
        _ => {}
    }
}

fn due(now: u32, at: u32) -> bool {
    now.wrapping_sub(at) as i32 >= 0
}

/// Delay until a wrap-safe deadline. An overdue deadline is immediately due, never a huge wait.
fn deadline_delay(deadline: u32, now: u32) -> u32 {
    let delta = deadline.wrapping_sub(now);
    if delta as i32 >= 0 {
        delta
    } else {
        0
    }
}

fn started_at(active: Active) -> u32 {
    match active {
        Active::Snapshot { started, .. }
        | Active::Track { started, .. }
        | Active::Artwork { started, .. }
        | Active::Command { started, .. } => started,
    }
}

fn timeout_for(active: Active) -> u32 {
    match active {
        Active::Snapshot { .. } => SNAPSHOT_TIMEOUT_MS,
        Active::Track { .. } => TRACK_TIMEOUT_MS,
        Active::Artwork { .. } => ARTWORK_TIMEOUT_MS,
        Active::Command { .. } => COMMAND_TIMEOUT_MS,
    }
}

fn status_of(status: u8) -> Status {
    match status {
        rpc::OK => Status::Ok,
        rpc::ASK => Status::Ask,
        rpc::DENIED => Status::Denied,
        rpc::BAD_REQUEST | rpc::UNAVAILABLE => Status::Unavailable,
        _ => Status::Unavailable,
    }
}

/// A compact, fixed-size text buffer used by the view.
pub struct Buf<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Buf<N> {
    pub const fn new() -> Self {
        Buf {
            bytes: [0; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }

    fn push_byte(&mut self, byte: u8) {
        if self.len < N {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
    }

    fn push_u8(&mut self, value: u8) {
        if value >= 100 {
            self.push_byte(b'0' + value / 100);
            self.push_byte(b'0' + (value / 10) % 10);
        } else if value >= 10 {
            self.push_byte(b'0' + value / 10);
        }
        self.push_byte(b'0' + value % 10);
    }
}

/// Writes a volume percentage without allocating or pulling in the formatting machinery.
pub fn percent(out: &mut Buf<8>, volume: u8, max: u8) {
    let value = if max == 0 {
        0
    } else {
        (volume as u32 * 100 / max as u32).min(100)
    } as u8;
    out.push_u8(value);
    out.push_byte(b'%');
}

/// Days since 1970-01-01 of a civil date (H. Hinnant's `days_from_civil`).
pub fn days(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i32 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i32 - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// Local wall-clock minutes since 1970-01-01.
pub fn wall_min(y: i32, m: u32, d: u32, hour: u32, minute: u32) -> i32 {
    days(y, m, d) * 1440 + (hour * 60 + minute) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc;

    fn reply(id: u8, status: u8, index: u8, total: u8, record: &[u8]) -> Vec<u8> {
        let mut out = vec![b'M', b'1', b'A', id, status, index, total];
        out.extend_from_slice(record);
        out
    }

    fn metadata_record(flags: u8, volume: u8, max_volume: u8) -> Vec<u8> {
        vec![0, flags, volume, max_volume]
    }

    #[test]
    fn metadata_snapshot_commits() {
        let mut model = Model::new();
        let request = model.poll(0).unwrap();
        let id = match request {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        let record = metadata_record(3, 7, 15);
        assert_eq!(
            model.reply(&reply(id, 0, 0, 1, &record), 10, 100),
            Got::Save
        );
        assert!(model.has_snapshot);
        assert!(model.active() && model.playing());
        assert_eq!((model.volume(), model.max_volume()), (7, 15));
        assert_eq!(model.status, Status::Ok);
    }

    #[test]
    fn malformed_snapshot_is_rejected_without_committing() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        assert_eq!(
            model.reply(&reply(id, 0, 0, 1, &[0, 3, 16, 15]), 10, 100),
            Got::Changed
        );
        assert!(!model.has_snapshot);
        assert_eq!(model.status, Status::Unavailable);
        assert!(model.poll(10 + RETRY_MS).is_some());
    }

    #[test]
    fn commands_are_queued_behind_a_snapshot() {
        let mut model = Model::new();
        model.poll(0);
        assert!(model.command(rpc::TOGGLE, 0));
        assert!(model.metadata.playing());
        assert!(model.poll(10).is_none());
        assert!(model.poll(SNAPSHOT_TIMEOUT_MS).is_none());
        assert_eq!(model.status, Status::Silent);
        let id = match model.poll(SNAPSHOT_TIMEOUT_MS + 1).unwrap() {
            rpc::Request::Command(bytes) => bytes[3],
            _ => panic!("expected queued command"),
        };
        model.reply(&reply(id, 0, 0, 0, &[]), SNAPSHOT_TIMEOUT_MS + 11, 100);
        assert!(model.poll(SNAPSHOT_TIMEOUT_MS + 12).is_none());
        assert!(matches!(
            model.poll(SNAPSHOT_TIMEOUT_MS + 12 + COMMAND_REFRESH_MS + 1),
            Some(rpc::Request::Snapshot(_))
        ));
    }

    #[test]
    fn optimistic_volume_is_clamped() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);
        model.command(rpc::SET_VOLUME, 200);
        assert_eq!(model.volume(), 15);
        model.command(rpc::VOLUME_UP, 0);
        assert_eq!(model.volume(), 15);
        model.command(rpc::VOLUME_DOWN, 0);
        assert_eq!(model.volume(), 14);
    }

    #[test]
    fn queue_overflow_does_not_change_visible_state() {
        let mut model = Model::new();
        for _ in 0..COMMAND_QUEUE_MAX {
            assert!(model.command(rpc::VOLUME_UP, 0));
        }
        let volume = model.volume();
        assert!(!model.command(rpc::VOLUME_UP, 0));
        assert_eq!(model.volume(), volume);
    }

    #[test]
    fn unchanged_metadata_does_not_bump_the_visible_revision() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);
        let rev = model.rev;
        let id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(id, 0, 0, 1, &metadata_record(3, 7, 15)),
            POLL_MS + 11,
            100,
        );
        assert_eq!(model.rev, rev);
    }

    #[test]
    fn cache_round_trip_keeps_non_personal_state() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 1234);
        let mut blob = [0u8; BLOB_MAX];
        let len = model.save(&mut blob);
        assert_eq!(len, BLOB_MAX);
        let mut restored = Model::new();
        restored.load(&blob[..len]);
        assert!(restored.has_snapshot);
        assert!(restored.active() && restored.playing());
        assert_eq!(restored.volume(), 7);
        assert_eq!(restored.fetched, Some(1234));
    }

    #[test]
    fn a_failed_command_is_queued_again() {
        let mut model = Model::new();
        assert!(model.command(rpc::TOGGLE, 0));
        let request = model.poll(0).unwrap();
        assert!(matches!(request, rpc::Request::Command(_)));
        model.send_failed(1);
        assert_eq!(model.status, Status::Silent);
        let retry_at = 1 + RETRY_MS;
        let request = model.poll(retry_at).unwrap();
        assert!(matches!(request, rpc::Request::Command(_)));
    }

    #[test]
    fn a_timed_out_command_is_queued_again() {
        let mut model = Model::new();
        assert!(model.command(rpc::TOGGLE, 0));
        assert!(matches!(model.poll(0), Some(rpc::Request::Command(_))));
        assert!(model.poll(COMMAND_TIMEOUT_MS).is_none());
        assert_eq!(model.status, Status::Silent);
        let retry_at = COMMAND_TIMEOUT_MS + RETRY_MS * 2;
        assert!(matches!(
            model.poll(retry_at),
            Some(rpc::Request::Command(_))
        ));
    }

    #[test]
    fn a_timed_out_command_at_queue_capacity_is_not_lost() {
        let mut model = Model::new();
        for _ in 0..COMMAND_QUEUE_MAX {
            assert!(model.command(rpc::VOLUME_UP, 0));
        }

        let first_id = match model.poll(0).unwrap() {
            rpc::Request::Command(bytes) => bytes[3],
            _ => panic!("expected command"),
        };
        // The active command still occupies a queue slot, so later input cannot hide it.
        assert!(!model.command(rpc::VOLUME_DOWN, 0));
        assert!(model.poll(COMMAND_TIMEOUT_MS).is_none());

        let retry_at = COMMAND_TIMEOUT_MS + RETRY_MS * 2;
        let retry_id = match model.poll(retry_at).unwrap() {
            rpc::Request::Command(bytes) => bytes[3],
            _ => panic!("expected retried command"),
        };
        assert_ne!(retry_id, first_id);
        assert_eq!(
            model.reply(&reply(retry_id, 0, 0, 0, &[]), retry_at + 1, 100),
            Got::Changed
        );
        // Completing the head frees exactly one slot.
        assert!(model.command(rpc::VOLUME_DOWN, 0));
    }

    #[test]
    fn an_optimistic_command_marks_the_cache_dirty() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);
        model.cache_saved();
        assert!(model.command(rpc::VOLUME_UP, 0));
        assert!(model.cache_dirty());
        let mut blob = [0u8; BLOB_MAX];
        assert!(model.save(&mut blob) > 0);
        model.cache_saved();
        assert!(!model.cache_dirty());
    }

    #[test]
    fn snapshot_saves_when_reapplied_optimistic_state_changes() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);
        model.cache_saved();

        let id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        assert!(model.command(rpc::VOLUME_UP, 0));
        assert_eq!(model.volume(), 8);
        model.cache_saved();

        // The raw snapshot equals the old optimistic value, but replaying the queued command
        // changes the effective value that must be written to the cache.
        assert_eq!(
            model.reply(
                &reply(id, 0, 0, 1, &metadata_record(3, 8, 15)),
                POLL_MS + 11,
                100,
            ),
            Got::Save
        );
        assert_eq!(model.volume(), 9);
        assert!(model.cache_dirty());
    }

    #[test]
    fn malformed_command_reply_is_not_accepted_as_success() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);
        assert!(model.command(rpc::VOLUME_UP, 0));
        assert_eq!(model.volume(), 8);

        let id = match model.poll(11).unwrap() {
            rpc::Request::Command(bytes) => bytes[3],
            _ => panic!("expected command"),
        };
        // OK with a non-empty result shape is not a valid command acknowledgement.
        assert_eq!(
            model.reply(&reply(id, rpc::OK, 0, 1, &[]), 12, 100),
            Got::Save
        );
        assert_eq!(model.volume(), 7);
        assert_eq!(model.status, Status::Unavailable);
        assert!(model.cache_dirty());
    }

    #[test]
    fn denied_command_rolls_back_and_reapplies_later_optimistic_commands() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);
        model.cache_saved();
        assert!(model.command(rpc::PAUSE, 0));
        assert!(model.command(rpc::VOLUME_UP, 0));
        assert!(!model.playing());
        assert_eq!(model.volume(), 8);

        let id = match model.poll(11).unwrap() {
            rpc::Request::Command(bytes) => bytes[3],
            _ => panic!("expected command"),
        };
        assert_eq!(
            model.reply(&reply(id, rpc::DENIED, 0, 0, &[]), 12, 100),
            Got::Save
        );
        assert!(model.playing());
        assert_eq!(model.volume(), 8);
        assert!(model.cache_dirty());
        assert_eq!(model.status, Status::Denied);
    }

    #[test]
    fn snapshot_while_a_command_is_queued_refreshes_its_rollback_baseline() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100);

        let id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        assert!(model.command(rpc::VOLUME_UP, 0));
        assert_eq!(model.volume(), 8);
        model.reply(
            &reply(id, 0, 0, 1, &metadata_record(3, 6, 15)),
            POLL_MS + 11,
            100,
        );
        assert_eq!(model.volume(), 7);

        let id = match model.poll(POLL_MS + 12).unwrap() {
            rpc::Request::Command(bytes) => bytes[3],
            _ => panic!("expected command"),
        };
        assert_eq!(
            model.reply(&reply(id, rpc::DENIED, 0, 0, &[]), POLL_MS + 13, 100),
            Got::Save
        );
        assert_eq!(model.volume(), 6);
    }

    #[test]
    fn permission_and_unavailable_statuses_are_distinct() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, rpc::ASK, 0, 0, &[]), 10, 100);
        assert_eq!(model.status, Status::Ask);
        assert!(model.poll(ASK_AGAIN_MS).is_none());
        let id = match model.poll(ASK_AGAIN_MS + 11).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(&reply(id, rpc::DENIED, 0, 0, &[]), ASK_AGAIN_MS + 12, 100);
        assert_eq!(model.status, Status::Denied);
    }

    #[test]
    fn stale_reply_ids_are_ignored() {
        let mut model = Model::new();
        model.poll(0);
        assert_eq!(
            model.reply(&reply(99, 0, 0, 1, &metadata_record(3, 7, 15)), 10, 100),
            Got::Nothing
        );
        assert!(!model.has_snapshot);
    }

    #[test]
    fn track_text_and_artwork_are_committed_only_after_valid_replies() {
        let mut model = Model::new();
        let snapshot_id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(snapshot_id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            10,
            100,
        );

        // At the shared poll deadline the playback snapshot has priority, then track text and
        // artwork follow as separate bounded requests.
        let snapshot_id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(snapshot_id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            POLL_MS + 11,
            100,
        );

        let track_id = match model.poll(POLL_MS + 12).unwrap() {
            rpc::Request::Track(bytes) => bytes[3],
            _ => panic!("expected track request"),
        };
        model.reply(
            &reply(
                track_id,
                rpc::OK,
                0,
                2,
                &[
                    0, 13, b'M', b'i', b'd', b'n', b'i', b'g', b'h', b't', b' ', b'C', b'i', b't',
                    b'y',
                ],
            ),
            POLL_MS + 13,
            100,
        );
        model.reply(
            &reply(track_id, rpc::OK, 1, 2, &[1, 3, b'M', b'8', b'3']),
            POLL_MS + 14,
            100,
        );
        assert_eq!(model.track_title(), "Midnight City");
        assert_eq!(model.track_artist(), "M83");

        let (artwork_id, nonce) = match model.poll(POLL_MS + 15).unwrap() {
            rpc::Request::Artwork(bytes) => (
                bytes[3],
                u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]),
            ),
            _ => panic!("expected artwork request"),
        };
        let payload = [0_u8; artwork::PAYLOAD_LEN];
        let mut manifest = [0_u8; artwork::MANIFEST_LEN];
        manifest[0] = artwork::MANIFEST_VERSION;
        manifest[1] = artwork::ENCODING_RGB332_6BPP;
        manifest[2] = artwork::WIDTH;
        manifest[3] = artwork::HEIGHT;
        manifest[5..9].copy_from_slice(&nonce.to_le_bytes());
        manifest[9..17].copy_from_slice(&1_u64.to_le_bytes());
        manifest[17..21].copy_from_slice(&(artwork::PAYLOAD_LEN as u32).to_le_bytes());
        manifest[21..25].copy_from_slice(&artwork::crc32(&payload).to_le_bytes());
        model.reply(
            &reply(artwork_id, rpc::OK, 0, artwork::RECORD_COUNT, &manifest),
            POLL_MS + 16,
            100,
        );
        assert!(!model.artwork_visible());

        for index in 1..=artwork::CHUNK_COUNT {
            let offset = (index - 1) * artwork::CHUNK_LEN;
            let len = if index == artwork::CHUNK_COUNT {
                artwork::LAST_CHUNK_LEN
            } else {
                artwork::CHUNK_LEN
            };
            let mut chunk = Vec::with_capacity(6 + len);
            chunk.extend_from_slice(&nonce.to_le_bytes());
            chunk.extend_from_slice(&(offset as u16).to_le_bytes());
            chunk.extend_from_slice(&payload[offset..offset + len]);
            model.reply(
                &reply(
                    artwork_id,
                    rpc::OK,
                    index as u8,
                    artwork::RECORD_COUNT,
                    &chunk,
                ),
                POLL_MS + 16 + index as u32,
                100,
            );
        }
        assert!(model.artwork_visible());
        assert_eq!(model.artwork_generation(), 1);
        assert_eq!(model.status, Status::Ok);

        // The persistent cache remains the fixed playback-state record only.
        let mut blob = [0_u8; BLOB_MAX];
        let len = model.save(&mut blob);
        assert_eq!(len, BLOB_MAX);
        assert!(!blob.windows(13).any(|window| window == b"Midnight City"));
    }

    #[test]
    fn wait_never_busy_spins_or_delays_an_overdue_snapshot() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(id, rpc::ASK, 0, 0, &[]),
            10,
            100,
        );
        assert_eq!(model.wait(10), ASK_AGAIN_MS);

        model.reply(
            &reply(id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            11,
            100,
        );
        // The late OK reply is ignored because the ASK already terminated the request.
        assert_eq!(model.wait(11), ASK_AGAIN_MS - 1);

        let mut model = Model::new();
        model.next_poll = 100;
        assert_eq!(model.wait(500), 0);
    }

    #[test]
    fn wait_ignores_track_and_artwork_deadlines_until_media_is_active() {
        let mut model = Model::new();
        model.next_poll = 5_000;
        model.next_track = 10;
        model.next_artwork = 20;
        model.has_snapshot = true;
        model.status = Status::Ok;
        assert_eq!(model.wait(100), 4_900);

        model.metadata.flags = rpc::RESPONSE_ACTIVE | rpc::RESPONSE_PLAYING;
        assert_eq!(model.wait(100), 0);
        assert!(matches!(model.poll(100), Some(rpc::Request::Track(_))));
    }

    #[test]
    fn malformed_track_error_is_rejected_without_being_interpreted() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            10,
            100,
        );
        let id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            POLL_MS + 11,
            100,
        );
        let id = match model.poll(POLL_MS + 12).unwrap() {
            rpc::Request::Track(bytes) => bytes[3],
            _ => panic!("expected track request"),
        };
        model.reply(
            &reply(id, rpc::DENIED, 1, 2, b"x"),
            POLL_MS + 13,
            100,
        );
        assert_eq!(model.status, Status::Ok);
        assert_eq!(model.next_track, POLL_MS + 13 + TRACK_POLL_MS);
        // The artwork poll is already due while the media session is active, so the malformed
        // track reply must not introduce a busy wait.
        assert_eq!(model.wait(POLL_MS + 13), 0);
        assert!(matches!(
            model.poll(POLL_MS + 13),
            Some(rpc::Request::Artwork(_))
        ));
        assert_eq!(model.next_artwork, 2_010);
    }

    #[test]
    fn no_media_snapshot_clears_text_and_committed_artwork() {
        let mut model = Model::new();
        let id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            10,
            100,
        );
        model.set_track_text(b"Midnight City", b"M83");
        model.artwork_visible();

        let id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(id, rpc::OK, 0, 1, &metadata_record(0, 7, 15)),
            POLL_MS + 11,
            100,
        );
        assert!(!model.active());
        assert!(model.track_title().is_empty());
        assert!(model.track_artist().is_empty());
        assert!(!model.artwork_visible());
        assert_eq!(model.artwork_generation(), 0);
    }

    #[test]
    fn old_companion_fallback_keeps_controls_live_without_names_or_artwork() {
        let mut model = Model::new();
        let snapshot_id = match model.poll(0).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(snapshot_id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            10,
            100,
        );
        let snapshot_id = match model.poll(POLL_MS + 10).unwrap() {
            rpc::Request::Snapshot(bytes) => bytes[3],
            _ => panic!("expected snapshot"),
        };
        model.reply(
            &reply(snapshot_id, rpc::OK, 0, 1, &metadata_record(3, 7, 15)),
            POLL_MS + 11,
            100,
        );

        let track_id = match model.poll(POLL_MS + 12).unwrap() {
            rpc::Request::Track(bytes) => bytes[3],
            _ => panic!("expected track request"),
        };
        model.reply(
            &reply(track_id, rpc::BAD_REQUEST, 0, 0, &[]),
            POLL_MS + 13,
            100,
        );
        assert!(model.track_title().is_empty());
        assert!(model.track_artist().is_empty());

        let artwork_id = match model.poll(POLL_MS + 14).unwrap() {
            rpc::Request::Artwork(bytes) => bytes[3],
            _ => panic!("expected artwork request"),
        };
        model.reply(
            &reply(artwork_id, rpc::BAD_REQUEST, 0, 0, &[]),
            POLL_MS + 15,
            100,
        );
        assert!(!model.artwork_visible());
        assert_eq!(model.status, Status::Ok);
        assert!(model.controls_enabled());
    }

    #[test]
    fn wall_clock_uses_local_calendar_time() {
        assert_eq!(wall_min(1970, 1, 1, 0, 0), 0);
        assert_eq!(wall_min(1970, 1, 2, 0, 0), 1440);
        assert_eq!(
            wall_min(2026, 11, 19, 12, 30),
            days(2026, 11, 19) * 1440 + 750
        );
    }
}

//! Bounded track-title and artist records for the watch display.
//!
//! Text is intentionally restricted to printable ASCII and 60 bytes per field. The companion
//! performs the same restriction before transmission, and the watch validates it again because
//! every BLE message is untrusted.

pub const TRACK_TOTAL: u8 = 2;
pub const TRACK_TEXT_MAX: usize = 60;

pub const TITLE_KIND: u8 = 0;
pub const ARTIST_KIND: u8 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Field {
    Title,
    Artist,
}

/// Decodes one title/artist record: `kind || length || printable ASCII bytes`.
pub fn decode(record: &[u8]) -> Option<(Field, &[u8])> {
    if record.len() < 2 || record.len() > TRACK_TEXT_MAX + 2 {
        return None;
    }
    let field = match record[0] {
        TITLE_KIND => Field::Title,
        ARTIST_KIND => Field::Artist,
        _ => return None,
    };
    let len = record[1] as usize;
    if len > TRACK_TEXT_MAX || record.len() != len + 2 {
        return None;
    }
    if record[2..].iter().any(|byte| !(0x20..=0x7e).contains(byte)) {
        return None;
    }
    Some((field, &record[2..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_artist_records_decode() {
        assert_eq!(
            decode(&[TITLE_KIND, 4, b'N', b'i', b't', b'e']),
            Some((Field::Title, &b"Nite"[..]))
        );
        assert_eq!(
            decode(&[ARTIST_KIND, 3, b'M', b'8', b'3']),
            Some((Field::Artist, &b"M83"[..]))
        );
    }

    #[test]
    fn malformed_text_records_are_rejected() {
        assert!(decode(&[]).is_none());
        assert!(decode(&[TITLE_KIND]).is_none());
        assert!(decode(&[2, 0]).is_none());
        assert!(decode(&[TITLE_KIND, 2, b'A']).is_none());
        assert!(decode(&[TITLE_KIND, 1, 0x1f]).is_none());
        let mut too_long = vec![TITLE_KIND, 0x3f];
        too_long.extend_from_slice(&[b'A'; 60]);
        assert!(decode(&too_long).is_none());
    }
}

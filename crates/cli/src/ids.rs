//! The wall clock and the session ids the binary hands the command line, which a test replaces
//! with its own.

use std::fmt::Write as _;
use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use farik_protocol::clock::{Clock, IdSource};

/// The wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// How many session ids this process has handed out, so that no two hash the same input.
static HANDED_OUT: AtomicU64 = AtomicU64::new(0);

/// Session ids as version 4 UUIDs, which is what Claude Code's `--session-id` takes: 128 bits from
/// two of the standard library's randomly keyed hashers over a process-wide counter, with the
/// version and variant bits set. Unique, not secret: the daemon's token is the secret.
pub struct RandomSessionIds;

impl IdSource for RandomSessionIds {
    fn session_id(&self) -> String {
        let count = HANDED_OUT.fetch_add(1, Ordering::Relaxed);
        let high = RandomState::new().hash_one(count);
        let low = RandomState::new().hash_one(count);
        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&high.to_be_bytes());
        bytes[8..].copy_from_slice(&low.to_be_bytes());
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex = bytes
            .iter()
            .fold(String::with_capacity(32), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use farik_protocol::clock::IdSource;

    use super::RandomSessionIds;

    #[test]
    fn hands_out_distinct_version_4_uuids() {
        let ids: Vec<String> = (0..1000).map(|_| RandomSessionIds.session_id()).collect();
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 1000);
        for id in &ids {
            assert_eq!(id.len(), 36, "{id}");
            assert_eq!(id.as_bytes()[14], b'4', "{id}");
            assert!(
                matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'),
                "{id}"
            );
        }
    }
}

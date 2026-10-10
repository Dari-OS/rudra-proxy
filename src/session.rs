use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// OpenCode session identifier.
///
/// Format required by the OpenCode Zen gateway:
/// `ses_` + 12 lowercase hexadecimal characters + 14 alphanumeric characters (30 chars total).
/// E.g. `ses_01944747eb48OeFGXjAMxiyjz3`
///
/// The 12 hex digits encode the current Unix millisecond timestamp and a monotonic counter:
/// `(BigInt(Date.now()) * 0x1000n + BigInt(counter))` in 6 big-endian hex bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a fresh, valid OpenCode session ID with legitimate millisecond timestamp encoding.
    pub fn generate() -> Self {
        const ALNUM_CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let counter = COUNTER.fetch_add(1, Ordering::Relaxed) & 0xfff;
        let current = (now_ms as u128) * 0x1000 + (counter as u128);

        let mut time_hex = String::with_capacity(12);
        for index in 0..6 {
            let shift = 40 - 8 * index;
            let byte = ((current >> shift) & 0xff) as u8;
            time_hex.push_str(&format!("{:02x}", byte));
        }

        let mut rng = rand::thread_rng();
        let mut random_suffix = String::with_capacity(14);
        for _ in 0..14 {
            let idx = rng.gen_range(0..ALNUM_CHARS.len());
            random_suffix.push(ALNUM_CHARS[idx] as char);
        }

        Self(format!("ses_{}{}", time_hex, random_suffix))
    }

    /// Creates a `SessionId` from a string without validation.
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Validates and parses an OpenCode session ID.
    pub fn parse(s: &str) -> Result<Self, SessionIdError> {
        if s.len() != 30 {
            return Err(SessionIdError::InvalidLength(s.len()));
        }
        if !s.starts_with("ses_") {
            return Err(SessionIdError::InvalidPrefix);
        }
        let hex_part = &s[4..16];
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
            return Err(SessionIdError::InvalidHexPart);
        }
        let alnum_part = &s[16..30];
        if !alnum_part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(SessionIdError::InvalidAlnumPart);
        }
        Ok(Self(s.to_string()))
    }

    /// Access the underlying string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::generate()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = SessionIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages generation and rotation of session IDs across requests.
#[derive(Clone)]
pub struct SessionManager {
    rotate_after_requests: u64,
    counter: Arc<AtomicU64>,
    current_session: Arc<RwLock<SessionId>>,
}

impl SessionManager {
    pub fn new(rotate_after_requests: u64) -> Self {
        Self {
            rotate_after_requests,
            counter: Arc::new(AtomicU64::new(0)),
            current_session: Arc::new(RwLock::new(SessionId::generate())),
        }
    }

    /// Resolves the session ID for a request.
    /// If the client supplied a valid session header, honors it.
    /// Otherwise uses the managed session ID, rotating every `rotate_after_requests` requests.
    pub async fn get_or_rotate_session(&self, client_header: Option<&str>) -> SessionId {
        if let Some(header_val) = client_header
            && let Ok(parsed) = SessionId::parse(header_val) {
                return parsed;
            }

        // If rotate_after_requests <= 1, generate a fresh ID on every request
        if self.rotate_after_requests <= 1 {
            return SessionId::generate();
        }

        let count = self.counter.fetch_add(1, Ordering::SeqCst);
        if count >= self.rotate_after_requests {
            let mut lock = self.current_session.write().await;
            if self.counter.load(Ordering::SeqCst) >= self.rotate_after_requests {
                self.counter.store(1, Ordering::SeqCst);
                let new_session = SessionId::generate();
                *lock = new_session.clone();
                tracing::info!(
                    new_session = %new_session,
                    after_requests = self.rotate_after_requests,
                    "Rotated OpenCode session ID"
                );
                new_session
            } else {
                lock.clone()
            }
        } else {
            let lock = self.current_session.read().await;
            lock.clone()
        }
    }

    pub fn rotate_after_requests(&self) -> u64 {
        self.rotate_after_requests
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionIdError {
    #[error("Session ID must be exactly 30 characters, got {0}")]
    InvalidLength(usize),
    #[error("Session ID must start with 'ses_'")]
    InvalidPrefix,
    #[error("Characters 4..16 must be lowercase hex digits")]
    InvalidHexPart,
    #[error("Characters 16..30 must be alphanumeric")]
    InvalidAlnumPart,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_parse() {
        for _ in 0..50 {
            let session_id = SessionId::generate();
            assert_eq!(session_id.as_str().len(), 30);
            assert!(session_id.as_str().starts_with("ses_"));
            assert!(SessionId::parse(session_id.as_str()).is_ok());
        }
    }

    #[test]
    fn test_known_session_id() {
        let valid = "ses_ee2a1858bea8OeFGXjAMxiyjz3";
        let parsed = SessionId::parse(valid).expect("should parse");
        assert_eq!(parsed.as_str(), valid);
    }

    #[test]
    fn test_timestamp_hex_encoding() {
        let before_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let session_id = SessionId::generate();
        let hex_part = &session_id.as_str()[4..16];
        assert_eq!(hex_part.len(), 12);
        // Ensure hex part can be decoded as a 48-bit number
        let val = u64::from_str_radix(hex_part, 16).expect("must be valid hex");
        // Lower 48 bits of (timestamp * 4096)
        let expected_min = ((before_ms as u128 * 0x1000) & 0xffffffffffff) as u64;
        // The value should be very close to expected_min (within a few seconds of counter/time)
        assert!(val >= expected_min.saturating_sub(0x1000 * 5000));
    }
}

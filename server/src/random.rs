//! The random source is injected into the app state so behaviour that depends
//! on randomness (the Path Template's `{random:N}`) is deterministic in tests.
//!
//! Secrets (session tokens, API Token secrets, password salts) do NOT use this
//! source: they always come from the OS CSPRNG, so a test double can never
//! weaken them.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use rand::{RngCore, SeedableRng, rngs::StdRng};

pub trait RandomSource: Send + Sync + 'static {
    fn fill_bytes(&self, buf: &mut [u8]);

    fn next_u64(&self) -> u64 {
        let mut buf = [0u8; 8];
        self.fill_bytes(&mut buf);
        u64::from_le_bytes(buf)
    }
}

/// Shared handle to the random source, as stored in the app state.
pub type SharedRandom = Arc<dyn RandomSource>;

/// The OS random generator; used in production.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsRandom;

impl RandomSource for OsRandom {
    fn fill_bytes(&self, buf: &mut [u8]) {
        rand::rngs::OsRng.fill_bytes(buf);
    }
}

/// A reproducible generator for tests.
#[derive(Debug)]
pub struct SeededRandom {
    rng: Mutex<StdRng>,
}

impl SeededRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Mutex::new(StdRng::seed_from_u64(seed)),
        }
    }
}

impl RandomSource for SeededRandom {
    fn fill_bytes(&self, buf: &mut [u8]) {
        self.rng.lock().unwrap().fill_bytes(buf);
    }
}

/// A test double that first hands out scripted bytes, then falls back to a
/// seeded generator. Cloning yields a handle to the same script, so a test can
/// keep one handle and give another to the app, e.g. to force the Path
/// Template to draw a colliding or reserved `{random:N}`.
#[derive(Debug, Clone)]
pub struct ScriptedRandom {
    script: Arc<Mutex<VecDeque<u8>>>,
    fallback: Arc<SeededRandom>,
}

impl ScriptedRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            script: Arc::default(),
            fallback: Arc::new(SeededRandom::new(seed)),
        }
    }

    /// Queues raw bytes to be returned next.
    pub fn push_bytes(&self, bytes: &[u8]) {
        self.script.lock().unwrap().extend(bytes);
    }

    /// Queues the bytes that make `{random:N}` produce exactly `text`
    /// (which must consist of base62 characters).
    pub fn push_base62(&self, text: &str) {
        let bytes: Vec<u8> = text
            .bytes()
            .map(|c| {
                let i = crate::path_template::BASE62
                    .iter()
                    .position(|&b| b == c)
                    .unwrap_or_else(|| panic!("{:?} is not a base62 character", c as char));
                i as u8
            })
            .collect();
        self.push_bytes(&bytes);
    }
}

impl RandomSource for ScriptedRandom {
    fn fill_bytes(&self, buf: &mut [u8]) {
        let mut script = self.script.lock().unwrap();
        for b in buf.iter_mut() {
            *b = match script.pop_front() {
                Some(v) => v,
                None => {
                    let mut one = [0u8; 1];
                    self.fallback.fill_bytes(&mut one);
                    one[0]
                }
            };
        }
    }
}

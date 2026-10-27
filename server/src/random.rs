//! The random source is injected into the app state so behaviour that depends
//! on randomness (e.g. Path Template `{random:N}`, added in a later slice) is
//! deterministic in tests.
//!
//! Secrets (session tokens, API Token secrets, password salts) do NOT use this
//! source: they always come from the OS CSPRNG, so a test double can never
//! weaken them.

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

//! The hasher of the maps keyed by node addresses and byte offsets: the
//! keys come from the engine, not from the input, so the default hasher's
//! protection against chosen keys buys nothing there.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// One multiply per key; `finish` rotates the well-mixed high bits down
/// to the bits the table indexes by (addresses have zero low bits).
#[derive(Default, Clone, Copy)]
pub struct AddrHasher(u64);

const K: u64 = 0x9E37_79B9_7F4A_7C15;

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(K);
        }
    }

    fn write_usize(&mut self, n: usize) {
        self.0 = (self.0 ^ n as u64).wrapping_mul(K);
    }
}

pub type AddrMap<K, V> = HashMap<K, V, BuildHasherDefault<AddrHasher>>;
pub type AddrSet<K> = HashSet<K, BuildHasherDefault<AddrHasher>>;

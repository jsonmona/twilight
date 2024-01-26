use rand::{CryptoRng, RngCore};
use serde::Serialize;
use std::fmt::{Debug, Write};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId([u64; 4]);

impl SessionId {
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(4 * 16);
        for val in &self.0 {
            write!(s, "{val:016x}").expect("writing to string does not fail");
        }
        s
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 4 * 16 {
            return None;
        }

        let s = s.as_bytes();
        let (a, s) = s.split_at(16);
        let (b, s) = s.split_at(16);
        let (c, d) = s.split_at(16);

        // No need to check lengths because s had length of 4 * 16

        let a = u64::from_str_radix(std::str::from_utf8(a).ok()?, 16).ok()?;
        let b = u64::from_str_radix(std::str::from_utf8(b).ok()?, 16).ok()?;
        let c = u64::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok()?;
        let d = u64::from_str_radix(std::str::from_utf8(d).ok()?, 16).ok()?;

        Some(SessionId([a, b, c, d]))
    }

    pub fn from_random(rng: &mut (impl RngCore + CryptoRng)) -> Self {
        let mut arr = [0; 4];

        let data: &mut [u8] = bytemuck::cast_slice_mut(&mut arr);
        rng.fill_bytes(data);

        SessionId(arr)
    }
}

impl Debug for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SessionId").field(&self.to_hex()).finish()
    }
}

impl Serialize for SessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

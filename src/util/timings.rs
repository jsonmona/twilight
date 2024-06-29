use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::Micros;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    #[serde(skip)]
    pub capture: SidedTime,

    pub encode_begin: Micros,
    pub encode_end: Micros,
    pub network_send: Micros,

    #[serde(skip)]
    pub network_recv: SidedTime,

    pub decode_begin: Micros,
    pub decode_end: Micros,
    pub present: Micros,
}

impl Timings {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn elapsed_since_capture(&self) -> Option<Micros> {
        let elapsed = self.capture.as_local()?.elapsed();
        Some(Micros::from_duration_saturating(elapsed))
    }

    pub fn elapsed_since_recv(&self) -> Option<Micros> {
        let elapsed = self.network_recv.as_local()?.elapsed();
        Some(Micros::from_duration_saturating(elapsed))
    }
}

/// A time (`std::time::Instant`) that becomes `Self::Remote` when serialized.
/// Thus, a time that is "sided".
#[derive(Debug, Clone)]
pub enum SidedTime {
    Local(Instant),
    Remote,
}

impl From<Instant> for SidedTime {
    fn from(value: Instant) -> Self {
        Self::Local(value)
    }
}

impl Default for SidedTime {
    fn default() -> Self {
        SidedTime::Remote
    }
}

impl SidedTime {
    pub fn as_local(&self) -> Option<&Instant> {
        match self {
            Self::Local(x) => Some(x),
            Self::Remote => None,
        }
    }
}

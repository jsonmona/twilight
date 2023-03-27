use std::num::TryFromIntError;
use std::ops::{Add, Sub};
use std::time::Duration;
use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Zeroable, Pod)]
#[repr(transparent)]
pub struct Micros(u32);

impl Micros {
    pub const MIN: Micros = Micros(u32::MIN);
    pub const MAX: Micros = Micros(u32::MAX);

    pub fn from_seconds(s: u32) -> Self {
        Micros(s * 1_000_000)
    }

    pub fn from_millis(m: u32) -> Self {
        Micros(m * 1_000)
    }

    pub fn from_micros(m: u32) -> Self {
        Micros(m)
    }

    pub fn from_duration_saturating(dur: Duration) -> Self {
        dur.try_into().unwrap_or(Self::MAX)
    }

    pub fn as_micros(self) -> u32 {
        self.0
    }

    pub fn as_millis(self) -> u32 {
        self.0 / 1_000
    }

    pub fn as_secs(self) -> u32 {
        self.0 / 1_000_000
    }

    pub fn as_secs_f32(self) -> f32 {
        self.0 as f32 / 1_000_000.0
    }

    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    pub fn min(self, rhs: Self) -> Self {
        Micros(u32::min(self.0, rhs.0))
    }

    pub fn max(self, rhs: Self) -> Self {
        Micros(u32::max(self.0, rhs.0))
    }
}

impl<'a, 'b> Add<&'b Micros> for &'a Micros {
    type Output = Micros;

    fn add(self, rhs: &'b Micros) -> Micros {
        Micros(self.0 + rhs.0)
    }
}

impl<'a, 'b> Sub<&'b Micros> for &'a Micros {
    type Output = Micros;

    fn sub(self, rhs: &'b Micros) -> Micros {
        Micros(self.0 - rhs.0)
    }
}

impl TryFrom<Duration> for Micros {
    type Error = TryFromIntError;

    fn try_from(value: Duration) -> Result<Self, Self::Error> {
        value.as_micros().try_into().map(Micros)
    }
}

impl From<Micros> for Duration {
    fn from(value: Micros) -> Self {
        Duration::from_micros(value.0.into())
    }
}

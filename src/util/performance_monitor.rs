use crate::util::micros::Micros;
use std::fmt::{Debug, Formatter};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct PerformanceStats {
    pub min: Micros,
    pub max: Micros,
    pub avg: Micros,
}

pub struct PerformanceMonitor {
    last_measure: Instant,
    valid_samples: usize,
    sample_pos: usize,
    samples: Box<[Micros; 128]>,
}

impl Debug for PerformanceMonitor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerformanceMonitor")
            .field("last_measure", &self.last_measure)
            .field("valid_samples", &self.valid_samples)
            .field("sample_pos", &self.sample_pos)
            .finish_non_exhaustive()
    }
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dur = Micros::from_duration_saturating(now - self.last_measure);
        let curr_pos = self.sample_pos;
        self.last_measure = now;
        self.valid_samples = usize::min(self.samples.len(), self.valid_samples + 1);
        self.sample_pos = (curr_pos + 1) % self.samples.len();
        self.samples[curr_pos] = dur;
    }

    pub fn start_zone(&mut self) -> PerformanceMonitorZone {
        self.last_measure = Instant::now();
        PerformanceMonitorZone(self)
    }

    pub fn reset(&mut self) {
        self.last_measure = Instant::now();
        self.sample_pos = 0;
        self.valid_samples = 0;
    }

    pub fn get(&self) -> Option<PerformanceStats> {
        if self.valid_samples < self.samples.len() {
            return None;
        }

        let mut min = self.samples[0];
        let mut max = self.samples[0];
        let mut accum = self.samples[0].as_micros() as u64;
        for sample in self.samples.iter().skip(1) {
            min = Micros::min(min, *sample);
            max = Micros::max(max, *sample);
            accum += sample.as_micros() as u64;
        }
        accum /= self.samples.len() as u64;
        let avg = Micros::from_micros(accum as u32);

        Some(PerformanceStats { min, max, avg })
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        PerformanceMonitor {
            last_measure: Instant::now(),
            valid_samples: 0,
            sample_pos: 0,
            samples: bytemuck::allocation::zeroed_box(),
        }
    }
}

#[must_use]
#[derive(Debug)]
pub struct PerformanceMonitorZone<'a>(&'a mut PerformanceMonitor);

impl PerformanceMonitorZone<'_> {
    pub fn end_zone(self) {
        // just drop
    }
}

impl Drop for PerformanceMonitorZone<'_> {
    fn drop(&mut self) {
        self.0.update();
    }
}

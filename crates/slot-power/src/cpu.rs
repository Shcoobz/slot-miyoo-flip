//! The CPU's top clock, capped while a light core plays. The governor still picks the clock
//! under the cap; only `scaling_max_freq` is written.

use std::fs;
use std::path::{Path, PathBuf};

/// The step to write for a cap of `khz`: the highest the CPU offers at or under it, the lowest
/// it offers when every step is over, or the cap itself when the steps are not listed.
pub fn cpu_step(steps: &[u32], khz: u32) -> u32 {
    steps
        .iter()
        .copied()
        .filter(|s| *s <= khz)
        .max()
        .or_else(|| steps.iter().copied().min())
        .unwrap_or(khz)
}

pub struct CpuCap {
    max_freq: PathBuf,
    steps: Vec<u32>,
    written: Option<u32>,
}

impl CpuCap {
    /// The cap of the cpufreq policy at `policy`, if it has a top clock to set.
    pub fn at(policy: &Path) -> Option<CpuCap> {
        let max_freq = policy.join("scaling_max_freq");
        if !max_freq.is_file() {
            return None;
        }
        let steps = fs::read_to_string(policy.join("scaling_available_frequencies"))
            .map(|s| {
                s.split_whitespace()
                    .filter_map(|f| f.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        Some(CpuCap {
            max_freq,
            steps,
            written: None,
        })
    }

    /// Caps the top clock at `khz`, writing only when the step changes.
    pub fn cap(&mut self, khz: u32) {
        let step = cpu_step(&self.steps, khz);
        if self.written == Some(step) {
            return;
        }
        match fs::write(&self.max_freq, step.to_string()) {
            Ok(()) => {
                eprintln!("slot: cpu: top clock {step} kHz");
                self.written = Some(step);
            }
            Err(e) => eprintln!("slot: cpu: {}: {e}", self.max_freq.display()),
        }
    }
}

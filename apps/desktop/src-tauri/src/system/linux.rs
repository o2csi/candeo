//! The Linux reader: `/proc` and `hwmon`, readable without root. The CPU's
//! power is not: RAPL's `energy_uj` is root only.

use std::fs;
use std::path::{Path, PathBuf};

use super::parse;
use super::{Read, Reading};

pub struct Reader {
    /// The previous `/proc/stat` times, for the load between two reads.
    times: Option<(u64, u64)>,
    /// The CPU's temperature file: `coretemp`'s package or `k10temp`'s `Tctl`.
    cpu_temp: Option<PathBuf>,
    /// An `amdgpu` card's `hwmon` directory and its `gpu_busy_percent`.
    gpu_temp: Option<PathBuf>,
    gpu_busy: Option<PathBuf>,
}

impl Reader {
    pub fn open() -> Reader {
        let mut reader = Reader {
            times: None,
            cpu_temp: None,
            gpu_temp: None,
            gpu_busy: None,
        };
        for dir in fs::read_dir("/sys/class/hwmon")
            .into_iter()
            .flatten()
            .flatten()
        {
            let dir = dir.path();
            let name = fs::read_to_string(dir.join("name")).unwrap_or_default();
            match name.trim() {
                "coretemp" | "k10temp" | "zenpower" => {
                    reader.cpu_temp.get_or_insert(dir.join("temp1_input"));
                }
                "amdgpu" => {
                    reader.gpu_temp.get_or_insert(dir.join("temp1_input"));
                    let busy = dir.join("device/gpu_busy_percent");
                    if busy.exists() {
                        reader.gpu_busy.get_or_insert(busy);
                    }
                }
                _ => {}
            }
        }
        reader
    }
}

fn text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

impl Read for Reader {
    fn read(&mut self) -> Reading {
        let now = text(Path::new("/proc/stat")).and_then(|s| parse::cpu_times(&s));
        let cpu = match (self.times, now) {
            (Some(before), Some(after)) => parse::load(before, after),
            _ => None,
        };
        self.times = now;
        Reading {
            cpu,
            memory: text(Path::new("/proc/meminfo")).and_then(|m| parse::memory_used(&m)),
            gpu: self
                .gpu_busy
                .as_deref()
                .and_then(text)
                .and_then(|t| t.trim().parse::<f32>().ok())
                .map(|p| (p / 100.0).clamp(0.0, 1.0)),
            cpu_celsius: self
                .cpu_temp
                .as_deref()
                .and_then(text)
                .and_then(|t| parse::millidegrees(&t)),
            gpu_celsius: self
                .gpu_temp
                .as_deref()
                .and_then(text)
                .and_then(|t| parse::millidegrees(&t)),
            cpu_watts: None,
        }
    }
}

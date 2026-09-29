//! What the readers make of the text and names the system hands them. Pure, and
//! compiled everywhere, so the Linux files and the Windows counter names are
//! tested on either system. Each reader uses its own system's half.

use std::collections::HashMap;

/// The CPU's time spent, and how much of it idle, from `/proc/stat`'s `cpu`
/// line: `user nice system idle iowait irq softirq steal`, in ticks.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn cpu_times(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let ticks: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|t| t.parse().ok())
        .collect::<Option<_>>()?;
    if ticks.len() < 5 {
        return None;
    }
    let idle = ticks[3] + ticks[4];
    Some((ticks.iter().sum(), idle))
}

/// The load between two readings of [`cpu_times`], in 0..1.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn load(before: (u64, u64), after: (u64, u64)) -> Option<f32> {
    let total = after.0.checked_sub(before.0)?;
    let idle = after.1.checked_sub(before.1)?;
    (total > 0).then(|| 1.0 - idle.min(total) as f32 / total as f32)
}

/// The share of memory in use, from `/proc/meminfo`: what is not available.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn memory_used(meminfo: &str) -> Option<f32> {
    let field = |name: &str| -> Option<f64> {
        meminfo
            .lines()
            .find(|l| l.starts_with(name))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    (total > 0.0).then(|| (1.0 - available / total).clamp(0.0, 1.0) as f32)
}

/// The GPU's load as Task Manager shows it, from the `GPU Engine` counter's
/// instances, `pid_1234_luid_0x0_0xD1A5_phys_0_eng_3_engtype_3D`, each a
/// percentage: summed per engine across processes, the busiest engine's share.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn busiest_engine<'a>(items: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f32> {
    let mut engines: HashMap<&str, f64> = HashMap::new();
    for (name, percent) in items {
        let engine = name.find("luid_").map_or(name, |at| &name[at..]);
        *engines.entry(engine).or_default() += percent;
    }
    engines
        .values()
        .copied()
        .fold(None, |most: Option<f64>, v| {
            Some(most.map_or(v, |m| m.max(v)))
        })
        .map(|p| (p / 100.0).clamp(0.0, 1.0) as f32)
}

/// The CPU's package power in watts, from the `Energy Meter` counter's
/// instances in milliwatts: the packages (`RAPL_Package0_PKG`), summed.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn package_watts<'a>(items: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f32> {
    let packages: Vec<f64> = items
        .into_iter()
        .filter(|(name, _)| name.to_ascii_uppercase().ends_with("_PKG"))
        .map(|(_, mw)| mw)
        .collect();
    (!packages.is_empty()).then(|| (packages.iter().sum::<f64>() / 1000.0) as f32)
}

/// The hottest ACPI thermal zone, from `High Precision Temperature` in tenths of
/// a kelvin, in degrees Celsius. A zone reading nothing sensible is left out.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn hottest_zone(tenths_of_kelvin: impl IntoIterator<Item = f64>) -> Option<f32> {
    tenths_of_kelvin
        .into_iter()
        .map(|t| t / 10.0 - 273.15)
        .filter(|c| (1.0..150.0).contains(c))
        .fold(None, |most: Option<f64>, c| {
            Some(most.map_or(c, |m| m.max(c)))
        })
        .map(|c| c as f32)
}

/// A `hwmon` temperature file, in thousandths of a degree.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn millidegrees(text: &str) -> Option<f32> {
    let c = text.trim().parse::<f64>().ok()? / 1000.0;
    (1.0..150.0).contains(&c).then_some(c as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cpu_load_is_what_was_not_idle() {
        let before = cpu_times("cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3").unwrap();
        let after = cpu_times("cpu  200 0 200 1300 100 0 0 0 0 0").unwrap();
        assert_eq!(before, (1000, 800));
        assert_eq!(load(before, after), Some(0.25));
        assert_eq!(load(after, after), None, "no time passed");
        assert_eq!(cpu_times("intr 1 2 3"), None);
    }

    #[test]
    fn memory_in_use_is_what_is_not_available() {
        let meminfo = "MemTotal:       16000000 kB\nMemFree:  1 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(memory_used(meminfo), Some(0.75));
        assert_eq!(memory_used("MemTotal: 1 kB"), None);
    }

    /// Two processes on one engine add up; the busiest engine is the load.
    #[test]
    fn the_gpu_load_is_its_busiest_engine() {
        let items = [
            ("pid_10_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 30.0),
            ("pid_20_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 25.0),
            ("pid_20_luid_0x0_0xA_phys_0_eng_3_engtype_VideoDecode", 40.0),
            ("pid_30_luid_0x0_0xB_phys_0_eng_0_engtype_3D", 5.0),
        ];
        assert_eq!(busiest_engine(items), Some(0.55));
        assert_eq!(busiest_engine([]), None);
    }

    #[test]
    fn the_package_power_is_in_watts() {
        let items = [
            ("RAPL_Package0_PKG", 60_066.8),
            ("RAPL_Package0_PP0", 52_346.6),
            ("_Total", 0.0),
        ];
        assert_eq!(package_watts(items), Some(60.0668));
        assert_eq!(package_watts([("RAPL_Package0_DRAM", 1.0)]), None);
    }

    #[test]
    fn the_hottest_zone_is_in_celsius() {
        assert_eq!(hottest_zone([3231.5, 3331.5]), Some(60.0));
        assert_eq!(hottest_zone([0.0]), None, "a zone reading nothing");
        assert_eq!(millidegrees("54000\n"), Some(54.0));
        assert_eq!(millidegrees("-1"), None);
    }
}

//! What the benchmark ran on, and what the process used: CPU time, peak memory, threads.

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

/// The machine and build a report came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    pub cpu: String,
    /// Cores the process can run on.
    pub cores: usize,
    pub memory_gb: Option<f64>,
    pub os: String,
    pub rustc: String,
    /// The Cargo profile, its optimization level, and whether debug assertions are on.
    pub profile: String,
    pub target: String,
}

/// The first line a command prints, trimmed (none when it can't be run or fails).
fn said(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// The value after `key:` on the first line of `text` that starts with it.
fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| value.trim().to_string())
    })
}

impl Machine {
    pub fn here() -> Self {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let cpu = said("sysctl", &["-n", "machdep.cpu.brand_string"])
            .or_else(|| field(&cpuinfo, "model name"))
            .unwrap_or_else(|| "unknown".to_string());
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let memory_bytes = said("sysctl", &["-n", "hw.memsize"])
            .and_then(|bytes| bytes.parse::<f64>().ok())
            .or_else(|| {
                let kb = field(&meminfo, "MemTotal")?;
                Some(kb.split_whitespace().next()?.parse::<f64>().ok()? * 1024.0)
            });
        let os = match (
            said("sw_vers", &["-productName"]),
            said("sw_vers", &["-productVersion"]),
        ) {
            (Some(name), Some(version)) => format!("{name} {version}"),
            _ => said("uname", &["-sr"]).unwrap_or_else(|| std::env::consts::OS.to_string()),
        };
        Self {
            cpu,
            cores: std::thread::available_parallelism().map_or(1, usize::from),
            memory_gb: memory_bytes.map(|b| (b / f64::from(1u32 << 30) * 10.0).round() / 10.0),
            os,
            rustc: env!("PF_BENCH_RUSTC").to_string(),
            profile: format!(
                "{} (opt-level {}{})",
                env!("PF_BENCH_PROFILE"),
                env!("PF_BENCH_OPT_LEVEL"),
                if cfg!(debug_assertions) {
                    ", debug assertions"
                } else {
                    ""
                }
            ),
            target: env!("PF_BENCH_TARGET").to_string(),
        }
    }
}

/// Whether this is an optimized build (timings from any other say nothing about the product).
pub fn optimized() -> bool {
    !cfg!(debug_assertions)
}

/// What the process has used so far.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Used {
    /// CPU time on every thread, in user code and in the kernel.
    pub user: Duration,
    pub system: Duration,
    /// The most memory the process has held at once, in bytes.
    pub peak_rss: u64,
}

impl Used {
    pub fn cpu(&self) -> Duration {
        self.user + self.system
    }
}

/// What the process has used so far (`None` where that can't be asked).
#[cfg(unix)]
pub fn used() -> Option<Used> {
    use nix::sys::resource::{UsageWho, getrusage};
    let usage = getrusage(UsageWho::RUSAGE_SELF).ok()?;
    let time = |t: nix::sys::time::TimeVal| {
        Duration::from_secs(t.tv_sec().max(0) as u64) + Duration::from_micros(t.tv_usec().max(0) as u64)
    };
    // The peak is in bytes on macOS and in kilobytes everywhere else.
    let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
    Some(Used {
        user: time(usage.user_time()),
        system: time(usage.system_time()),
        peak_rss: usage.max_rss().max(0) as u64 * unit,
    })
}

#[cfg(not(unix))]
pub fn used() -> Option<Used> {
    None
}

/// How many threads the process has right now, as the system counts them.
pub fn threads() -> Option<usize> {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        return field(&status, "Threads")?.parse().ok();
    }
    // macOS: one line per thread after the header.
    let pid = std::process::id().to_string();
    let out = Command::new("ps").args(["-M", "-p", &pid]).output().ok()?;
    let lines = String::from_utf8_lossy(&out.stdout).lines().count();
    (out.status.success() && lines > 1).then(|| lines - 1)
}

/// The system's one-minute load average: how busy the machine was with other things.
pub fn load_average() -> Option<f64> {
    let text = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .or_else(|| said("sysctl", &["-n", "vm.loadavg"]))?;
    text.split_whitespace().find_map(|word| word.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_machine_is_described_without_failing_anywhere() {
        let machine = Machine::here();
        assert!(machine.cores >= 1);
        assert!(!machine.cpu.is_empty() && !machine.os.is_empty());
        assert!(machine.rustc.starts_with("rustc"), "{}", machine.rustc);
    }

    #[cfg(unix)]
    #[test]
    fn usage_only_grows() {
        let before = used().expect("usage on unix");
        let mut held = vec![0u8; 8 << 20];
        // Touch every page so the memory is really held.
        for byte in held.iter_mut().step_by(1024) {
            *byte = 1;
        }
        let after = used().expect("usage on unix");
        assert!(after.cpu() >= before.cpu());
        assert!(after.peak_rss >= before.peak_rss);
        assert!(after.peak_rss > 8 << 20, "{}", after.peak_rss);
        assert_eq!(
            held.iter().step_by(1024).map(|&b| u64::from(b)).sum::<u64>(),
            8 << 10
        );
    }

    #[test]
    fn fields_are_read_by_name() {
        let text = "Name:\tpf-bench\nThreads:\t7\nmodel name\t: Some CPU @ 3GHz\n";
        assert_eq!(field(text, "Threads").as_deref(), Some("7"));
        assert_eq!(field(text, "model name").as_deref(), Some("Some CPU @ 3GHz"));
        assert_eq!(field(text, "missing"), None);
    }
}

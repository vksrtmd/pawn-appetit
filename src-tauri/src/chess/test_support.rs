//! Fake engines for tests: shell scripts that speak just enough UCI.

use std::path::{Path, PathBuf};

/// Write an executable shell script `name` with `body` into `dir`.
pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// An engine that appends its pid to `pids` next to it, waits `delay` seconds, answers the
/// handshake, then ignores everything but `quit`-less stdin until it is killed.
pub fn engine(dir: &Path, name: &str, delay: &str) -> PathBuf {
    script(
        dir,
        name,
        &format!(
            "echo $$ >> \"$(dirname \"$0\")/pids\"\n\
             while read l; do\n\
               case \"$l\" in\n\
                 uci) sleep {delay}; echo uciok;;\n\
                 isready) echo readyok;;\n\
                 go*) sleep 30;;\n\
               esac\n\
             done\n"
        ),
    )
}

/// The pids the engines in `dir` have recorded so far.
pub fn pids(dir: &Path) -> Vec<u32> {
    std::fs::read_to_string(dir.join("pids"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.parse().ok())
        .collect()
}

/// Whether `pid` is a live process (a zombie awaiting reaping is not).
pub fn alive(pid: u32) -> bool {
    let out = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&out.stdout);
    !stat.trim().is_empty() && !stat.trim().starts_with('Z')
}

/// Wait up to `secs` seconds for `cond` to hold.
pub async fn eventually(secs: u64, mut cond: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        if cond() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    cond()
}

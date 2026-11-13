//! Persist attempts before networking, with a process lock to avoid races.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

pub(super) fn claim_check(dir: &Path, now: u64) -> io::Result<bool> {
    fs::create_dir_all(dir)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("version-check"))?;
    // Never wait for another pup process. Closing the file releases the lock,
    // including when a process crashes; the state file must not be removed.
    if file.try_lock().is_err() {
        return Ok(false);
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    if let Ok(last_check) = contents.trim().parse::<u64>() {
        if now.saturating_sub(last_check) < CHECK_INTERVAL_SECONDS {
            return Ok(false);
        }
    }
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    write!(file, "{now}")?;
    file.sync_data()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn first_use_then_once_per_24_hours() {
        let tmp = TempDir::new("version_check_interval");
        let dir = tmp.path().join("new_config_dir");
        assert!(claim_check(&dir, NOW).unwrap());
        assert_eq!(
            fs::read_to_string(dir.join("version-check")).unwrap(),
            NOW.to_string()
        );
        assert!(!claim_check(&dir, NOW).unwrap());
        assert!(!claim_check(&dir, NOW + CHECK_INTERVAL_SECONDS - 1).unwrap());
        assert!(claim_check(&dir, NOW + CHECK_INTERVAL_SECONDS).unwrap());
    }

    #[test]
    fn future_timestamp_suppresses_checks_after_clock_rollback() {
        let tmp = TempDir::new("version_check_clock");
        assert!(claim_check(tmp.path(), NOW).unwrap());
        assert!(!claim_check(tmp.path(), NOW - 1).unwrap());
    }

    #[test]
    fn empty_or_corrupt_timestamp_is_replaced() {
        let tmp = TempDir::new("version_check_corrupt");
        for contents in ["", "invalid", "-1"] {
            fs::write(tmp.path().join("version-check"), contents).unwrap();
            assert!(claim_check(tmp.path(), NOW).unwrap());
            assert!(!claim_check(tmp.path(), NOW + 1).unwrap());
        }
    }

    #[test]
    fn locked_state_is_skipped_without_waiting() {
        let tmp = TempDir::new("version_check_locked");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tmp.path().join("version-check"))
            .unwrap();
        file.lock().unwrap();
        assert!(!claim_check(tmp.path(), NOW).unwrap());
        drop(file);
        assert!(claim_check(tmp.path(), NOW).unwrap());
    }

    #[test]
    fn concurrent_checks_have_one_winner() {
        let tmp = TempDir::new("version_check_concurrent");
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        claim_check(tmp.path(), NOW).unwrap()
                    })
                })
                .collect();
            assert_eq!(
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .filter(|&claimed| claimed)
                    .count(),
                1
            );
        });
    }

    #[test]
    fn unavailable_state_directory_returns_an_error() {
        let tmp = TempDir::new("version_check_unavailable");
        let file = tmp.path().join("file");
        fs::write(&file, "").unwrap();
        assert!(claim_check(&file, NOW).is_err());
        fs::create_dir(tmp.path().join("version-check")).unwrap();
        assert!(claim_check(tmp.path(), NOW).is_err());
    }
}

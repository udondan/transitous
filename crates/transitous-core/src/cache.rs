//! A cache of responses on disk, shared by every process on the machine, so
//! that repeated calls do not reach Transitous. Stop searches keep for 15
//! minutes, connections, departures and trips for a minute, because their
//! realtime data changes.
//!
//! The cache is best effort: an entry that cannot be read or written is a
//! miss.

use std::path::PathBuf;
use std::time::Duration;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::usage::Endpoint;

/// How long the answers of an endpoint keep.
pub fn ttl(endpoint: Endpoint) -> Duration {
    match endpoint {
        Endpoint::Geocode | Endpoint::ReverseGeocode => Duration::from_secs(15 * 60),
        Endpoint::Plan | Endpoint::Stoptimes | Endpoint::Trip => Duration::from_secs(60),
    }
}

/// Entries older than this are removed.
const MAX_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    key: String,
    stored: Timestamp,
    body: String,
}

#[derive(Debug, Clone)]
pub struct Cache {
    dir: Option<PathBuf>,
}

impl Cache {
    /// A cache in `dir`, or none.
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    /// The body stored for `key` within `ttl` before `now`.
    pub fn get(&self, key: &str, ttl: Duration, now: Timestamp) -> Option<Vec<u8>> {
        let path = self.path(key)?;
        let text = std::fs::read_to_string(path).ok()?;
        let entry: Entry = serde_json::from_str(&text).ok()?;
        let age = now.as_second() - entry.stored.as_second();
        let fresh = (0..i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX)).contains(&age);
        (entry.key == key && fresh).then(|| entry.body.into_bytes())
    }

    /// Stores `body` for `key`, and removes old entries.
    pub fn put(&self, key: &str, body: &[u8], now: Timestamp) {
        let (Some(dir), Some(path)) = (&self.dir, self.path(key)) else {
            return;
        };
        let Ok(body) = String::from_utf8(body.to_vec()) else {
            return;
        };
        let entry = Entry {
            key: key.to_owned(),
            stored: now,
            body,
        };
        // Plain data does not fail to serialize; an empty file would only be
        // a miss.
        let text = serde_json::to_vec(&entry).unwrap_or_default();
        let _ = std::fs::create_dir_all(dir);
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let stored =
            std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, &path));
        if stored.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        self.prune();
    }

    fn path(&self, key: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{:016x}.json", fnv1a(key))))
    }

    /// Removes the entries last written more than [`MAX_AGE`] ago.
    fn prune(&self) {
        let Some(entries) = self
            .dir
            .as_ref()
            .and_then(|dir| std::fs::read_dir(dir).ok())
        else {
            return;
        };
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > MAX_AGE);
            if old {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// The 64-bit FNV-1a hash: stable across versions and platforms, unlike the
/// hasher of the standard library.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + second).unwrap()
    }

    #[test]
    fn keeps_answers_for_their_time() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(Some(dir.path().join("cache")));
        let ttl = Duration::from_secs(60);
        assert_eq!(cache.get("a", ttl, at(0)), None);
        cache.put("a", b"{}", at(0));
        assert_eq!(cache.get("a", ttl, at(59)).as_deref(), Some(&b"{}"[..]));
        assert_eq!(cache.get("a", ttl, at(60)), None);
        assert_eq!(cache.get("a", ttl, at(-1)), None);
        assert_eq!(cache.get("b", ttl, at(1)), None);
    }

    #[test]
    fn ignores_entries_of_another_key() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(Some(dir.path().to_owned()));
        cache.put("a", b"1", at(0));
        let theirs = cache.path("a").unwrap();
        std::fs::rename(&theirs, cache.path("b").unwrap()).unwrap();
        assert_eq!(cache.get("b", Duration::from_secs(60), at(1)), None);
    }

    #[test]
    fn skips_bodies_that_are_not_text() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(Some(dir.path().to_owned()));
        cache.put("a", &[0xff, 0xfe], at(0));
        assert_eq!(cache.get("a", Duration::from_secs(60), at(1)), None);
    }

    #[test]
    fn removes_old_entries() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.json");
        std::fs::write(&old, "{}").unwrap();
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(std::time::SystemTime::now() - 2 * MAX_AGE)
            .unwrap();
        let cache = Cache::new(Some(dir.path().to_owned()));
        cache.put("a", b"1", at(0));
        assert!(!old.exists());
        assert!(cache.path("a").unwrap().exists());
    }

    #[test]
    fn without_a_directory_nothing_is_kept() {
        let cache = Cache::new(None);
        cache.put("a", b"1", at(0));
        assert_eq!(cache.get("a", Duration::from_secs(60), at(1)), None);
    }

    #[test]
    fn a_cache_it_cannot_write_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();
        let cache = Cache::new(Some(blocker.join("cache")));
        cache.put("a", b"1", at(0));
        assert_eq!(cache.get("a", Duration::from_secs(60), at(1)), None);
    }

    #[test]
    fn stop_searches_keep_longer() {
        assert_eq!(ttl(Endpoint::Geocode), Duration::from_secs(900));
        assert_eq!(ttl(Endpoint::ReverseGeocode), Duration::from_secs(900));
        assert_eq!(ttl(Endpoint::Plan), Duration::from_secs(60));
    }

    #[test]
    fn hashes_stably() {
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }
}

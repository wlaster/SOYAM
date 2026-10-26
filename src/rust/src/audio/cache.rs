use crate::api::models::AudioQuality;
use foldhash::HashMap;
use foldhash::HashMapExt;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};

const STRM_URL_TTL: Duration = Duration::from_secs(45);

#[derive(Clone)]
struct CachedUrl {
    url: String,
    mirror_urls: Vec<String>,
    codec: String,
    quality: AudioQuality,
    fetched_at: Instant,
}

#[derive(Clone, Default)]
pub struct UrlCache {
    cache: Arc<RwLock<HashMap<String, CachedUrl>>>,
}

impl UrlCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get(&self, track_id: &str, quality: AudioQuality) -> Option<(String, Vec<String>, String)> {
        let entry = self.cache.read().get(track_id).cloned()?;
        if entry.quality != quality || entry.fetched_at.elapsed() >= STRM_URL_TTL {
            return None;
        }
        Some((entry.url, entry.mirror_urls, entry.codec))
    }

    pub fn insert(&self, track_id: String, url: String, mirror_urls: Vec<String>, codec: String, quality: AudioQuality) {
        let mut cache = self.cache.write();
        // Sweep expired entries + cap size so a long session doesn't grow
        // one entry per played track forever.
        cache.retain(|_, e| e.fetched_at.elapsed() < STRM_URL_TTL);
        if cache.len() > 200 {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, e)| e.fetched_at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest);
            }
        }
        cache.insert(
            track_id,
            CachedUrl {
                url,
                mirror_urls,
                codec,
                quality,
                fetched_at: Instant::now(),
            },
        );
    }

    pub fn remove(&self, track_id: &str) {
        self.cache.write().remove(track_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_quality_and_late_prefetch_responses_never_match_new_quality() {
        let cache = UrlCache::new();
        cache.insert("song".into(), "low-url".into(), vec![], "aac".into(), AudioQuality::Low);
        assert!(cache.get("song", AudioQuality::Low).is_some());
        assert!(cache.get("song", AudioQuality::High).is_none());
        cache.insert("song".into(), "high-url".into(), vec![], "flac".into(), AudioQuality::High);
        assert_eq!(cache.get("song", AudioQuality::High).unwrap().2, "flac");
        cache.insert("song".into(), "late-low-url".into(), vec![], "aac".into(), AudioQuality::Low);
        assert!(cache.get("song", AudioQuality::High).is_none());
    }
}

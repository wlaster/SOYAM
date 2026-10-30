use crate::api::models::AudioQuality;
use crate::audio::cache::UrlCache;
use crate::audio::progress::TrackProgress;
use crate::http::ApiService;
use crate::storage::cache::TrackCache;
use crate::stream;
use foldhash::HashMap;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use yandex_music::model::track::Track;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct PreparedStream {
    pub session: stream::StreamingSession,
    pub progress: Arc<TrackProgress>,
    pub codec: String,
    /// Starts disarmed; `AudioController` arms it once this session is the one
    /// actually feeding the engine. See `BufferingGate`.
    pub buffering: Arc<stream::BufferingGate>,
}

// Keep one prepared stream ahead of the current track. This mirrors the desktop client's
// single-source preload policy and avoids competing network reads during rapid skips.
// During a transition the current track's prepared session has not yet been
// consumed when the queue asks to prepare its successor. Reserve both slots.
const MAX_PREWARM_ENTRIES: usize = 2;

#[derive(Default)]
struct PrewarmCache {
    entries: HashMap<String, PreparedStream>,
    in_flight: HashMap<String, u64>,
    next_generation: u64,
    quality: Option<AudioQuality>,
    // Insertion order, oldest first, for FIFO eviction once `entries` is full.
    order: VecDeque<String>,
}

impl PrewarmCache {
    fn ensure_quality(&mut self, quality: AudioQuality) {
        if self.quality != Some(quality) {
            self.entries.clear();
            self.in_flight.clear();
            self.order.clear();
            self.quality = Some(quality);
        }
    }

    fn retain_interest(&mut self, current: Option<&str>, next: Option<&str>) {
        let keep = |id: &str| current == Some(id) || next == Some(id);
        self.entries.retain(|id, _| keep(id));
        self.in_flight.retain(|id, _| keep(id));
        self.order.retain(|id| keep(id));
    }
    fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id) || self.in_flight.contains_key(id)
    }

    /// True when a finished decode session is cached for `id` (not merely
    /// in flight). Used to skip redundant URL prefetching for that track:
    /// session building already resolved and cached its URL.
    fn has_ready(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    fn start(&mut self, id: &str) -> Option<u64> {
        if self.contains(id) || self.entries.len() + self.in_flight.len() >= MAX_PREWARM_ENTRIES {
            return None;
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        let generation = self.next_generation;
        self.in_flight.insert(id.to_owned(), generation);
        Some(generation)
    }

    fn finish(&mut self, id: &str, generation: u64, result: Option<PreparedStream>) {
        if self.in_flight.get(id) == Some(&generation) {
            self.in_flight.remove(id);
            if let Some(result) = result {
                self.insert(id.to_owned(), result);
            }
        }
    }

    fn remove(&mut self, id: &str) -> Option<PreparedStream> {
        let result = self.entries.remove(id);
        if result.is_some() {
            self.order.retain(|existing| existing != id);
        }
        result
    }

    fn invalidate(&mut self, id: &str) -> Option<PreparedStream> {
        self.in_flight.remove(id);
        self.remove(id)
    }

    fn insert(&mut self, id: String, result: PreparedStream) {
        if self.entries.contains_key(&id) {
            return;
        }
        while self.entries.len() >= MAX_PREWARM_ENTRIES {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
        self.order.push_back(id.clone());
        self.entries.insert(id, result);
    }
}

#[derive(Clone)]
pub struct StreamManager {
    api: Arc<ApiService>,
    url_cache: UrlCache,
    track_cache: Arc<TrackCache>,
    prewarm_cache: Arc<Mutex<PrewarmCache>>,
    http_client: reqwest::Client,
}

impl StreamManager {
    pub fn update_prewarm_interest(&self, current: Option<&str>, next: Option<Track>) {
        self.prewarm_cache.lock().retain_interest(current, next.as_ref().map(|track| track.id.as_str()));
        if let Some(next) = next {
            self.prewarm(next);
        }
    }
    pub fn new(api: Arc<ApiService>, url_cache: UrlCache, track_cache: Arc<TrackCache>) -> Self {
        let http_client = crate::util::tls::builder()
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(std::time::Duration::from_secs(60))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            api,
            url_cache,
            track_cache,
            prewarm_cache: Arc::new(Mutex::new(PrewarmCache::default())),
            http_client,
        }
    }

    pub fn prewarm(&self, track: Track) {
        let id = track.id.clone();
        let quality = self.api.get_quality();
        let mut cache = self.prewarm_cache.lock();
        cache.ensure_quality(quality);
        let Some(generation) = cache.start(&id) else {
            return;
        };

        drop(cache);
        let this = self.clone();
        tokio::spawn(async move {
            let result = match this.create_stream_session(&track).await {
                Ok(session) => Some(session),
                Err(e) => {
                    tracing::warn!(track_id = %track.id, error = %e, "prewarm_failed");
                    None
                }
            };
            this.prewarm_cache
                .lock()
                .finish(&track.id, generation, result);
        });
    }

    pub fn invalidate_track(&self, track_id: &str) {
        self.url_cache.remove(track_id);
        let mut cache = self.prewarm_cache.lock();
        cache.invalidate(track_id);
    }

    /// Whether a finished prewarm session is cached for `track_id`.
    /// Its URL was resolved during session building, so the URL prefetcher
    /// can skip this id without risking a cold start.
    pub fn has_prewarm_ready(&self, track_id: &str) -> bool {
        let mut cache = self.prewarm_cache.lock();
        cache.ensure_quality(self.api.get_quality());
        cache.has_ready(track_id)
    }

    pub async fn is_track_offline(&self, track_id: &str) -> bool {
        self.track_cache.get_track_file(track_id).await.is_some()
    }

    pub async fn create_stream_session(&self, track: &Track) -> Result<PreparedStream> {
        let resolved = if crate::external_music::enabled() && !crate::external_music::is_external(&track.id) && track.storage_dir.is_none() {
            self.api.fetch_tracks(vec![track.id.clone()]).await.ok().and_then(|tracks| tracks.into_iter().next())
        } else { None };
        let track = resolved.as_ref().unwrap_or(track);
        if (crate::external_music::is_unavailable(&track.id) || track.available == Some(false)) && !crate::external_music::enabled() {
            return Err("Включите «Недоступные треки» в настройках".into());
        }
        let quality = self.api.get_quality();
        {
            let mut cache = self.prewarm_cache.lock();
            cache.ensure_quality(quality);
            if let Some(res) = cache.remove(&track.id) {
                if !crate::external_music::is_unavailable(&track.id) { return Ok(res); }
            }
        }

        let progress = Arc::new(TrackProgress::new());

        let progress_clone = progress.clone();

        // 1. Check if track is fully downloaded in offline cache
        if !crate::external_music::is_unavailable(&track.id) && let Some((path, codec)) = self.track_cache.get_track_file(&track.id).await {
            let codec_clone = codec.clone();

            // Blocking FS stays off the async executor.
            let (file, total_bytes) = tokio::task::spawn_blocking(move || {
                let file = std::fs::File::open(&path)?;
                let total = file.metadata().map(|m| m.len()).unwrap_or(0);
                std::io::Result::Ok((file, total))
            })
            .await
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?
            .map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "failed to open offline file: {}",
                    e
                ))
            })?;
            if total_bytes == 0 {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                    "offline file is empty",
                ));
            }

            let session = tokio::task::spawn_blocking(move || {
                stream::create_streaming_session(file, total_bytes, codec_clone, progress_clone)
            })
            .await
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))??;

            return Ok(PreparedStream {
                session,
                progress,
                codec,
                // A local file never waits on the network, so this gate stays silent.
                buffering: stream::BufferingGate::new(),
            });
        }

        // 2. Fallback to streaming.
        let duration_ms = track.duration.map(|d| d.as_millis() as u64);
        let (url, mirror_urls, codec) = if track.available == Some(false) || crate::external_music::is_unavailable(&track.id) {
            let url = crate::external_music::stream_url(track).await.map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;
            (url, vec![], "mp3".to_string())
        } else if let Some(cached) = self.url_cache.get(&track.id, quality) {
            cached
        } else {
            let info = self
                .api
                .fetch_track_stream_info(track.id.clone(), quality)
                .await;
            match info {
            Ok(info) => {
            self.url_cache.insert(
                track.id.clone(),
                info.url.clone(),
                info.mirror_urls.clone(),
                info.codec.clone(),
                quality,
            );
            (info.url, info.mirror_urls, info.codec)
            }
            Err(error) => {
                // A cached playlist entry can lack availability. Confirm with
                // Yandex before treating an error as an unavailable recording.
                let confirmed = if crate::external_music::enabled() {
                    self.api.fetch_tracks(vec![track.id.clone()]).await.ok()
                        .and_then(|tracks| tracks.into_iter().next())
                        .filter(|full| full.available == Some(false))
                } else { None };
                if let Some(full) = confirmed {
                    crate::external_music::note_availability(&full);
                    let url = crate::external_music::stream_url(&full).await.map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;
                    (url, vec![], "mp3".to_string())
                } else {
                    return Err(error.to_string().into());
                }
            }
            }
        };

        let client = self.http_client.clone();
        let buffering = stream::BufferingGate::new();
        let track_id = track.id.clone();
        let url_cache = self.url_cache.clone();
        let data_source = stream::StreamingDataSource::new(
            client,
            url,
            mirror_urls,
            Arc::clone(&progress),
            Arc::clone(&buffering),
            duration_ms,
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            // Expired/rejected stream URL: drop it so the next attempt refetches.
            if msg.contains("403") || msg.contains("rejected") {
                url_cache.remove(&track_id);
            }
            Box::<dyn std::error::Error + Send + Sync>::from(msg)
        })?;
        let codec_clone = codec.clone();

        let total_bytes = data_source.total_bytes();
        let session = tokio::task::spawn_blocking(move || {
            stream::create_streaming_session(data_source, total_bytes, codec_clone, progress_clone)
        })
        .await
        .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))??;

        Ok(PreparedStream {
            session,
            progress,
            codec,
            buffering,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{PrewarmCache, AudioQuality};

    #[test]
    fn next_track_prepares_while_current_session_is_still_reserved() {
        let mut cache = PrewarmCache::default();
        cache.start("current").unwrap();
        cache.retain_interest(Some("current"), Some("next"));
        assert!(cache.start("next").is_some());
        assert!(cache.contains("current"));
        assert!(cache.contains("next"));
        cache.retain_interest(Some("next"), Some("third"));
        assert!(!cache.contains("current"));
        assert!(cache.start("third").is_some());
    }

    #[test]
    fn quality_change_rejects_prepared_and_in_flight_old_quality() {
        let mut cache = PrewarmCache::default();
        cache.ensure_quality(AudioQuality::Low);
        let old = cache.start("next").unwrap();
        cache.ensure_quality(AudioQuality::High);
        assert!(!cache.contains("next"));
        let current = cache.start("next").unwrap();
        cache.finish("next", old, None);
        assert_eq!(cache.in_flight.get("next"), Some(&current));
    }

    #[test]
    fn stale_prewarm_finish_cannot_consume_a_new_attempt() {
        let mut cache = PrewarmCache::default();
        let first = cache.start("track-42").expect("first attempt starts");

        cache.invalidate("track-42");
        let second = cache.start("track-42").expect("second attempt starts");
        assert_ne!(first, second);

        cache.finish("track-42", first, None);
        assert_eq!(cache.in_flight.get("track-42"), Some(&second));

        cache.finish("track-42", second, None);
        assert!(!cache.in_flight.contains_key("track-42"));
    }

    #[test]
    fn duplicate_prewarm_start_is_rejected() {
        let mut cache = PrewarmCache::default();
        cache.start("track-7").expect("first attempt starts");
        assert!(cache.start("track-7").is_none());
        assert!(!cache.has_ready("track-7"));
    }

    #[test]
    fn invalidate_unblocks_a_new_attempt() {
        let mut cache = PrewarmCache::default();
        cache.start("track-7").expect("first attempt starts");
        cache.invalidate("track-7");
        assert!(!cache.has_ready("track-7"));
        cache
            .start("track-7")
            .expect("attempt restarts after invalidate");
    }
}

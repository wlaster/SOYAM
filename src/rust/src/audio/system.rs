use crate::audio::cache::UrlCache;
use crate::{
    audio::{
        commands::AudioMessage, controller::AudioController, fetcher::is_usable_wave_session,
        playback::PlaybackEngine, progress::TrackProgress, queue::QueueManager,
        queue::as_wave_seed, signals::AudioSignals, state::SystemState,
        stream_manager::StreamManager, yandex::YandexProvider,
    },
    http::{ApiService, LikedTracksSync, SessionExt},
};

#[cfg(not(any(target_os = "android")))]
use crate::audio::smtc::SmtcManager;

use parking_lot::RwLock as PRwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
#[cfg(not(any(target_os = "android")))]
use tokio::sync::Mutex;
use tokio::sync::{RwLock, mpsc};
use yandex_music::model::track::Track;

#[path = "system_loading.rs"]
mod system_loading;
#[path = "system_wave.rs"]
mod system_wave;

use system_wave::WavePostAction;

// Abort-on-Drop guard for detached SMTC tasks (polling loop has no
// other shutdown signal); explicit stop via `AudioSystem::shutdown`.
#[cfg(not(any(target_os = "android")))]
struct AbortOnDrop {
    handle: Option<tokio::task::JoinHandle<()>>,
}

#[cfg(not(any(target_os = "android")))]
impl AbortOnDrop {
    fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    fn shutdown(&mut self) {
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

#[cfg(not(any(target_os = "android")))]
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// Selects the head of the unified advance path (ended vs user skip).
#[derive(Clone, Copy, PartialEq, Eq)]
enum AdvanceReason {
    TrackEnded,
    Skipped,
}

pub type EffectHandles =
    Arc<parking_lot::RwLock<foldhash::HashMap<String, crate::audio::fx::EffectHandle>>>;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const PREVIOUS_TRACK_RESTART_THRESHOLD: Duration = Duration::from_secs(5);

pub struct AudioSystem {
    controller: AudioController,
    queue: QueueManager,
    yandex: YandexProvider,
    error_sink: Arc<dyn Fn(String) + Send + Sync>,
    state: Arc<RwLock<SystemState>>,
    signals: AudioSignals,
    tx: mpsc::Sender<AudioMessage>,
    db: Arc<tokio::sync::Mutex<crate::db::AppDatabase>>,
    context_generation: Arc<AtomicU64>,
    #[cfg(not(any(target_os = "android")))]
    smtc_guards: Vec<AbortOnDrop>,
}

impl AudioSystem {
    pub async fn spawn(
        error_sink: Arc<dyn Fn(String) + Send + Sync>,
        api: Arc<ApiService>,
        db: Arc<tokio::sync::Mutex<crate::db::AppDatabase>>,
        _http_cache: Arc<crate::storage::cache::HttpCache>,
        track_cache: Arc<crate::storage::cache::TrackCache>,
    ) -> Result<(
        mpsc::Sender<AudioMessage>,
        AudioSignals,
        Arc<RwLock<SystemState>>,
        EffectHandles,
    )> {
        let (tx, mut rx) = mpsc::channel(100);

        let engine = PlaybackEngine::new(tx.clone())?;
        let url_cache = UrlCache::new();
        let stream_manager = Arc::new(
            tokio::task::spawn_blocking({
                let api = api.clone();
                let url_cache = url_cache.clone();
                let track_cache = track_cache.clone();
                move || StreamManager::new(api, url_cache, track_cache)
            })
            .await
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?,
        );

        let signals = AudioSignals::new();
        let track_progress_inner = Arc::new(TrackProgress::default());
        let track_progress = Arc::new(PRwLock::new(track_progress_inner.clone()));

        let controller = AudioController::new(
            engine,
            stream_manager.clone(),
            tx.clone(),
            error_sink.clone(),
            signals.clone(),
            track_progress.clone(),
        );

        let mut queue = QueueManager::new(
            api.clone(),
            url_cache,
            stream_manager.clone(),
            signals.clone(),
            track_progress_inner,
        );

        let state = Arc::new(RwLock::new(SystemState::default()));
        queue.set_library(state.clone());

        // Load liked tracks from DB for instant start
        {
            let mut db = db.lock().await;
            if let Ok(Some(ids)) = db.load_setting::<Vec<String>>("disliked_track_ids").await {
                state.write().await.liked.replace_disliked_ids(&ids);
            }
            if let Ok(ids) = db.load_liked_tracks().await {
                let revision: Option<u64> = db
                    .load_setting("liked_tracks_revision")
                    .await
                    .ok()
                    .flatten();
                let mut state = state.write().await;
                state.liked.set_liked_ids(ids);
                state.liked.revision = revision;
                signals.library_changed.send_replace(());
            }
        }

        #[cfg(not(any(target_os = "android")))]
        let (smtc, smtc_cmd_rx) = {
            let (smtc_cmd_tx, smtc_cmd_rx) = mpsc::unbounded_channel();
            let smtc = Arc::new(Mutex::new(SmtcManager::new(
                smtc_cmd_tx,
                _http_cache.clone(),
            )?));
            (smtc, smtc_cmd_rx)
        };

        let yandex = YandexProvider::new(api.clone(), signals.clone());

        let effect_handles = controller.get_effect_handles();

        let system = Self {
            controller,
            queue,
            yandex,
            error_sink: error_sink.clone(),
            state: state.clone(),
            signals: signals.clone(),
            tx: tx.clone(),
            db,
            context_generation: Arc::new(AtomicU64::new(0)),
            #[cfg(not(any(target_os = "android")))]
            smtc_guards: Vec::new(),
        };

        let mut system_loop = system;

        // Background task for SMTC
        #[cfg(not(any(target_os = "android")))]
        {
            let tx_clone = tx.clone();
            let mut rx_smtc = smtc_cmd_rx;
            let fwd_handle = tokio::spawn(async move {
                while let Some(msg) = rx_smtc.recv().await {
                    let _ = tx_clone.send(msg).await;
                }
            });
            system_loop.smtc_guards.push(AbortOnDrop::new(fwd_handle));
        }

        // Monitor signals to update SMTC
        #[cfg(not(any(target_os = "android")))]
        {
            let smtc_clone = smtc.clone();
            let signals_clone = signals.clone();
            let poll_handle = tokio::spawn(async move {
                let mut last_track_id = None;
                let mut last_playing = false;

                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                    let current_track = signals_clone.current_track.get();
                    let current_track_id = current_track.as_ref().map(|t| (t.id.clone(), crate::api::models::get_any_cover(t)));
                    let is_playing = signals_clone.is_playing.get();

                    let mut smtc_guard: tokio::sync::MutexGuard<SmtcManager> =
                        smtc_clone.lock().await;

                    if current_track_id != last_track_id {
                        if let Some(track) = current_track {
                            smtc_guard.update_metadata(&track);
                        }
                        last_track_id = current_track_id;
                    }

                    if is_playing != last_playing {
                        smtc_guard.update_playback_status(is_playing);
                        last_playing = is_playing;
                    }
                }
            });
            system_loop.smtc_guards.push(AbortOnDrop::new(poll_handle));
        }

        // Main Audio Loop
        tokio::spawn(async move {
            let mut library_changed = system_loop.signals.library_changed_rx.clone();
            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        let Some(msg) = msg else { break; };
                        system_loop.process_message(msg).await;
                    }
                    result = library_changed.changed() => {
                        if result.is_err() { break; }
                    }
                }
                if let Some(track) = system_loop.signals.current_track.get()
                    && system_loop.track_is_disliked(&track).await
                {
                    system_loop.play_filtered_track(track, false, Duration::ZERO, false).await;
                }
            }
            // Channel closed on context teardown: stop SMTC loops and the
            // controller monitor/playback task in a defined order instead
            // of relying on Drop alone (Drop guards stay as a backstop).
            system_loop.shutdown().await;
        });

        Ok((tx, signals, state, effect_handles))
    }

    /// Universal spawn for loading playback context
    fn begin_context_change(&self) -> u64 {
        self.context_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    fn spawn_fetch_context<F, Fut>(&self, generation: u64, fetcher: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<
                Output = std::result::Result<
                    (
                        crate::audio::queue::PlaybackContext,
                        im::Vector<yandex_music::model::track::Track>,
                        usize,
                    ),
                    String,
                >,
            > + Send
            + 'static,
    {
        let tx = self.tx.clone();
        self.signals.set_buffering(true);
        tokio::spawn(async move {
            let result = fetcher().await;
            let _ = tx
                .send(AudioMessage::ContextFetched { generation, result })
                .await;
        });
    }

    pub fn get_effect_handles(&self) -> EffectHandles {
        self.controller.get_effect_handles()
    }

    // Explicit stop for background tasks: SMTC loops plus the controller
    // monitor loop and in-flight playback task. Idempotent; Drop guards
    // abort the same tasks as a backstop. Call on context teardown
    // (e.g. re-login spawns a fresh AudioSystem in-process).
    pub async fn shutdown(&mut self) {
        #[cfg(not(any(target_os = "android")))]
        for g in &mut self.smtc_guards {
            g.shutdown();
        }
        self.controller.shutdown().await;
    }

    // Unified offline-backed single-track spawn; `source` selects the
    // album/playlist/liked remote branch (resolver lives in system_loading).
    fn spawn_single_track_with_offline(
        &self,
        source: system_loading::SingleTrackSource,
        tid: String,
    ) {
        // Offline check + DB read run in background so the actor stays
        // responsive to Pause/Seek/Next while they complete.
        let generation = self.begin_context_change();
        let tx = self.tx.clone();
        let stream_manager = self.queue.stream_manager.clone();
        let db = self.db.clone();
        let yandex = self.yandex.clone();
        let state = match &source {
            system_loading::SingleTrackSource::Liked => Some(self.state.clone()),
            _ => None,
        };
        tokio::spawn(async move {
            system_loading::resolve_single_track_offline_or_remote(
                source,
                tid,
                stream_manager,
                db,
                state,
                yandex,
                tx,
                generation,
            )
            .await;
        });
    }

    async fn load_context(
        &mut self,
        ctx: crate::audio::queue::PlaybackContext,
        tracks: im::Vector<Track>,
        index: usize,
    ) {
        self.load_context_inner(None, ctx, tracks, index).await;
    }

    async fn load_fetched_context(
        &mut self,
        generation: u64,
        ctx: crate::audio::queue::PlaybackContext,
        tracks: im::Vector<Track>,
        index: usize,
    ) {
        self.load_context_inner(Some(generation), ctx, tracks, index)
            .await;
    }

    // Single loader; `Some(generation)` enables the fetched-path
    // generation guard (entry + right before applying state).
    async fn load_context_inner(
        &mut self,
        generation: Option<u64>,
        ctx: crate::audio::queue::PlaybackContext,
        tracks: im::Vector<Track>,
        index: usize,
    ) {
        if let Some(g) = generation
            && self.context_generation.load(Ordering::Acquire) != g
        {
            return;
        }

        let in_wave = matches!(&ctx, crate::audio::queue::PlaybackContext::Wave(_));
        if let Some(track) = self.queue.load(ctx, tracks, index).await {
            // Keep the check in the actor immediately before applying the loaded
            // queue/controller state. ContextFetched messages are the only path
            // where a background fetch can reach playback.
            if let Some(g) = generation
                && self.context_generation.load(Ordering::Acquire) != g
            {
                return;
            }

            if in_wave {
                self.send_wave_started();
            }
            self.play_filtered_track(track.clone(), false, Duration::ZERO, false)
                .await;
            if in_wave {
                self.send_wave_track_started(&track);
            }
        }
    }

    async fn play_filtered_track(&mut self, mut track: Track, start_paused: bool, mut position: Duration, soft_reload: bool) {
        let limit = self.signals.queue.get().len().max(1);
        for _ in 0..=limit {
            let disliked = self.state.read().await.liked.is_disliked(&track.id);
            if !disliked && (crate::app::settings::ALLOW_ADULT_CONTENT.load(Ordering::Relaxed) || !crate::app::settings::is_adult_track(&track)) {
                if !crate::app::settings::is_adult_track(&track) {
                    let api = self.yandex.api.clone();
                    let tx = self.tx.clone();
                    let id = track.id.clone();
                    tokio::spawn(async move {
                        if let Ok(tracks) = api.fetch_tracks(vec![id]).await
                            && let Some(full) = tracks.into_iter().next()
                        {
                            let _ = tx.send(AudioMessage::TrackMetadataResolved(full)).await;
                        }
                    });
                }
                self.controller.play_track(track, start_paused, position, soft_reload).await;
                return;
            }
            match self.queue.skip_track().await {
                Some(next) => { track = next; position = Duration::ZERO; }
                None => break,
            }
        }
        self.controller.stop().await;
    }

    async fn track_is_disliked(&self, track: &Track) -> bool {
        self.state.read().await.liked.is_disliked(&track.id)
    }

    async fn resume_filtered(&mut self) {
        if let Some(track) = self.signals.current_track.get()
            && self.track_is_disliked(&track).await
        {
            self.play_filtered_track(track, false, Duration::ZERO, false).await;
            return;
        }
        self.controller.resume().await;
    }

    async fn load_standalone(
        &mut self,
        tracks: im::Vector<Track>,
        start_paused: bool,
        position: Duration,
    ) {
        if let Some(track) = self
            .queue
            .load(crate::audio::queue::PlaybackContext::Standalone, tracks, 0)
            .await
        {
            self.play_filtered_track(track, start_paused, position, false)
                .await;
        }
    }

    async fn recreate_stream(&mut self) {
        let device = self.signals.selected_device.get();
        if let Err(e) = self.controller.recreate_engine(device.as_deref()) {
            tracing::error!("Failed to recreate stream: {}", e);
        } else {
            self.reload_track().await;
        }
    }

    async fn reload_track(&mut self) {
        if let Some(track) = self.signals.current_track.get() {
            if self.track_is_disliked(&track).await {
                self.play_filtered_track(track, false, Duration::ZERO, false).await;
                return;
            }
            let position_ms = self.signals.position_ms.get();
            self.signals.set_buffering(true);
            self.controller.invalidate_track(&track.id);
            self.controller.replace_track(track, position_ms).await;
        }
    }

    async fn process_message(&mut self, msg: AudioMessage) {
        match msg {
            AudioMessage::TrackMetadataResolved(full) => {
                use crate::util::track::CleanId;
                if let Some(mut current) = self.signals.current_track.get()
                    && current.id.to_base_id() == full.id.to_base_id()
                {
                    current.explicit = Some(crate::app::settings::is_adult_track(&full));
                    if crate::api::models::get_any_cover(&current).is_none() {
                        current.cover_uri = crate::api::models::get_any_cover(&full);
                    }
                    current.content_warning = full.content_warning;
                    self.signals.set_current_track(Some(current));
                }
            }
            AudioMessage::ContextFetched { generation, result } => match result {
                Ok((ctx, tracks, index)) => {
                    self.load_fetched_context(generation, ctx, tracks, index)
                        .await;
                }
                Err(e) => {
                    if self.context_generation.load(Ordering::Acquire) != generation {
                        return;
                    }
                    (self.error_sink)(e);
                    self.signals.set_buffering(false);
                }
            },
            AudioMessage::TrackWaveFetched { generation, previous_seeds, seeds, result } => {
                if self.context_generation.load(Ordering::Acquire) != generation
                { return; }
                match result {
                    Ok((ctx, tracks, _)) => {
                        if let Some(current) = self.signals.current_track.get() {
                            self.queue.load_track_wave(ctx, tracks, current).await;
                            self.signals.current_wave_seeds.set(seeds);
                            self.send_wave_started();
                        }
                    }
                    Err(error) => {
                        self.signals.current_wave_seeds.set(previous_seeds);
                        self.signals.changed.send_replace(());
                        (self.error_sink)(error);
                    },
                }
            }
            AudioMessage::PlayPause => {
                if self.signals.is_playing.get() {
                    self.controller.pause().await;
                } else {
                    self.resume_filtered().await;
                }
            }
            AudioMessage::Pause => self.controller.pause().await,
            AudioMessage::Resume => self.resume_filtered().await,
            AudioMessage::Stop => {
                self.begin_context_change();
                self.controller.stop().await;
                self.queue.clear();
            }
            AudioMessage::Next => {
                self.play_next().await;
            }
            AudioMessage::Prev => {
                if self.signals.position_ms.get()
                    > PREVIOUS_TRACK_RESTART_THRESHOLD.as_millis() as u64
                {
                    self.controller.seek(Duration::ZERO).await;
                    self.signals
                        .update_progress(0, self.signals.duration_ms.get());
                } else if let Some(prev_track) = self.queue.get_previous_track() {
                    self.play_filtered_track(prev_track, false, Duration::ZERO, false)
                        .await;
                }
            }
            AudioMessage::PreviousTrack => {
                if let Some(prev_track) = self.queue.get_previous_track() {
                    self.play_filtered_track(prev_track, false, Duration::ZERO, false)
                        .await;
                }
            }
            AudioMessage::TrackEnded => {
                self.on_track_ended().await;
            }
            AudioMessage::Seek(dur) => self.controller.seek(dur).await,
            AudioMessage::SetVolume(vol) => self.controller.set_volume(vol as f32 / 100.0),
            AudioMessage::SetTransientVolumeGain(gain) => {
                self.controller.set_transient_volume_gain(gain)
            }
            AudioMessage::ToggleMute => self.controller.toggle_mute(),

            AudioMessage::PlayTrackList(ids, selected) => {
                let generation = self.begin_context_change();
                let api = self.yandex.api.clone();
                let db = self.db.clone();
                let stream_manager = self.queue.stream_manager.clone();
                let current_generation = self.context_generation.clone();
                self.spawn_fetch_context(generation, move || async move {
                    use crate::util::track::CleanId;
                    let ids: Vec<String> = ids.iter().map(|id| id.to_base_id().to_owned()).collect();
                    let selected = selected.to_base_id().to_owned();
                    let offline = stream_manager.is_track_offline(&selected).await;
                    let metadata = db.lock().await.get_track_metadata(&ids).await.map_err(|e| e.to_string())?;
                    let mut fetched: std::collections::HashMap<String, Track> = metadata.iter()
                        .map(|entry| (entry.id.to_base_id().to_owned(), crate::util::track::track_from_metadata(entry))).collect();
                    let missing: Vec<_> = ids.iter().filter(|id| !fetched.contains_key(*id)).cloned().collect();
                    for chunk in missing.chunks(50) {
                        if current_generation.load(Ordering::Acquire) != generation {
                            return Err("Cancelled".to_string());
                        }
                        if !offline {
                            if let Ok(tracks) = api.fetch_tracks(chunk.to_vec()).await {
                                for track in tracks { fetched.insert(track.id.to_base_id().to_owned(), track); }
                            }
                        }
                    }
                    let (tracks, index) = crate::util::track::ordered_playback_tracks(&ids, &selected, &fetched)?;
                    Ok((crate::audio::queue::PlaybackContext::Standalone, tracks, index))
                });
            }
            AudioMessage::PlayTrack(track) => {
                self.begin_context_change();
                self.load_standalone(im::Vector::from(vec![track]), false, Duration::ZERO)
                    .await
            }
            AudioMessage::RestoreTrack(track, pos, was_playing) => {
                self.begin_context_change();
                self.load_standalone(im::Vector::from(vec![track]), !was_playing, pos)
                    .await
            }
            AudioMessage::LoadContext(ctx, tracks, index) => {
                self.begin_context_change();
                self.load_context(ctx, tracks, index).await;
            }
            AudioMessage::LoadTracks(tracks) => {
                self.begin_context_change();
                self.load_standalone(im::Vector::from(tracks), false, Duration::ZERO)
                    .await
            }
            AudioMessage::QueueTrack(track) => self.queue.queue_track(track),
            AudioMessage::PlayTrackNext(track) => self.queue.play_next(track),
            AudioMessage::RemoveFromQueue(idx) => self.queue.remove_track(idx),
            AudioMessage::ClearQueue => self.queue.clear(),
            AudioMessage::ToggleShuffle => self.queue.toggle_shuffle(),
            AudioMessage::ToggleRepeatMode => self.queue.toggle_repeat_mode(),

            AudioMessage::PlayPlaylist(kind) => {
                let generation = self.begin_context_change();
                let yandex = self.yandex.clone();
                self.spawn_fetch_context(generation, move || async move {
                    yandex
                        .fetch_playlist_context(kind, None)
                        .await
                        .map_err(|e| format!("Failed to load playlist: {e}"))
                });
            }
            AudioMessage::PlayAlbum(album_id) => {
                let generation = self.begin_context_change();
                let yandex = self.yandex.clone();
                self.spawn_fetch_context(generation, move || async move {
                    yandex
                        .fetch_album_context(album_id, None)
                        .await
                        .map_err(|e| format!("Failed to load album: {e}"))
                });
            }
            AudioMessage::PlayArtist(artist_id) => {
                let generation = self.begin_context_change();
                let yandex = self.yandex.clone();
                self.spawn_fetch_context(generation, move || async move {
                    yandex.fetch_artist_context(artist_id).await
                        .map_err(|e| format!("Failed to load artist: {e}"))
                });
            }
            AudioMessage::PlayAlbumTrack(aid, tid) => {
                self.spawn_single_track_with_offline(
                    system_loading::SingleTrackSource::AlbumTrack { album_id: aid },
                    tid,
                );
            }
            AudioMessage::PlayPlaylistTrack(kind, tid) => {
                self.spawn_single_track_with_offline(
                    system_loading::SingleTrackSource::PlaylistTrack { kind },
                    tid,
                );
            }
            AudioMessage::PlayLikedTrack(tid) => {
                self.spawn_single_track_with_offline(system_loading::SingleTrackSource::Liked, tid);
            }
            AudioMessage::StartWave(seeds) => {
                let generation = self.begin_context_change();
                let yandex = self.yandex.clone();
                let current = self.signals.current_track.get().filter(|track| {
                    seeds.len() == 1 && crate::audio::queue::clean_wave_seed(&seeds[0])
                        == format!("track:{}", crate::util::track::CleanId::to_base_id(track.id.as_str()))
                });
                if current.is_some() {
                    let previous_seeds = self.signals.current_wave_seeds.get();
                    self.signals.current_wave_seeds.set(seeds.clone());
                    self.signals.changed.send_replace(());
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let clean = seeds.iter().map(|seed| crate::audio::queue::clean_wave_seed(seed)).collect();
                        let result = yandex.api.create_session(clean).await
                            .and_then(|session| {
                                if session.sequence.is_empty() { return Err("Wave session is empty".into()); }
                                let tracks = session.sequence.iter().map(|item| item.track.clone()).collect();
                                Ok((crate::audio::queue::PlaybackContext::Wave(session), tracks, 0))
                            }).map_err(|e| format!("Failed to start wave: {e}"));
                        let _ = tx.send(AudioMessage::TrackWaveFetched {
                            generation, previous_seeds, seeds, result,
                        }).await;
                    });
                } else {
                    self.spawn_fetch_context(generation, move || async move {
                        yandex.fetch_wave_context(seeds).await
                            .map_err(|e| format!("Failed to start wave: {e}"))
                    });
                }
            }
            AudioMessage::RefreshWavePreferences => {
                if self.queue.in_wave() {
                    let generation = self.context_generation.load(Ordering::Acquire);
                    let api = self.yandex.api.clone();
                    let preference_revision = api.ai_preference_revision();
                    let mut seeds: Vec<String> = self.signals.current_wave_seeds.get().iter()
                        .map(|seed| crate::audio::queue::clean_wave_seed(seed)).collect();
                    if seeds.is_empty() { seeds.push("user:onyourwave".into()); }
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let result = api.create_session(seeds).await.map_err(|e| e.to_string());
                        let _ = tx.send(AudioMessage::WavePreferencesFetched {
                            generation, preference_revision, result }).await;
                    });
                }
            }
            AudioMessage::WavePreferencesFetched { generation, preference_revision, result } => {
                if self.context_generation.load(Ordering::Acquire) == generation
                    && self.yandex.api.ai_preference_revision() == preference_revision
                    && self.queue.in_wave() {
                    match result {
                        Ok(session) => {
                            if self.queue.apply_wave_preferences(session).await { self.send_wave_started(); }
                            else { (self.error_sink)("Не удалось обновить Мою волну".into()); }
                        },
                        Err(_) => (self.error_sink)("Настройка сохранена. Не удалось обновить Мою волну — запустите её ещё раз".into()),
                    }
                }
            }
            AudioMessage::SyncLiked => {
                Self::sync_liked_collection_with(
                    self.yandex.api.clone(),
                    self.state.clone(),
                    self.signals.clone(),
                    self.db.clone(),
                )
                .await;
                self.signals.changed.send_replace(());
            }
            AudioMessage::WaveFeedbackApplied { generation, session_id } => {
                if self.context_generation.load(Ordering::Acquire) == generation
                    && self.queue.wave_context().and_then(|s| s.radio_session_id).as_deref() == Some(&session_id) {
                    self.queue.refresh_wave_queue();
                }
            }
            AudioMessage::WaveLike(track_id) => {
                if self.queue.in_wave() {
                    let current = self.signals.current_track.get();
                    if current.as_ref().map(|t| t.id.as_str()) == Some(&track_id) {
                        if let Some(track) = current {
                            self.send_wave_like(&track);
                        }
                    } else {
                        let batch = self.queue.wave_batch_for_id(&track_id);
                        self.send_wave_feedback("like", Some(track_id), batch, None);
                    }
                }
            }
            AudioMessage::WaveUnlike(track_id) => {
                if self.queue.in_wave() {
                    let current = self.signals.current_track.get();
                    if current.as_ref().map(|t| t.id.as_str()) == Some(&track_id) {
                        if let Some(track) = current {
                            self.send_wave_unlike(&track);
                        }
                    } else {
                        let batch = self.queue.wave_batch_for_id(&track_id);
                        self.send_wave_feedback("unlike", Some(track_id), batch, None);
                    }
                }
            }
            AudioMessage::WaveDislike(track_id) => {
                use crate::util::track::CleanId;
                if self.queue.in_wave() {
                    let current = self.signals.current_track.get();
                    if current.as_ref().map(|t| t.id.to_base_id()) == Some(track_id.to_base_id()) {
                        if let Some(track) = current {
                            self.send_wave_dislike_skip(&track).await;
                        }
                    } else {
                        let batch = self.queue.wave_batch_for_id(&track_id);
                        self.send_wave_feedback("dislike", Some(track_id), batch, None);
                        self.queue.refresh_wave_queue();
                    }
                }
            }
            AudioMessage::WaveUndislike(track_id) => {
                if self.queue.in_wave() {
                    let current = self.signals.current_track.get();
                    if current.as_ref().map(|t| t.id.as_str()) == Some(&track_id) {
                        if let Some(track) = current {
                            self.send_wave_undislike(&track);
                        }
                    } else {
                        let batch = self.queue.wave_batch_for_id(&track_id);
                        self.send_wave_feedback("undislike", Some(track_id), batch, None);
                        self.queue.refresh_wave_queue();
                    }
                }
            }
            AudioMessage::SetAudioDevice(device_name) => {
                self.signals.selected_device.set(device_name.clone());
                // Don't hold the actor on a DB write; persist in background.
                let db = self.db.clone();
                tokio::spawn(async move {
                    let mut db = db.lock().await;
                    let _ = db.save_setting("audio_device", &device_name).await;
                });
                self.recreate_stream().await;
            }
            AudioMessage::RecreateStream => {
                self.recreate_stream().await;
            }
            AudioMessage::ReloadCurrentTrack => {
                self.reload_track().await;
            }
        }
    }

    async fn on_track_ended(&mut self) {
        self.advance_queue(AdvanceReason::TrackEnded).await;
    }

    async fn play_next(&mut self) {
        self.advance_queue(AdvanceReason::Skipped).await;
    }

    // Single advance path; reason selects wave_finish+get_next (natural end)
    // vs skip_wave/skip (user skip). Shared tail plays or auto-starts wave.
    async fn advance_queue(&mut self, reason: AdvanceReason) {
        let next = match reason {
            AdvanceReason::TrackEnded => {
                self.queue.wave_finish_track();
                self.queue.get_next_track().await
            }
            AdvanceReason::Skipped => {
                if self.queue.in_wave() {
                    self.queue.skip_wave_track().await
                } else {
                    self.queue.skip_track().await
                }
            }
        };

        if let Some(next_track) = next {
            if self.queue.in_wave() {
                self.send_wave_track_started(&next_track);
            }
            self.play_filtered_track(next_track, false, Duration::ZERO, false)
                .await;
        } else {
            // Queue ended, start "My Wave"
            let yandex = self.yandex.clone();
            let generation = self.begin_context_change();
            self.spawn_fetch_context(generation, move || async move {
                yandex
                    .fetch_wave_context(vec!["user:onyourwave".to_string()])
                    .await
                    .map_err(|e| format!("Failed to auto-start wave: {e}"))
            });
        }
    }

    fn send_wave_feedback(
        &self,
        feedback_type: &'static str,
        track_id: Option<String>,
        batch_id: Option<String>,
        total_played: Option<Duration>,
    ) {
        let session = match self.queue.wave_context() {
            Some(s) => s,
            None => return,
        };
        // Never send feedback for a dead session: without radio_session_id
        // the request goes to the user:onyourwave fallback URL with a foreign
        // batch_id (HTTP 400), and a terminated session is rejected too.
        // The session is repaired by preservation (fetcher) / recreate (queue).
        if session.terminated || !is_usable_wave_session(&session) {
            tracing::error!(
                feedback_type,
                track_id = track_id.as_deref().unwrap_or("-"),
                batch_id = %session.batch_id,
                radio_session_id = session.radio_session_id.as_deref().unwrap_or("-"),
                terminated = session.terminated,
                "wave_feedback_skipped_dead_session"
            );
            return;
        }
        let station_id = session.station_id().to_string();
        // Per-track batch attribution like the original client; fall back to
        // the stored session batch for tracks served before per-track
        // mapping existed. `radioStarted` carries no batch.
        let batch_id = match (track_id.is_some(), batch_id) {
            (false, _) => None,
            (true, Some(batch)) => Some(batch),
            (true, None) => Some(session.batch_id.clone()),
        };
        let from = Some(session.source_id().to_string());

        let api = self.yandex.api.clone();
        let tx = self.tx.clone();
        let generation = self.context_generation.load(Ordering::Acquire);
        let session_id = session.radio_session_id.clone().unwrap_or_default();
        tokio::spawn(async move {
            if let Err(e) = api
                .send_rotor_feedback(
                    station_id.clone(),
                    batch_id.clone(),
                    feedback_type,
                    track_id.clone(),
                    from,
                    total_played,
                )
                .await
            {
                tracing::warn!(
                    error = %e,
                    feedback_type,
                    track_id = track_id.as_deref().unwrap_or("-"),
                    station_id = %station_id,
                    batch_id = batch_id.as_deref().unwrap_or("-"),
                    "wave_feedback_failed"
                );
            } else {
                tracing::info!(feedback_type, "wave_feedback_sent");
                if matches!(feedback_type, "like" | "unlike") {
                    let _ = tx.send(AudioMessage::WaveFeedbackApplied { generation, session_id }).await;
                }
            }
        });
    }

    pub fn send_wave_started(&self) {
        self.send_wave_feedback("radioStarted", None, None, None);
    }

    pub fn send_wave_track_started(&self, track: &Track) {
        let track_id = as_wave_seed(track);
        let batch = self.queue.wave_batch_for_track(track);
        self.send_wave_feedback("trackStarted", Some(track_id), batch, None);
    }

    // Dislikes refresh immediately; likes refresh after acknowledgement.
    fn send_wave_track_with_post(
        &mut self,
        feedback_type: &'static str,
        track: &Track,
        post: WavePostAction,
    ) {
        let (track_id, batch) = system_wave::track_feedback_parts(&self.queue, track);
        self.send_wave_feedback(feedback_type, Some(track_id), batch, None);
        if post == WavePostAction::Refresh {
            self.queue.refresh_wave_queue();
        }
    }

    pub fn send_wave_like(&mut self, track: &Track) {
        // Refresh after the server accepts this preference; keep current playback.
        self.send_wave_track_with_post("like", track, WavePostAction::None);
    }

    pub fn send_wave_unlike(&mut self, track: &Track) {
        self.send_wave_track_with_post("unlike", track, WavePostAction::None);
    }

    pub fn send_wave_dislike(&mut self, track: &Track) {
        self.send_wave_track_with_post("dislike", track, WavePostAction::Refresh);
    }

    pub async fn send_wave_dislike_skip(&mut self, track: &Track) {
        self.send_wave_track_with_post("dislike", track, WavePostAction::Refresh);

        self.play_next().await;
    }

    pub fn send_wave_undislike(&mut self, track: &Track) {
        self.send_wave_track_with_post("undislike", track, WavePostAction::Refresh);
    }

    async fn sync_dislikes(
        api: &ApiService,
        state: &RwLock<SystemState>,
        signals: &AudioSignals,
        db: &tokio::sync::Mutex<crate::db::AppDatabase>,
    ) {
        match api.fetch_disliked_ids().await {
            Ok(ids) => {
                let changed = {
                    let mut state = state.write().await;
                    let before = state.liked.snapshot().1;
                    state.liked.replace_disliked_ids(&ids);
                    before != state.liked.snapshot().1
                };
                let _ = db.lock().await.save_setting("disliked_track_ids", &ids).await;
                if changed {
                    signals.library_changed.send_replace(());
                    signals.changed.send_replace(());
                }
            }
            Err(error) => tracing::warn!("Disliked collection sync failed: {error:?}"),
        }
    }

    pub async fn sync_liked_collection_with(
        api: Arc<ApiService>,
        state: Arc<RwLock<SystemState>>,
        signals: AudioSignals,
        db_arc: Arc<tokio::sync::Mutex<crate::db::AppDatabase>>,
    ) {
        let _mutation = api.like_mutation_lock.lock().await;
        // Revision-gated sync (official-client style): when our revision is
        // current the server answers `likedTracks: {info}` with no `values`
        // → skip DB + rebuild entirely.
        let revision = state.read().await.liked.revision;
        match api.sync_liked_tracks_collection(revision).await {
            Ok(outcome) => {
                let (new_revision, count, ids) = match outcome {
                    LikedTracksSync::Unchanged => {
                        Self::sync_dislikes(&api, &state, &signals, &db_arc).await;
                        tracing::debug!("Liked tracks unchanged (revision {:?})", revision);
                        return;
                    }
                    LikedTracksSync::Changed {
                        revision: new_revision,
                        liked_track_ids,
                        disliked_track_ids,
                    } => {
                        // Belt and suspenders (the server should send no
                        // `values` instead when unchanged).
                        if Some(new_revision) == revision {
                            Self::sync_dislikes(&api, &state, &signals, &db_arc).await;
                            return;
                        }
                        let count = liked_track_ids.len();
                        {
                            let mut state = state.write().await;
                            state.liked.apply_liked_tracks(
                                new_revision,
                                liked_track_ids,
                                disliked_track_ids,
                            );
                        }
                        let ids = state.read().await.liked.ordered_snapshot().0;
                        (new_revision, count, ids)
                    }
                };

                let mut db = db_arc.lock().await;
                let dislikes: Vec<String> = state.read().await.liked.snapshot().1.into_iter().collect();
                let _ = db.save_setting("disliked_track_ids", &dislikes).await;
                let _ = db.save_liked_tracks(&ids).await;
                let _ = db
                    .save_setting("liked_tracks_revision", &new_revision)
                    .await;
                drop(db);

                signals.library_changed.send_replace(());
                signals.changed.send_replace(());

                tracing::info!(
                    "Synced {} liked track IDs (revision {:?} -> {})",
                    count,
                    revision,
                    new_revision
                );
            }
            Err(e) => {
                tracing::warn!("Liked collection sync failed: {:?}", e);
                Self::sync_dislikes(&api, &state, &signals, &db_arc).await;
                // Legacy fallback (servers without collection/sync): full pull.
                // No revision comes back, so the next sync retries the gated path.
                match api.fetch_liked_ids().await {
                    Ok(ids) => {
                        let count = ids.len();
                        {
                            let mut state = state.write().await;
                            state.liked.set_liked_ids(ids);
                        }
                        let liked_ids = state.read().await.liked.ordered_snapshot().0;
                        let mut db = db_arc.lock().await;
                        let _ = db.save_liked_tracks(&liked_ids).await;
                        drop(db);
                        signals.library_changed.send_replace(());
                        tracing::info!("Synced {} liked track IDs (fallback full pull)", count);
                    }
                    Err(e) => {
                        tracing::warn!("Liked fallback pull failed, keeping cache: {:?}", e);
                    }
                }
            }
        }
    }
}

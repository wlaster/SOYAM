use crate::audio::queue::PlaybackContext;
use im::Vector;
use std::time::Duration;
use yandex_music::model::track::Track;

#[derive(Debug, Clone)]
pub enum AudioMessage {
    ContextFetched {
        generation: u64,
        result: Result<(PlaybackContext, Vector<Track>, usize), String>,
    },
    TrackWaveFetched {
        generation: u64,
        previous_seeds: Vec<String>,
        seeds: Vec<String>,
        result: Result<(PlaybackContext, Vector<Track>, usize), String>,
    },
    // Basic playback
    PlayPause,
    Pause,
    Resume,
    Stop,
    Next,
    Prev,
    /// Unconditional previous-track skip: unlike `Prev`, it does not restart
    /// the current track when the playhead is past the restart threshold.
    PreviousTrack,
    Seek(Duration),
    SetVolume(u8),
    /// Applies a transient gain on top of the user volume without changing
    /// the volume signal or its persisted setting (e.g. Android ducking).
    SetTransientVolumeGain(u8),
    ToggleMute,

    // Queue & Context
    PlayTrack(Track),
    PlayTrackList(Vec<String>, String),
    /// Restores a persisted session: the track, where it left off, and whether it
    /// was playing. The play state has to ride along with the track — resuming with
    /// a separate `Resume` message races the spawned playback task, which applies
    /// its own start-paused state afterwards and would pause it right back.
    RestoreTrack(Track, Duration, bool),
    LoadContext(PlaybackContext, Vector<Track>, usize),
    LoadTracks(Vec<Track>),
    QueueTrack(Track),
    PlayTrackNext(Track),
    RemoveFromQueue(usize),
    ClearQueue,
    ToggleShuffle,
    ToggleRepeatMode,

    // Yandex specific (will be handled by YandexProvider)
    PlayPlaylist(u32),
    PlayAlbum(u32),
    PlayArtist(String),
    PlayAlbumTrack(u32, String),
    PlayPlaylistTrack(u32, String),
    PlayLikedTrack(String),
    StartWave(Vec<String>),
    RefreshWavePreferences,
    WavePreferencesFetched {
        generation: u64,
        preference_revision: u64,
        result: Result<yandex_music::model::rotor::session::Session, String>,
    },
    SyncLiked,
    WaveFeedbackApplied { generation: u64, session_id: String },
    WaveLike(String),
    WaveUnlike(String),
    WaveDislike(String),
    WaveUndislike(String),

    // Device
    SetAudioDevice(Option<String>),

    // Internal/Other
    ReloadCurrentTrack,
    TrackMetadataResolved(Track),
    TrackEnded,
    RecreateStream,
}

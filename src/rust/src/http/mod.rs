mod recommendations;
use crate::api::models::AudioQuality;
use chrono::Utc;
use parking_lot::RwLock;
use serde_json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use yandex_music::{
    DEFAULT_CLIENT_ID,
    YandexMusicClient,
    // ... (rest of imports remains similar, but I need to provide full block)
    api::{
        album::{
            add_liked_album::AddLikedAlbumOptions, get_album::GetAlbumOptions,
            remove_liked_album::RemoveLikedAlbumOptions,
        },
        artist::{
            add_disliked_artist::AddDislikedArtistOptions, add_liked_artist::AddLikedArtistOptions,
            get_artist_albums::GetArtistAlbumsOptions,
            get_artist_tracks::ArtistTracksOptions,
            remove_disliked_artist::RemoveDislikedArtistOptions,
            remove_liked_artist::RemoveLikedArtistOptions,
        },
        playlist::{
            add_liked_playlist::AddLikedPlaylistOptions,
            change_playlist_visibility::ChangePlaylistVisibilityOptions,
            create_playlist::CreatePlaylistOptions, delete_playlist::DeletePlaylistOptions,
            get_all_playlists::GetAllPlaylistsOptions, get_playlists::GetPlaylistsOptions,
            modify_playlist::ModifyPlaylistOptions,
            remove_liked_playlist::RemoveLikedPlaylistOptions,
            rename_playlist::RenamePlaylistOptions,
        },
        rotor::{
            get_session_tracks::GetSessionTracksOptions,
            send_station_feedback::SendStationFeedbackOptions,
        },
        search::get_search::SearchOptions,
        track::{
            add_disliked_tracks::AddDislikedTracksOptions, add_liked_tracks::AddLikedTracksOptions,
            get_file_info::GetFileInfoOptions, get_file_info_batch::GetFileInfoBatchOptions,
            get_lyrics::GetLyricsOptions, get_tracks::GetTracksOptions,
            remove_disliked_tracks::RemoveDislikedTracksOptions,
            remove_liked_tracks::RemoveLikedTracksOptions,
        },
    },
    model::{
        album::Album,
        artist::Artist,
        info::{
            file_info::{Codec, Quality, TrackFileInfo},
            lyrics::LyricsFormat,
            pager::Pager,
        },
        playlist::{
            Playlist,
            modify::{Diff, DiffOp},
        },
        rotor::{
            Rotor,
            feedback::{StationFeedback, StationFeedbackEvent},
            session::Session,
        },
        search::Search,
        track::{Track, TrackShort},
    },
};

// `get-file-info`/`get-file-info/batch` sit directly on the playback critical path (called from
// StreamManager::create_stream_session before any audio can start), and the 15s buffering
// watchdog in AudioController silently pauses playback if that path stalls. Without its own
// timeout this request could otherwise hang far longer than 15s, so it errors out well before
// the watchdog would kick in, giving a real `Event::Error` instead of an unexplained pause.
const FILE_INFO_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_ATTEMPTS: usize = 3;

fn retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 500..=504)
}

/// True if the error chain carries HTTP 412 Precondition Failed.
///
/// `yandex-music 0.7.0` surfaces a non-2xx from `change-relative` as
/// `ClientError::YandexMusicError { message: "Request failed with status code: 412 ..." }`
/// (see `client/request.rs::send_request`), and transport-level failures as
/// `ClientError::RequestError { reqwest::Error }`. Our API surface boxes everything
/// into `Box<dyn Error>`, so match structurally where possible and fall back to
/// substring search on the whole cause chain.
fn is_precondition_failed(err: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        if let Some(client_err) = e.downcast_ref::<yandex_music::error::ClientError>() {
            if let yandex_music::error::ClientError::RequestError { error } = client_err {
                if error.status().is_some_and(|s| s.as_u16() == 412) {
                    return true;
                }
            }
        }
        if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
            if req_err.status().is_some_and(|s| s.as_u16() == 412) {
                return true;
            }
        }
        let msg = e.to_string();
        if msg.contains("status code: 412") || msg.contains("412 Precondition") {
            return true;
        }
        cur = e.source();
    }
    false
}

/// Execute a raw request with the same bounded retry policy as the desktop client.
/// The reqwest client timeout provides cancellation of the in-flight operation.
pub async fn send_with_retry<F>(mut make_request: F) -> Result<reqwest::Response>
where
    F: FnMut() -> reqwest::RequestBuilder,
{
    let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for attempt in 0..MAX_REQUEST_ATTEMPTS {
        match make_request().send().await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) if retryable_status(response.status()) => {
                last_error = Some(format!("HTTP {}", response.status()).into());
            }
            Ok(response) => return Err(format!("HTTP {}", response.status()).into()),
            Err(error) => last_error = Some(error.into()),
        }
        if attempt + 1 < MAX_REQUEST_ATTEMPTS {
            sleep(Duration::from_millis(200 * (1 << attempt))).await;
        }
    }
    Err(last_error.unwrap_or_else(|| "request failed".into()))
}

pub trait SessionExt {
    fn station_id(&self) -> &str;
    fn source_id(&self) -> &str;
}

impl SessionExt for Session {
    fn station_id(&self) -> &str {
        self.radio_session_id
            .as_deref()
            .or(self.wave.as_ref().map(|w| w.station_id.as_str()))
            .unwrap_or("user:onyourwave")
    }

    fn source_id(&self) -> &str {
        self.wave
            .as_ref()
            .map(|w| w.id_for_from.as_str())
            .unwrap_or("rotor")
    }
}

pub struct ApiService {
    pub client: Arc<YandexMusicClient>,
    /// Separate client for `get-file-info`/`get-file-info/batch`: the crate's
    /// bundled sign key is only valid for its own default client identity
    /// (`DEFAULT_CLIENT_ID`), which doesn't match the desktop identity `client`
    /// above uses for everything else — signing with one and identifying as
    /// the other gets the request rejected with 403.
    file_info_client: Arc<YandexMusicClient>,
    pub http_client: reqwest::Client,
    user_id: u64,
    pub quality: RwLock<AudioQuality>,
    pub(crate) like_mutation_lock: tokio::sync::Mutex<()>,
    ai_content_reduction: RwLock<Option<bool>>,
    ai_settings_lock: tokio::sync::Mutex<()>,
    ai_revision: std::sync::atomic::AtomicU64,
}

#[derive(Debug)]
pub enum LikedTracksSync {
    /// Server confirmed our stored revision (section has no `values`):
    /// nothing to apply, keep the cached snapshot unchanged.
    Unchanged,
    Changed {
        revision: u64,
        /// Base track ids (strings in the API payload, e.g. `"156189026"`).
        liked_track_ids: Vec<String>,
        disliked_track_ids: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub struct TrackStreamInfo {
    pub url: String,
    pub mirror_urls: Vec<String>,
    pub codec: String,
}

impl ApiService {
    pub async fn new(token: String, user_id: Option<u64>) -> Result<Self> {
        let quality = RwLock::new(AudioQuality::default());

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {}", token))?,
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_str("YandexMusicDesktopAppWindows/5.110.1")?,
        );
        headers.insert("Accept-Language", HeaderValue::from_str("ru")?);
        headers.insert("Accept", HeaderValue::from_str("*/*")?);
        // Deliberately NOT setting X-Yandex-Music-Without-Invocation-Info here:
        // it strips `invocationInfo`/`exec-duration-millis` from every
        // response, which breaks the crate's strict typed deserialization
        // for account/playlists/etc.
        headers.insert(
            "Origin",
            HeaderValue::from_str("music-application://desktop")?,
        );

        let http_client = crate::util::tls::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 YandexMusic/5.110.1")
            .default_headers(headers.clone())
            .brotli(true)
            // Keep playlist/context requests from blocking playback indefinitely.
            .timeout(FILE_INFO_REQUEST_TIMEOUT)
            .build()?;

        let client = Arc::new(YandexMusicClient::from_client(http_client.clone()));

        // Built via `custom_client` (with headers replicated by hand) instead of the plain
        // `YandexMusicClient::builder(token).build()`, purely to attach a request timeout —
        // `ClientBuilder` has no `.timeout()` of its own.
        let mut file_info_headers = HeaderMap::with_capacity(2);
        file_info_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {}", token))?,
        );
        file_info_headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_str(DEFAULT_CLIENT_ID)?,
        );
        let file_info_http_client = crate::util::tls::builder()
            .default_headers(file_info_headers)
            .timeout(FILE_INFO_REQUEST_TIMEOUT)
            .build()?;
        let file_info_client = Arc::new(
            YandexMusicClient::builder(token)
                .custom_client(file_info_http_client)
                .build()?,
        );

        let user_id = if let Some(uid) = user_id {
            uid
        } else {
            client
                .get_account_status()
                .await?
                .account
                .uid
                .ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from("No user id found")
                })?
        };

        Ok(Self {
            client,
            file_info_client,
            http_client,
            user_id,
            quality,
            like_mutation_lock: tokio::sync::Mutex::new(()),
            ai_content_reduction: RwLock::new(None),
            ai_settings_lock: tokio::sync::Mutex::new(()),
            ai_revision: std::sync::atomic::AtomicU64::new(0),
        })
    }

    pub fn current_user_id(&self) -> u64 {
        self.user_id
    }

    pub async fn fetch_ugc_upload_info(&self, kind: u32, name: &str) -> Result<String> {
        let url = format!(
            "https://api.music.yandex.ru/loader/upload-url?uid={0}&playlist-id={0}:{1}&path={2}",
            self.user_id,
            kind,
            urlencoding::encode(name)
        );
        Ok(send_with_retry(|| self.http_client.post(&url))
            .await?
            .json::<serde_json::Value>()
            .await?["post-target"]
            .as_str()
            .ok_or("No target")?
            .to_string())
    }

    pub async fn upload_ugc_track(&self, url: &str, path: &str) -> Result<String> {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("t.mp3");
        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(tokio::fs::read(path).await?)
                .file_name(name.to_string())
                .mime_str("audio/mpeg")?,
        );
        let v = self
            .http_client
            .post(url)
            .multipart(form)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let id = v["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| {
            if v["result"] == "CREATED" {
                "CREATED_PENDING".into()
            } else {
                v.to_string()
            }
        });
        Ok(id)
    }

    pub fn set_quality(&self, quality: AudioQuality) {
        *self.quality.write() = quality;
    }

    pub fn get_quality(&self) -> AudioQuality {
        *self.quality.read()
    }

    pub async fn search(&self, query: &str) -> Result<Search> {
        let opts = SearchOptions::new(query);
        Ok(self.client.search(&opts).await?)
    }

    pub async fn fetch_liked_tracks(&self) -> Result<Playlist> {
        self.fetch_first(
            self.client.get_playlists(
                &GetPlaylistsOptions::new(self.user_id)
                    .kinds([3u32])
                    .with_tracks(true),
            ),
        )
        .await
    }

    pub async fn fetch_all_playlists(&self) -> Result<Vec<Playlist>> {
        let opts = GetAllPlaylistsOptions::new(self.user_id);
        Ok(self.client.get_all_playlists(&opts).await?)
    }

    pub async fn fetch_liked_albums(&self) -> Result<Vec<Album>> {
        let url = format!(
            "https://api.music.yandex.ru/users/{}/likes/albums?rich=true",
            self.user_id
        );
        let body: serde_json::Value = send_with_retry(|| self.http_client.get(&url))
            .await?
            .json()
            .await?;
        Ok(Self::extract_liked(&body, "albums", "album"))
    }

    pub async fn fetch_liked_artists(&self) -> Result<Vec<Artist>> {
        let url = format!(
            "https://api.music.yandex.ru/users/{}/likes/artists?with-timestamps=false",
            self.user_id
        );
        let body: serde_json::Value = send_with_retry(|| self.http_client.get(&url))
            .await?
            .json()
            .await?;
        Ok(Self::extract_liked(&body, "artists", "artist"))
    }

    /// Parses Yandex "likes" responses. The payload may arrive as a top-level array,
    /// `result: [..]`, `result: { <plural>: [..] }`, or `result: { likes: [..] }`.
    /// Each entry may be the object itself or wrapped as `{ id, timestamp, <singular>: {..} }`.
    fn extract_liked<T: serde::de::DeserializeOwned>(
        body: &serde_json::Value,
        plural_key: &str,
        singular_key: &str,
    ) -> Vec<T> {
        if let Some(items) = Self::liked_items(body, plural_key) {
            items
                .iter()
                .filter_map(|item| {
                    let value = if item.get(singular_key).is_some() {
                        &item[singular_key]
                    } else {
                        item
                    };
                    serde_json::from_value::<T>(value.clone()).ok()
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn liked_items<'a>(
        body: &'a serde_json::Value,
        plural_key: &str,
    ) -> Option<&'a Vec<serde_json::Value>> {
        let result = &body["result"];
        body.as_array()
            .or_else(|| result.as_array())
            .or_else(|| result[plural_key].as_array())
            .or_else(|| result["likes"].as_array())
            .or_else(|| result["library"][plural_key].as_array())
    }

    pub async fn fetch_playlist(&self, kind: u32) -> Result<Playlist> {
        self.fetch_first(
            self.client.get_playlists(
                &GetPlaylistsOptions::new(self.user_id)
                    .kinds([kind])
                    .with_tracks(true),
            ),
        )
        .await
    }

    pub async fn fetch_playlist_bare(&self, kind: u32) -> Result<Playlist> {
        self.fetch_first(
            self.client.get_playlists(
                &GetPlaylistsOptions::new(self.user_id)
                    .kinds([kind])
                    .with_tracks(true)
                    .rich_tracks(false),
            ),
        )
        .await
    }

    pub async fn fetch_playlists(&self, kinds: Vec<u32>) -> Result<Playlist> {
        self.fetch_first(
            self.client.get_playlists(
                &GetPlaylistsOptions::new(self.user_id)
                    .kinds(kinds)
                    .with_tracks(true),
            ),
        )
        .await
    }

    async fn fetch_first<T, Fut, E>(&self, fut: Fut) -> Result<T>
    where
        Fut: std::future::Future<Output = std::result::Result<Vec<T>, E>>,
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        fut.await
            .map_err(|e| e.into())?
            .into_iter()
            .next()
            .ok_or_else(|| "Not found".into())
    }

    pub async fn fetch_tracks(&self, track_ids: Vec<String>) -> Result<Vec<Track>> {
        let opts = GetTracksOptions::new(track_ids);
        Ok(self.client.get_tracks(&opts).await?)
    }

    pub async fn fetch_file_info_batch(
        &self,
        track_ids: Vec<String>,
        quality: Quality,
    ) -> Result<Vec<TrackFileInfo>> {
        let opts = GetFileInfoBatchOptions::new(track_ids).quality(quality);
        let mut last_error = None;
        for attempt in 0..MAX_REQUEST_ATTEMPTS {
            match self.file_info_client.get_file_info_batch(&opts).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    last_error = Some(error);
                    if attempt + 1 < MAX_REQUEST_ATTEMPTS {
                        sleep(Duration::from_millis(200 * (1 << attempt))).await;
                    }
                }
            }
        }
        Err(last_error.expect("file info attempts are non-empty").into())
    }

    fn map_quality(quality: AudioQuality) -> Quality {
        match quality {
            AudioQuality::High => Quality::Lossless,
            AudioQuality::Normal => Quality::Normal,
            AudioQuality::Low => Quality::Low,
        }
    }

    fn codec_by_name(name: &str) -> Option<Codec> {
        Codec::all().into_iter().find(|c| c.to_string() == name)
    }

    async fn fetch_track_url_with_codec(
        &self,
        track_id: String,
        quality: AudioQuality,
        preferred_codec: Option<&str>,
    ) -> Result<(String, String)> {
        let mut opts = GetFileInfoOptions::new(track_id.clone()).quality(Self::map_quality(quality));
        if let Some(codec) = preferred_codec.and_then(Self::codec_by_name) {
            opts = opts.codecs([codec]);
        }

        let mut last_error = None;
        let mut info = None;
        for attempt in 0..MAX_REQUEST_ATTEMPTS {
            match self.file_info_client.get_file_info(&opts).await {
                Ok(value) => {
                    info = Some(value);
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt + 1 < MAX_REQUEST_ATTEMPTS {
                        sleep(Duration::from_millis(200 * (1 << attempt))).await;
                    }
                }
            }
        }
        let info = info.ok_or_else(|| {
            last_error
                .expect("file info attempts are non-empty")
                .to_string()
        })?;
        Ok((info.url, info.codec))
    }

    pub async fn fetch_track_stream_info(&self, track_id: String, quality: AudioQuality) -> Result<TrackStreamInfo> {
        let opts = GetFileInfoOptions::new(track_id.clone()).quality(Self::map_quality(quality));
        let mut last_error = None;
        let mut info = None;
        for attempt in 0..MAX_REQUEST_ATTEMPTS {
            match self.file_info_client.get_file_info(&opts).await {
                Ok(value) => {
                    info = Some(value);
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt + 1 < MAX_REQUEST_ATTEMPTS {
                        sleep(Duration::from_millis(200 * (1 << attempt))).await;
                    }
                }
            }
        }
        let info = info.ok_or_else(|| {
            last_error
                .expect("stream info attempts are non-empty")
                .to_string()
        })?;
        let mirror_urls = info
            .urls
            .iter()
            .filter(|u| *u != &info.url)
            .cloned()
            .collect();
        Ok(TrackStreamInfo {
            url: info.url,
            mirror_urls,
            codec: info.codec,
        })
    }

    pub async fn fetch_track_url(&self, track_id: String) -> Result<(String, String)> {
        self.fetch_track_url_with_codec(track_id, self.get_quality(), None)
            .await
    }

    pub async fn fetch_track_url_for_download(&self, track_id: String) -> Result<(String, String)> {
        // Try the same fetch as playback first (favors FLAC for Lossless quality)
        let (url, codec) = match self.fetch_track_url(track_id.clone()).await {
            Ok(info) => info,
            Err(_) => return self.fetch_legacy_mp3_url(track_id).await,
        };

        // If it's already FLAC or MP3, we are happy.
        if codec.contains("flac") || codec.contains("mp3") {
            return Ok((url, codec));
        }

        // If it's AAC, we try to force MP3 instead because raw AAC doesn't support tags well.
        let quality = self.get_quality();
        if let Ok(res) = self
            .fetch_track_url_with_codec(track_id, quality, Some("mp3"))
            .await
        {
            return Ok(res);
        }

        Ok((url, codec))
    }

    pub async fn fetch_legacy_mp3_url(&self, track_id: String) -> Result<(String, String)> {
        use yandex_music::api::track::get_download_info::GetDownloadInfoOptions;
        let mut variants = self.client.get_download_info(&GetDownloadInfoOptions::new(track_id)).await?;
        variants.retain(|info| !info.preview && info.codec == "mp3");
        variants.sort_by_key(|info| std::cmp::Reverse(info.bitrate_in_kbps));
        let mut last_error = None;
        for variant in variants {
            match variant.get_direct_link(&self.http_client).await {
                Ok(url) => return Ok((url, "mp3".into())),
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        Err(last_error.unwrap_or_else(|| "Полная MP3-версия трека недоступна".into()).into())
    }

    pub async fn fetch_track_urls_batch(
        &self,
        track_ids: Vec<String>,
        quality: AudioQuality,
    ) -> Result<Vec<(String, String, Vec<String>, String)>> {

        let infos = self
            .fetch_file_info_batch(track_ids.clone(), Self::map_quality(quality))
            .await?;

        if infos.len() != track_ids.len() {
            return Err(Box::from(format!(
                "get-file-info/batch returned {} results for {} requested tracks",
                infos.len(),
                track_ids.len()
            )));
        }

        Ok(track_ids
            .into_iter()
            .zip(infos)
            .map(|(track_id, info)| {
                let mirror_urls = info
                    .urls
                    .iter()
                    .filter(|url| *url != &info.url)
                    .cloned()
                    .collect();
                (track_id, info.url, mirror_urls, info.codec)
            })
            .collect())
    }

    pub async fn fetch_lyrics(
        &self,
        track_id: String,
        format: LyricsFormat,
    ) -> Result<Option<String>> {
        let opts = GetLyricsOptions::new(track_id, format);
        match self.client.get_lyrics(&opts).await {
            Ok(lyrics) => {
                let url = lyrics.download_url;
                let text = self
                    .client
                    .inner
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                Ok(Some(text))
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn fetch_album_with_tracks(&self, album_id: u32) -> Result<Album> {
        let opts = GetAlbumOptions::new(album_id).with_tracks();
        Ok(self.client.get_album(&opts).await?)
    }

    pub async fn fetch_artist(
        &self,
        artist_id: String,
    ) -> Result<(yandex_music::model::artist::Artist, Option<u32>)> {
        let value: serde_json::Value = self.http_client
            .get(format!("https://api.music.yandex.net/artists/{}/brief-info", urlencoding::encode(&artist_id)))
            .send().await?.error_for_status()?.json().await?;
        let result = &value["result"];
        let listeners = result["stats"]["lastMonthListeners"].as_u64().and_then(|n| u32::try_from(n).ok());
        // The brief response includes partial dates and nested composite artists.
        // Deserialize only fields used by the artist card, avoiding unrelated legacy shapes.
        let mut fields = serde_json::Map::new();
        for key in ["id", "name", "cover", "likesCount", "links", "ratings"] {
            if let Some(value) = result["artist"].get(key) { fields.insert(key.into(), value.clone()); }
        }
        let artist = serde_json::Value::Object(fields);
        Ok((serde_json::from_value(artist)?, listeners))
    }

    pub async fn fetch_artist_albums(
        &self,
        artist_id: String,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Album>> {
        let opts = GetArtistAlbumsOptions::new(artist_id)
            .page(page)
            .page_size(page_size)
            .sort_by(yandex_music::model::artist::SortBy::Year);
        Ok(self.client.get_artist_albums(&opts).await?.albums)
    }

    pub async fn fetch_artist_tracks_paginated(
        &self,
        artist_id: String,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<Track>, Pager)> {
        let opts = ArtistTracksOptions::new(artist_id)
            .page(page)
            .page_size(page_size);
        let result = self.client.get_artist_tracks(&opts).await?;
        Ok((result.tracks, result.pager))
    }

    pub async fn fetch_stations(&self) -> Result<Vec<Rotor>> {
        let opts = yandex_music::api::rotor::get_all_stations::GetAllStationsOptions::default();
        Ok(self.client.get_all_stations(&opts).await?)
    }

    pub async fn get_session_tracks(
        &self,
        session_id: String,
        queue: Vec<String>,
        feedbacks: Vec<StationFeedback>,
    ) -> Result<Session> {
        let opts = GetSessionTracksOptions::new(session_id, queue).feedbacks(feedbacks);
        Ok(self.client.get_session_tracks(opts).await?)
    }

    pub async fn send_rotor_feedback(
        &self,
        station_id: String,
        batch_id: Option<String>,
        feedback_type: &str,
        track_id: Option<String>,
        from: Option<String>,
        total_played: Option<Duration>,
    ) -> Result<()> {
        let event = StationFeedbackEvent {
            item_type: Some(feedback_type.to_string()),
            timestamp: Utc::now(),
            from: None,
            track_id,
            total_played,
            track_length: None,
        };
        let feedback = StationFeedback {
            batch_id,
            event,
            from,
        };

        let opts = SendStationFeedbackOptions::new(station_id, feedback);
        self.client.send_station_feedback(&opts).await?;

        Ok(())
    }

    pub async fn add_like_track(&self, track_id: String) -> Result<()> {
        let opts = AddLikedTracksOptions::new(self.user_id, vec![track_id]);
        // yandex_music crate returns Err if the response JSON has no `revision`
        // (InvalidValue), so `?` here rejects non-OK responses without revision.
        let _revision: u64 = self.client.add_liked_tracks(&opts).await?;
        Ok(())
    }

    pub async fn remove_like_track(&self, track_id: String) -> Result<()> {
        // Yandex can group related versions when removing a like. Snapshot the
        // server list and restore every distinct ID except the requested one.
        let before = self.fetch_liked_ids().await?;
        let opts = RemoveLikedTracksOptions::new(self.user_id, vec![track_id.clone()]);
        let _revision: u64 = self.client.remove_liked_tracks(&opts).await?;
        let after = self.fetch_liked_ids().await?;
        let restore = collateral_unlikes(&before, &after, &track_id);
        if !restore.is_empty() {
            let opts = AddLikedTracksOptions::new(self.user_id, restore);
            self.client.add_liked_tracks(&opts).await?;
        }
        Ok(())
    }

    pub async fn add_dislike_track(&self, track_id: String) -> Result<()> {
        let opts = AddDislikedTracksOptions::new(self.user_id, vec![track_id]);
        // Same revision check as above: Err if no `revision` in response.
        let _revision: u64 = self.client.add_disliked_tracks(&opts).await?;
        Ok(())
    }

    pub async fn remove_dislike_track(&self, track_id: String) -> Result<()> {
        let opts = RemoveDislikedTracksOptions::new(self.user_id, vec![track_id]);
        // Same revision check as above: Err if no `revision` in response.
        let _revision: u64 = self.client.remove_disliked_tracks(&opts).await?;
        Ok(())
    }

    pub async fn add_like_album(&self, album_id: u32) -> Result<()> {
        let opts = AddLikedAlbumOptions::new(self.user_id, album_id);
        self.client.add_liked_album(&opts).await?;
        Ok(())
    }

    pub async fn remove_like_album(&self, album_id: u32) -> Result<()> {
        let opts = RemoveLikedAlbumOptions::new(self.user_id, album_id);
        self.client.remove_liked_album(&opts).await?;
        Ok(())
    }

    pub async fn add_track_to_playlist(
        &self,
        kind: u32,
        track_id: String,
        album_id: String,
    ) -> Result<()> {
        let track = TrackShort {
            id: track_id,
            album_id: Some(album_id),
        };

        let diff = Diff::new(DiffOp::insert(0), vec![track]);
        self.modify_playlist_with_revision_retry(kind, diff).await?;
        Ok(())
    }

    pub async fn remove_track_from_playlist(
        &self,
        kind: u32,
        track_id: String,
        album_id: String,
    ) -> Result<()> {
        let track = TrackShort {
            id: track_id,
            album_id: Some(album_id),
        };

        let diff = Diff::new(DiffOp::delete(0, 1), vec![track]);
        self.modify_playlist_with_revision_retry(kind, diff).await?;
        Ok(())
    }

    pub async fn move_track_in_playlist(
        &self,
        kind: u32,
        from_index: usize,
        to_index: usize,
        track_id: String,
        album_id: String,
    ) -> Result<()> {
        let track = TrackShort {
            id: track_id,
            album_id: Some(album_id),
        };

        // NOTE: yandex-music 0.7.0 DiffOp supports only Insert/Delete (no atomic
        // move/reorder op), so a move stays a delete+insert pair. Each step goes
        // through with_revision_retry (one refetch + one retry max per step);
        // the second step fetches a fresh revision after the first completes.
        let diff_delete = Diff::new(
            DiffOp::delete(from_index, from_index + 1),
            vec![track.clone()],
        );
        let diff_insert = Diff::new(DiffOp::insert(to_index), vec![track]);

        self.modify_playlist_with_revision_retry(kind, diff_delete)
            .await?;
        self.modify_playlist_with_revision_retry(kind, diff_insert)
            .await?;

        Ok(())
    }

    /// Run one `change-relative` diff against the current revision; on HTTP 412
    /// Precondition Failed do a single `fetch_playlist_bare` refetch and one retry.
    /// A second 412 (or any other error) is returned to the caller (RELOAD semantics
    /// are decided upstream). At most 1 retry per call.
    async fn modify_playlist_with_revision_retry(&self, kind: u32, diff: Diff) -> Result<Playlist> {
        let playlist = self.fetch_playlist_bare(kind).await?;
        let opts = ModifyPlaylistOptions::new(self.user_id, kind, diff.clone(), playlist.revision);
        match self.client.modify_playlist(&opts).await {
            Ok(updated) => Ok(updated),
            Err(e) if is_precondition_failed(&e) => {
                let fresh = self.fetch_playlist_bare(kind).await?;
                let retry_opts =
                    ModifyPlaylistOptions::new(self.user_id, kind, diff, fresh.revision);
                Ok(self.client.modify_playlist(&retry_opts).await?)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub async fn create_playlist(&self, title: String, is_public: bool) -> Result<()> {
        let visibility = if is_public { "public" } else { "private" };
        let opts = CreatePlaylistOptions::new(self.user_id, title, visibility);
        self.client.create_playlist(&opts).await?;
        Ok(())
    }

    pub async fn delete_playlist(&self, kind: u32) -> Result<()> {
        let opts = DeletePlaylistOptions::new(self.user_id, kind);
        self.client.delete_playlist(&opts).await?;
        Ok(())
    }

    pub async fn rename_playlist(&self, kind: u32, new_title: String) -> Result<()> {
        let opts = RenamePlaylistOptions::new(self.user_id, kind, new_title);
        self.client.rename_playlist(&opts).await?;
        Ok(())
    }

    pub async fn change_playlist_visibility(&self, kind: u32, is_public: bool) -> Result<()> {
        let visibility = if is_public { "public" } else { "private" };
        let opts = ChangePlaylistVisibilityOptions::new(self.user_id, kind, visibility);
        self.client.change_playlist_visibility(&opts).await?;
        Ok(())
    }

    pub async fn add_like_artist(&self, artist_id: String) -> Result<()> {
        let opts = AddLikedArtistOptions::new(self.user_id, artist_id);
        self.client.add_liked_artist(&opts).await?;
        Ok(())
    }

    pub async fn remove_like_artist(&self, artist_id: String) -> Result<()> {
        let opts = RemoveLikedArtistOptions::new(self.user_id, artist_id);
        self.client.remove_liked_artist(&opts).await?;
        Ok(())
    }

    pub async fn add_dislike_artist(&self, artist_id: String) -> Result<()> {
        let opts = AddDislikedArtistOptions::new(self.user_id, artist_id);
        self.client.add_disliked_artist(&opts).await?;
        Ok(())
    }

    pub async fn remove_dislike_artist(&self, artist_id: String) -> Result<()> {
        let opts = RemoveDislikedArtistOptions::new(self.user_id, artist_id);
        self.client.remove_disliked_artist(&opts).await?;
        Ok(())
    }

    pub async fn add_like_playlist(&self, owner_uid: u64, kind: u32) -> Result<()> {
        let opts = AddLikedPlaylistOptions::new(self.user_id, owner_uid, kind);
        self.client.add_liked_playlist(&opts).await?;
        Ok(())
    }

    pub async fn remove_like_playlist(&self, owner_uid: u64, kind: u32) -> Result<()> {
        let opts = RemoveLikedPlaylistOptions::new(self.user_id, owner_uid, kind);
        self.client.remove_liked_playlist(&opts).await?;
        Ok(())
    }

    pub async fn get_account_info(&self) -> Result<crate::api::models::UserAccountDto> {
        let status = self.client.get_account_status().await?;

        let mut avatar_url = None;
        if let Ok(resp) = self
            .client
            .inner
            .get("https://api.music.yandex.net/account/about")
            .send()
            .await
            && let Ok(json) = resp.json::<serde_json::Value>().await
            && let Some(id) = json["result"]["avatarId"].as_str()
        {
            avatar_url = Some(format!(
                "https://avatars.mds.yandex.net/get-yapic/{}/islands-200",
                id
            ));
        }

        Ok(crate::api::models::UserAccountDto::from_yandex(
            status, avatar_url,
        ))
    }

    pub async fn fetch_liked_ids(&self) -> Result<Vec<String>> {
        let opts =
            yandex_music::api::track::get_liked_tracks::GetLikedTracksOptions::new(self.user_id);
        let library = self.client.get_liked_tracks(&opts).await?;

        Ok(library.tracks.into_iter().map(|t| t.id).collect())
    }

    /// Revision-gated collection sync for liked tracks (mirrors the official
    /// desktop client): with a known `revision` the server answers
    /// `likedTracks: {info: {...}}` with **no `values`** when nothing changed
    /// (verified against the real API); the official client guards on exactly
    /// that shape. The bundled `yandex-music` crate instead requires `values`
    /// in every present section, so this parses the response here and reports
    /// `Unchanged` itself. Missing storage revision (`None`) sends
    /// `allValuesRequired: true` for the full snapshot.
    pub async fn sync_liked_tracks_collection(
        &self,
        revision: Option<u64>,
    ) -> Result<LikedTracksSync> {
        let option = match revision {
            Some(rev) => serde_json::json!({ "revision": rev }),
            None => serde_json::json!({ "allValuesRequired": true }),
        };
        let body = serde_json::json!({ "likedTracks": option });
        let response: serde_json::Value = send_with_retry(|| {
            self.client
                .inner
                .post("https://api.music.yandex.net/collection/sync")
                .json(&body)
        })
        .await?
        .json()
        .await?;

        let section = &response["result"]["values"]["likedTracks"];
        let revision = section["info"]["revision"].as_u64();

        // Current revision: section is present but has no `values` key.
        let Some(values) = section.get("values") else {
            return Ok(LikedTracksSync::Unchanged);
        };
        let revision =
            revision.ok_or("likedTracks section carries no info.revision despite `values`")?;

        let parse_ids = |key: &str| -> Vec<String> {
            values[key]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| match &row["trackId"] {
                            serde_json::Value::String(id) => Some(id.clone()),
                            serde_json::Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(LikedTracksSync::Changed {
            revision,
            liked_track_ids: parse_ids("liked"),
            disliked_track_ids: parse_ids("disliked"),
        })
    }
}

fn collateral_unlikes(before: &[String], after: &[String], removed: &str) -> Vec<String> {
    use crate::util::track::CleanId;
    let after: std::collections::HashSet<&str> = after.iter().map(|id| id.to_base_id()).collect();
    before.iter().filter(|id| id.to_base_id() != removed.to_base_id()
        && !after.contains(id.to_base_id())).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unlike_restores_other_versions_but_never_requested_track() {
        let before = vec!["154773901:43778408".into(), "155352126:43778408".into(), "other".into()];
        let after = vec!["other".into()];
        assert_eq!(collateral_unlikes(&before, &after, "154773901:43778408"), vec!["155352126:43778408"]);
        assert!(collateral_unlikes(&before, &before[1..], "154773901").is_empty());
    }

    #[test]
    fn extracts_wrapped_albums_from_top_level_array() {
        let body = json!([
            {
                "timestamp": "2026-05-26T00:00:00+00:00",
                "album": {
                    "id": 10,
                    "title": "Album title"
                }
            }
        ]);

        let albums: Vec<Album> = ApiService::extract_liked(&body, "albums", "album");

        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].id, Some(10));
        assert_eq!(albums[0].title.as_deref(), Some("Album title"));
    }

    #[test]
    fn extracts_direct_artists_from_top_level_array() {
        let body = json!([
            {
                "id": "20",
                "name": "Artist name"
            }
        ]);

        let artists: Vec<Artist> = ApiService::extract_liked(&body, "artists", "artist");

        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].id.as_deref(), Some("20"));
        assert_eq!(artists[0].name.as_deref(), Some("Artist name"));
    }

    #[test]
    fn extracts_direct_albums_from_library_shape() {
        let body = json!({
            "result": {
                "library": {
                    "albums": [
                        {
                            "id": 30,
                            "title": "Library album"
                        }
                    ]
                }
            }
        });

        let albums: Vec<Album> = ApiService::extract_liked(&body, "albums", "album");

        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].id, Some(30));
        assert_eq!(albums[0].title.as_deref(), Some("Library album"));
    }
}

#[cfg(test)]
mod live_quality_tests {
    use super::*;
    #[tokio::test]
    #[ignore = "uses saved local account and internet"]
    async fn live_quality_and_yandex_audience() {
        let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("darkplayoff.yayma.app/yayma/yamusic_v2.db");
        let db = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let raw: String = db.query_row("SELECT value FROM app_settings WHERE key='auth_token'", [], |row| row.get(0)).unwrap();
        let (token, uid): (String, u64) = serde_json::from_str(&raw).unwrap();
        let track: String = db.query_row("SELECT track_id FROM track_metadata LIMIT 1", [], |row| row.get(0)).unwrap();
        drop(db);
        let api = ApiService::new(token, Some(uid)).await.unwrap();
        for quality in [AudioQuality::Low, AudioQuality::Normal, AudioQuality::High] {
            let opts = GetFileInfoOptions::new(track.clone()).quality(ApiService::map_quality(quality));
            let info = api.file_info_client.get_file_info(&opts).await.unwrap();
            println!("QUALITY {:?}: actual={} codec={} bitrate={} size={:?}", quality, info.quality, info.codec, info.bitrate, info.size);
            assert!(!info.url.is_empty());
        }
        let (artist, audience) = api.fetch_artist(std::env::var("ARTIST_STATS_TEST_ID").unwrap_or_else(|_| "675068".into())).await.unwrap();
        println!("YANDEX audience={:?}; chart position={:?}", audience, artist.ratings.map(|r| r.month));
        assert!(audience.is_some());
    }
}


#[cfg(test)]
mod popular_high_quality_diagnostics {
    use super::*;

    fn normalized(value: &str) -> String {
        value.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
    }

    #[tokio::test]
    #[ignore = "real network, saved local authorization; no UI or playback"]
    async fn live_current_high_quality_popular_tracks() {
        let db_path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("darkplayoff.yayma.app/yayma/yamusic_v2.db");
        let db = rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let raw: String = db.query_row("SELECT value FROM app_settings WHERE key='auth_token'", [], |row| row.get(0)).unwrap();
        let (token, uid): (String, u64) = serde_json::from_str(&raw).unwrap();
        drop(db);
        let api = ApiService::new(token, Some(uid)).await.unwrap();
        api.set_quality(AudioQuality::High);
        let media_client = crate::util::tls::builder().timeout(Duration::from_secs(20)).build().unwrap();
        let root = std::env::current_dir().unwrap();
        let report_path = root.join("../../docs/high-quality-current-check.json");
        let sample_dir = root.join("../../../tools/high-quality-current-samples");
        std::fs::create_dir_all(&sample_dir).unwrap();
        let songs = [
            ("Imagine Dragons", "Believer"),
            ("The Weeknd", "Blinding Lights"),
            ("Dua Lipa", "Levitating"),
            ("Billie Eilish", "bad guy"),
            ("Ed Sheeran", "Shape of You"),
            ("Linkin Park", "In the End"),
            ("Кино", "Группа крови"),
            ("Король и Шут", "Кукла колдуна"),
        ];
        let mut results = Vec::new();
        for (artist, title) in songs {
            let query = format!("{artist} {title}");
            let result: std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> = async {
                let data: serde_json::Value = api.http_client.get("https://api.music.yandex.net/search")
                    .query(&[("text", query.as_str()), ("type", "track"), ("page", "0")])
                    .send().await?.error_for_status()?.json().await?;
                let tracks = data["result"]["tracks"]["results"].as_array().ok_or("search has no tracks")?;
                let wanted_title = normalized(title);
                let wanted_artist = normalized(artist);
                let track = tracks.iter().find(|track| {
                    track["title"].as_str().is_some_and(|t| normalized(t) == wanted_title)
                        && track["artists"].as_array().is_some_and(|artists| artists.iter().any(|a| a["name"].as_str().is_some_and(|n| normalized(n) == wanted_artist)))
                        && track["available"].as_bool() != Some(false)
                }).ok_or("no exact original-artist/title match")?;
                let id = track["id"].as_str().map(str::to_string).unwrap_or_else(|| track["id"].to_string());
                // This is the actual production entry point used by StreamManager.
                let stream = api.fetch_track_stream_info(id.clone(), api.get_quality()).await?;
                let metadata = api.file_info_client.get_file_info(
                    &GetFileInfoOptions::new(id.clone()).quality(ApiService::map_quality(api.get_quality()))
                ).await?;
                let mut row = serde_json::json!({
                    "artist": artist, "title": title, "track_id": id,
                    "version": track["version"], "requested_mode": "High", "requested_quality": "lossless",
                    "production_codec": stream.codec, "api_codec": metadata.codec,
                    "api_quality": metadata.quality, "api_kbps": metadata.bitrate,
                    "production_url_received": !stream.url.is_empty(),
                    "codec_matches_metadata_request": stream.codec == metadata.codec,
                    "mirror_count": stream.mirror_urls.len()
                });
                let download: std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> = async {
                    let mut response = media_client.get(&stream.url).header("Range", "bytes=0-524287")
                        .send().await?.error_for_status()?;
                    let mut prefix = Vec::new();
                    while let Some(chunk) = response.chunk().await? {
                        let remaining = 524288usize.saturating_sub(prefix.len());
                        prefix.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                        if prefix.len() >= 524288 { break; }
                    }
                    Ok(prefix)
                }.await;
                match download {
                    Ok(prefix) => {
                        let filename = format!("{id}.bin");
                        std::fs::write(sample_dir.join(&filename), &prefix)?;
                        row["sample_filename"] = filename.into();
                        row["sample_bytes"] = prefix.len().into();
                    }
                    Err(_) => row["sample_error"] = "could not read bounded media prefix".into(),
                }
                Ok(row)
            }.await;
            match result {
                Ok(row) => {
                    println!("HQ {} — {}: codec={}, tier={}, bitrate={} kbps; production URL={}", artist, title,
                        row["production_codec"], row["api_quality"], row["api_kbps"], row["production_url_received"]);
                    results.push(row);
                }
                Err(_) => {
                    println!("HQ {artist} — {title}: lookup/request failed");
                    results.push(serde_json::json!({"artist":artist,"title":title,"error":"lookup or stream request failed"}));
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        let successful = results.iter().filter(|row| row["production_url_received"] == true).count();
        let report = serde_json::json!({"tested_at_utc":chrono::Utc::now().to_rfc3339(),"method":"Real ApiService::fetch_track_stream_info in High mode, separate identical metadata request, bounded production stream prefixes. No UI or playback; database read-only.","results":results});
        std::fs::write(report_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(successful > 0, "No actual production stream request succeeded");
        println!("Production High test: {successful}/8 successful stream URLs");
    }
}

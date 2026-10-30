//! Public external catalogues. No credentials, private endpoints or access-control bypasses.
use parking_lot::RwLock;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use yandex_music::model::track::Track;

pub(crate) static ENABLED: AtomicBool = AtomicBool::new(false);
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("SOYAM/1.2 public music search")
        .build()
        .expect("external music HTTP client")
});
static CATALOG: LazyLock<RwLock<HashMap<String, ExternalTrack>>> = LazyLock::new(Default::default);
static UNAVAILABLE: LazyLock<RwLock<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);
static SUBSTITUTION_TARGETS: LazyLock<RwLock<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);
const MAX_TRACKS: usize = 1000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExternalTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
    pub cover: Option<String>,
    pub source: String,
    pub page: String,
    #[serde(default)]
    pub artist_page: Option<String>,
    // Session URLs are never persisted; refresh from the source page on playback.
    #[serde(skip)]
    pub url: String,
}

pub(crate) fn is_external(id: &str) -> bool {
    id.starts_with("ext-")
}
pub(crate) fn is_unavailable(id: &str) -> bool {
    is_external(id) || UNAVAILABLE.read().contains(id)
}
pub(crate) fn note_availability(track: &Track) {
    if track.substituted.is_some() || (!track.real_id.is_empty() && track.id != track.real_id) {
        SUBSTITUTION_TARGETS.write().insert(track.real_id.clone());
    }
    if track.available == Some(false)
        || track.substituted.is_some()
        || (!track.real_id.is_empty() && track.id != track.real_id)
    {
        UNAVAILABLE.write().insert(track.id.clone());
    } else if track.available == Some(true) {
        UNAVAILABLE.write().remove(&track.id);
    }
}
pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub(crate) fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub(crate) fn artist_names(value: &str) -> Vec<String> {
    static SEPARATOR: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)\s*[,;]\s*|\s+(?:feat\.?|ft\.?)\s+").unwrap());
    SEPARATOR
        .split(value)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn same_artists(names: &[String], candidate: &str) -> bool {
    // Keep names such as Earth, Wind & Fire intact when Yandex identifies
    // them as a single artist; otherwise compare complete collaborator sets.
    if names.len() == 1 && normalize(&names[0]) == normalize(candidate) {
        return true;
    }
    let mut left: Vec<_> = names.iter().map(|s| normalize(s)).collect();
    let mut right: Vec<_> = artist_names(candidate)
        .iter()
        .map(|s| normalize(s))
        .collect();
    left.sort();
    left.dedup();
    right.sort();
    right.dedup();
    if !left.is_empty() && left == right {
        return true;
    }
    // Some catalogues omit collaborators. Require the same lead artist and
    // reject any new performer; title, version and duration are checked too.
    let candidate_names = artist_names(candidate);
    names
        .first()
        .zip(candidate_names.first())
        .is_some_and(|(a, b)| normalize(a) == normalize(b))
        && !right.is_empty()
        && right.iter().all(|name| left.contains(name))
}

pub(crate) fn contains_artist(candidate: &str, name: &str) -> bool {
    normalize(candidate) == normalize(name)
        || artist_names(candidate)
            .iter()
            .any(|a| normalize(a) == normalize(name))
}

pub(crate) fn is_live(title: &str) -> bool {
    static LIVE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b(live|concert|концерт|концертная|концертный)\b").unwrap()
    });
    LIVE.is_match(title)
}

fn recording_is_live(title: &str, version: Option<&str>, album: Option<&str>) -> bool {
    static LABEL: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)(?:[\(\[]\s*live\b|\s[-–—]\s*live\b|\blive\s+(?:at|in|from|on)\b|\bconcert\b|\bконцерт)").unwrap()
    });
    version.is_some_and(is_live)
        || split_version(title).1.is_some_and(is_live)
        || LABEL.is_match(title)
        || album.is_some_and(|name| normalize(name) == "live" || LABEL.is_match(name))
}

pub(crate) fn recording_title(title: &str, version: Option<&str>) -> String {
    normalize(&format!("{title}{}", version.unwrap_or(""))).replace("speedup", "spedup")
}

fn split_version(title: &str) -> (&str, Option<&str>) {
    static VERSION: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b(live|concert|sped|speed|slowed|reverb|bass|remix|remaster|acoustic|instrumental|karaoke|cover|radio|original|концерт)\b").unwrap()
    });
    if let Some(start) = title.rfind('(').filter(|_| title.ends_with(')')) {
        let version = &title[start + 1..title.len() - 1];
        if VERSION.is_match(version) {
            return (title[..start].trim(), Some(version));
        }
    }
    (title, None)
}

pub(crate) fn same_recording(
    dto: &crate::api::models::SimpleTrackDto,
    entry: &ExternalTrack,
) -> bool {
    !dto.is_unavailable
        && dto_is_live(dto) == recording_is_live(&entry.title, None, None)
        && recording_title(&dto.title, dto.version.as_deref())
            == recording_title(&entry.title, None)
        && same_artists(
            &dto.artists
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>(),
            &entry.artist,
        )
        && (dto.duration_ms as u64).abs_diff(entry.duration_ms) <= 3000
}

pub(crate) fn prioritize_studio(tracks: &mut [crate::api::models::SimpleTrackDto], query: &str) {
    if !is_live(query) {
        tracks.sort_by_key(dto_is_live);
    }
}

fn dto_is_live(dto: &crate::api::models::SimpleTrackDto) -> bool {
    SUBSTITUTION_TARGETS.read().contains(&dto.id)
        || recording_is_live(&dto.title, dto.version.as_deref(), dto.album.as_deref())
}

pub(crate) fn artist_alias(query: &str, name: &str) -> bool {
    let initials: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .collect();
    normalize(query) != normalize(name)
        && normalize(query).len() >= 3
        && normalize(query) == normalize(&initials)
}

fn query_matches(query: &str, entry: &ExternalTrack) -> bool {
    let haystack = recording_title(&format!("{} {}", entry.artist, entry.title), None);
    let initials: String = entry
        .artist
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .collect();
    if normalize(query) == normalize(&initials) {
        return true;
    }
    query
        .split_whitespace()
        .map(normalize)
        .filter(|word| !word.is_empty())
        .all(|word| haystack.contains(&word) || (word == "speed" && haystack.contains("sped")))
}

async fn hitmos_catalog(url: reqwest::Url) -> Vec<ExternalTrack> {
    let original = url.to_string();
    let mut tracks = fetch("hitmos", url).await;
    let pages: std::collections::HashSet<_> = tracks
        .iter()
        .filter_map(|t| t.artist_page.clone())
        .collect();
    for page in pages.into_iter().filter(|page| page != &original).take(2) {
        if let Ok(url) = reqwest::Url::parse(&page) {
            tracks.extend(fetch("hitmos", url).await);
        }
    }
    tracks
}

fn matches_recording(track: &Track, candidate: &ExternalTrack) -> bool {
    let artists: Vec<_> = track
        .artists
        .iter()
        .filter_map(|a| a.name.clone())
        .collect();
    let title = track.title.as_deref().unwrap_or("");
    !artists.is_empty()
        && !title.is_empty()
        && recording_is_live(
            title,
            track.version.as_deref(),
            track.albums.first().and_then(|a| a.title.as_deref()),
        ) == recording_is_live(&candidate.title, None, None)
        && same_artists(&artists, &candidate.artist)
        && recording_title(&candidate.title, None)
            == recording_title(title, track.version.as_deref())
        && track
            .duration
            .is_some_and(|d| (d.as_millis() as u64).abs_diff(candidate.duration_ms) <= 3000)
}
pub(crate) fn remember(entries: Vec<ExternalTrack>) {
    let mut catalog = CATALOG.write();
    for entry in entries {
        if entry.source != "hitmos" {
            continue;
        }
        if catalog.len() >= MAX_TRACKS
            && !catalog.contains_key(&entry.id)
            && let Some(oldest) = catalog.keys().next().cloned()
        {
            catalog.remove(&oldest);
        }
        catalog.insert(entry.id.clone(), entry);
    }
}
pub(crate) fn snapshot() -> Vec<ExternalTrack> {
    CATALOG.read().values().cloned().collect()
}
pub(crate) fn lookup(id: &str) -> Option<Track> {
    CATALOG.read().get(id).and_then(to_track)
}
fn to_track(t: &ExternalTrack) -> Option<Track> {
    let (title, version) = split_version(&t.title);
    let artists: Vec<_> = artist_names(&t.artist)
        .into_iter()
        .map(|name| serde_json::json!({"id":"", "name":name}))
        .collect();
    serde_json::from_value(serde_json::json!({
        "id": t.id, "realId": t.id, "title": title, "version":version, "available": false,
        "artists": artists, "albums": [],
        "durationMs": t.duration_ms, "coverUri": t.cover,
    }))
    .ok()
}
fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}
fn text(node: scraper::ElementRef<'_>, css: &str) -> String {
    node.select(&selector(css))
        .next()
        .map(|n| n.text().collect::<String>().trim().to_owned())
        .unwrap_or_default()
}
fn allowed_url(base: &str, value: &str) -> Option<String> {
    let url = reqwest::Url::parse(base).ok()?.join(value).ok()?;
    let host = url.host_str()?;
    let allowed = ["hitmoz.org", "hitmoz.me", "hitmos.me", "hitmcdn.com"];
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && allowed
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}"))))
    .then(|| url.to_string())
}
pub(crate) fn parse_hitmos(html: &str, base: &str) -> Vec<ExternalTrack> {
    let document = Html::parse_document(html);
    let artist_page = if reqwest::Url::parse(base)
        .ok()
        .is_some_and(|url| url.path().starts_with("/artist/"))
    {
        Some(base.to_owned())
    } else {
        document
            .select(&selector(".breadcrumb a[href^='/artist/']"))
            .next()
            .and_then(|node| node.value().attr("href"))
            .and_then(|value| allowed_url(base, value))
    };
    let scope = document
        .select(&selector(".p-track, .p-info"))
        .next()
        .unwrap_or(document.root_element());
    scope
        .select(&selector("[data-musmeta]"))
        .filter_map(|row| {
            let meta: serde_json::Value =
                serde_json::from_str(row.value().attr("data-musmeta")?).ok()?;
            let raw_id = meta["id"].as_str()?.strip_prefix("track-id-")?;
            if !raw_id.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let title = meta["title"].as_str()?.to_owned();
            let artist = meta["artist"].as_str()?.to_owned();
            let full = row
                .select(&selector("a.track__download-btn[href]"))
                .next()?
                .value()
                .attr("href")?;
            let time = text(row, ".track__fulltime");
            let (minutes, seconds) = time.split_once(':')?;
            let duration_ms =
                (minutes.parse::<u64>().ok()? * 60 + seconds.parse::<u64>().ok()?) * 1000;
            Some(ExternalTrack {
                id: format!("ext-hitmos-{raw_id}"),
                title,
                artist,
                duration_ms,
                cover: meta["img"].as_str().and_then(|s| allowed_url(base, s)),
                source: "hitmos".into(),
                page: allowed_url(base, &format!("/song/{raw_id}"))?,
                artist_page: artist_page.clone(),
                url: allowed_url(base, full)?,
            })
        })
        .take(30)
        .collect()
}
async fn fetch(source: &str, url: reqwest::Url) -> Vec<ExternalTrack> {
    let result = async {
        let response = CLIENT.get(url).send().await?.error_for_status()?;
        let base = response.url().to_string();
        let body = response.text().await?;
        Ok::<_, reqwest::Error>(match source {
            "hitmos" => parse_hitmos(&body, &base),
            _ => Vec::new(),
        })
    }
    .await;
    match result {
        Ok(tracks) => tracks,
        Err(error) => {
            tracing::warn!(source, "External catalogue request failed: {error}");
            Vec::new()
        }
    }
}
pub(crate) async fn search(query: &str) -> Vec<ExternalTrack> {
    if !enabled() || query.trim().is_empty() {
        return Vec::new();
    }
    // Public song/artist links are also accepted in the search field. This
    // discovers the artist's other versions without hardcoding track IDs.
    if (query.trim().starts_with("https://") || query.trim().starts_with("http://"))
        && let Ok(url) = reqwest::Url::parse(query.trim())
    {
        if allowed_url("https://ru.hitmoz.org/", query).is_some()
            && url
                .host_str()
                .is_some_and(|h| h.ends_with(".hitmoz.org") || h == "hitmos.me")
            && (url.path().starts_with("/song/") || url.path().starts_with("/artist/"))
        {
            let tracks = hitmos_catalog(url).await;
            remember(tracks.clone());
            return tracks;
        }
        return Vec::new();
    }
    let cached: Vec<_> = snapshot()
        .into_iter()
        .filter(|entry| query_matches(query, entry))
        .collect();
    let known_page = cached
        .iter()
        .find(|entry| entry.source == "hitmos")
        .and_then(|entry| {
            reqwest::Url::parse(entry.artist_page.as_deref().unwrap_or(&entry.page)).ok()
        });
    let mut hitmos = reqwest::Url::parse("https://ru.hitmoz.org/search").unwrap();
    hitmos.query_pairs_mut().append_pair("q", query);
    let (a, known) = tokio::join!(fetch("hitmos", hitmos), async {
        if let Some(url) = known_page {
            hitmos_catalog(url).await
        } else {
            Vec::new()
        }
    });
    let mut tracks = Vec::new();
    for track in a.into_iter().chain(known).chain(cached) {
        if !query_matches(query, &track) {
            continue;
        }
        if tracks.iter().any(|t: &ExternalTrack| {
            t.id == track.id
                || (recording_title(&t.title, None) == recording_title(&track.title, None)
                    && (same_artists(&artist_names(&t.artist), &track.artist)
                        || same_artists(&artist_names(&track.artist), &t.artist))
                    && t.duration_ms.abs_diff(track.duration_ms) <= 3000)
        }) {
            continue;
        }
        tracks.push(track);
    }
    if enabled() {
        remember(tracks.clone());
        tracks
    } else {
        Vec::new()
    }
}
pub(crate) async fn stream_url(track: &Track) -> Result<String, String> {
    if is_external(&track.id) && !track.id.starts_with("ext-hitmos-") {
        return Err("Этот внешний источник больше не поддерживается".into());
    }
    if !enabled() {
        return Err("Включите «Недоступные треки» в настройках".into());
    }
    let entry = { CATALOG.read().get(&track.id).cloned() };
    if let Some(entry) = entry {
        let url = reqwest::Url::parse(&entry.page).map_err(|e| e.to_string())?;
        let refreshed = fetch(&entry.source, url).await;
        if let Some(found) = refreshed.iter().find(|t| t.id == entry.id) {
            let url = found.url.clone();
            remember(refreshed);
            return Ok(url);
        }
        // Fresh search results may be playable even if a provider's song page
        // has a different layout. Persisted records never retain session URLs.
        if !entry.url.is_empty() {
            return Ok(entry.url);
        }
    }
    let artist = track
        .artists
        .first()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("");
    let title = track.title.as_deref().unwrap_or("");
    let query = format!(
        "{artist} {title} {}",
        track.version.as_deref().unwrap_or("")
    );
    let matches: Vec<_> = search(&query)
        .await
        .into_iter()
        .filter(|candidate| matches_recording(track, candidate))
        .collect();
    if matches.len() == 1 {
        return Ok(matches[0].url.clone());
    }
    Err("Не найдено однозначное совпадение в доступных источниках".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(title: &str, artist: &str) -> ExternalTrack {
        ExternalTrack {
            id: "ext-test".into(),
            title: title.into(),
            artist: artist.into(),
            duration_ms: 138000,
            cover: None,
            source: "hitmos".into(),
            page: String::new(),
            artist_page: None,
            url: String::new(),
        }
    }

    #[test]
    fn collaborators_are_separate_and_original_duplicates_merge() {
        let entry = sample("EUROMOLLY", "erikkaspi, elmora");
        let mut track = to_track(&entry).unwrap();
        assert_eq!(
            track
                .artists
                .iter()
                .filter_map(|a| a.name.as_deref())
                .collect::<Vec<_>>(),
            ["erikkaspi", "elmora"]
        );
        let empty = std::collections::HashSet::<String>::new();
        track.id = "euromolly-native-test".into();
        track.real_id = track.id.clone();
        track.available = Some(true);
        let dto = crate::api::models::SimpleTrackDto::from_yandex(&track, &empty, &empty);
        assert!(same_recording(&dto, &entry));
        assert!(same_recording(&dto, &sample("EUROMOLLY", "erikkaspi")));
        assert!(!same_recording(
            &dto,
            &sample("EUROMOLLY (Speed up + Bass)", "erikkaspi")
        ));
        assert!(!same_recording(&dto, &sample("EUROMOLLY", "other artist")));
        let sped = to_track(&sample("EUROMOLLY (Speed up + Bass)", "erikkaspi")).unwrap();
        assert_eq!(sped.title.as_deref(), Some("EUROMOLLY"));
        assert_eq!(sped.version.as_deref(), Some("Speed up + Bass"));
    }

    #[test]
    fn live_album_metadata_keeps_the_studio_recording_distinct() {
        let mut live = to_track(&sample("Toxicity", "System of a Down")).unwrap();
        live.albums =
            serde_json::from_value(serde_json::json!([{"id":1,"title":"Live at a festival"}]))
                .unwrap();
        let empty = std::collections::HashSet::<String>::new();
        let dto = crate::api::models::SimpleTrackDto::from_yandex(&live, &empty, &empty);
        assert!(
            !same_recording(&dto, &sample("Toxicity", "System of a Down")),
            "album-level live metadata must not hide the studio version"
        );
        assert!(!matches_recording(
            &live,
            &sample("Toxicity", "System of a Down")
        ));
        assert!(artist_alias("SOAD", "System Of A Down"));
        assert!(!recording_is_live(
            "Live Forever",
            None,
            Some("Live Through This")
        ));
    }

    #[test]
    fn substituted_tracks_do_not_hide_the_original_but_available_tracks_do() {
        let cached = crate::util::track::track_from_metadata(&crate::storage::db::TrackMetadata {
            id: "672687".into(),
            title: "Toxicity".into(),
            version: None,
            artists: vec![],
            album: None,
            album_id: None,
            cover_url: None,
            duration_ms: 216270,
            is_explicit: None,
        });
        let substituted: Track = serde_json::from_value(serde_json::json!({
            "id":"672687", "realId":"147251586", "title":"Toxicity", "available":true,
            "durationMs":216270, "artists":[{"id":67437,"name":"System of A Down"}],
            "albums":[{"id":40227128,"title":"Rock Am Ring 2011","version":"HQ 2026 Remastered"}],
            "substituted":{"id":"672687","realId":"672687","title":"Toxicity","available":false,
                "durationMs":216270,"error":"no-rights","artists":[{"id":67437,"name":"System of A Down"}]}
        })).unwrap();
        let empty = std::collections::HashSet::<String>::new();
        let dto = crate::api::models::SimpleTrackDto::from_yandex(&substituted, &empty, &empty);
        assert!(dto.is_unavailable);
        assert!(
            crate::api::models::SimpleTrackDto::from_yandex(&cached, &empty, &empty).is_unavailable,
            "old metadata must not erase a confirmed server-side substitution"
        );
        assert!(
            dto.album.is_none(),
            "replacement concert metadata must not describe the original"
        );
        let mut studio = sample("Toxicity", "System of A Down");
        studio.duration_ms = 216270;
        assert!(!same_recording(&dto, &studio));
        assert!(
            crate::api::models::SimpleTrackDto::from_yandex_owned(substituted, &empty, &empty)
                .is_unavailable
        );

        let replacement: Track = serde_json::from_value(serde_json::json!({
            "id":"147251586","realId":"147251586","title":"Toxicity","available":true,
            "durationMs":216270,"artists":[{"id":67437,"name":"System of A Down"}],
            "albums":[{"id":40227128,"title":"Rock Am Ring 2011"}]
        }))
        .unwrap();
        let replacement_dto =
            crate::api::models::SimpleTrackDto::from_yandex(&replacement, &empty, &empty);
        assert!(
            !same_recording(&replacement_dto, &studio),
            "a known replacement cannot hide a studio original even without a live label"
        );

        let available: Track = serde_json::from_value(serde_json::json!({
            "id":"149717716","realId":"149717716","title":"EUROMOLLY","available":true,
            "contentWarning":"explicit","durationMs":137810,
            "artists":[{"id":23449259,"name":"erikkaspi"},{"id":13702941,"name":"elmora"}]
        }))
        .unwrap();
        let dto = crate::api::models::SimpleTrackDto::from_yandex(&available, &empty, &empty);
        assert!(dto.is_explicit);
        assert!(!dto.is_unavailable);
        assert!(same_recording(
            &dto,
            &sample("EUROMOLLY", "erikkaspi, elmora")
        ));
        assert!(!same_recording(
            &dto,
            &sample("EUROMOLLY (Speed up + Bass)", "erikkaspi")
        ));
    }

    #[tokio::test]
    #[ignore = "explicit verification of the reported public pages"]
    async fn live_reported_studio_and_speed_versions() {
        ENABLED.store(true, Ordering::Relaxed);
        let versions = search("https://ru.hitmoz.org/song/81493731").await;
        assert!(
            versions
                .iter()
                .any(|t| t.title == "EUROMOLLY (Speed up + Bass)")
        );
        assert!(
            versions
                .iter()
                .any(|t| t.title == "EUROMOLLY" && artist_names(&t.artist).len() == 2)
        );
        let ordinary_search = search("euromolly").await;
        assert!(
            ordinary_search
                .iter()
                .any(|t| t.title == "EUROMOLLY (Speed up + Bass)"),
            "known artist catalogues must supply versions when public text search is empty"
        );
        assert!(
            ordinary_search
                .iter()
                .all(|t| normalize(&t.title).contains("euromolly"))
        );
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn automatic_matches_require_artist_version_and_duration() {
        let mut track = crate::util::track::test_track("unavailable");
        track.title = Some("Song".into());
        track.artists =
            serde_json::from_value(serde_json::json!([{"id":"1", "name":"Artist"}])).unwrap();
        track.duration = Some(Duration::from_secs(180));
        let mut candidate = ExternalTrack {
            id: "ext-test".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            duration_ms: 180000,
            cover: None,
            source: "test".into(),
            page: String::new(),
            artist_page: None,
            url: String::new(),
        };
        assert!(matches_recording(&track, &candidate));
        candidate.title = "Song (Live)".into();
        assert!(!matches_recording(&track, &candidate));
        track.version = Some("Live".into());
        assert!(matches_recording(&track, &candidate));
        candidate.artist = "Cover Artist".into();
        assert!(!matches_recording(&track, &candidate));
        candidate.artist = "Artist".into();
        candidate.duration_ms += 10000;
        assert!(!matches_recording(&track, &candidate));
        track.duration = None;
        assert!(!matches_recording(&track, &candidate));
    }

    #[test]
    fn hitmos_uses_full_audio_and_preserves_source_metadata() {
        let html = r#"<li data-musmeta='{"id":"track-id-42","artist":"Artist","title":"Song","url":"https://ru.hitmoz.org/get/cuts/42.mp3","img":"https://statcore.hitmcdn.com/cover/42.jpg"}'>
            <span class="track__fulltime">03:20</span>
            <a class="track__download-btn" href="/get/music/42.mp3"></a></li>"#;
        let tracks = parse_hitmos(html, "https://ru.hitmoz.org/");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].url, "https://ru.hitmoz.org/get/music/42.mp3");
        let track = to_track(&tracks[0]).expect("valid external playback record");
        assert_eq!(track.available, Some(false));
        assert_eq!(track.duration.unwrap().as_secs(), 200);
        assert_eq!(track.artists[0].name.as_deref(), Some("Artist"));
        let stored = serde_json::to_string(&tracks[0]).unwrap();
        assert!(
            !stored.contains("get/music"),
            "temporary audio addresses must not be stored"
        );
    }

    #[test]
    fn empty_or_changed_pages_produce_no_fake_results() {
        for html in [
            "",
            "<html>Sign in</html>",
            "<li data-musmeta='broken'></li>",
        ] {
            assert!(parse_hitmos(html, "https://ru.hitmoz.org/").is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "explicit live provider verification"]
    async fn live_public_provider_pages_and_audio() {
        let hitmos = fetch(
            "hitmos",
            reqwest::Url::parse("https://ru.hitmoz.org/song/81423717").unwrap(),
        )
        .await;
        assert!(
            !hitmos.is_empty(),
            "Hitmos song page must expose public audio"
        );
        for entry in [&hitmos[0]] {
            let mut response = CLIENT
                .get(&entry.url)
                .header(reqwest::header::RANGE, "bytes=0-1023")
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                content_type.starts_with("audio/")
                    || content_type.starts_with("application/octet-stream"),
                "{} returned {content_type}",
                entry.source
            );
            assert!(!response.chunk().await.unwrap().unwrap().is_empty());
        }
    }
}

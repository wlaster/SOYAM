//! Artist audience orchestration. Search/parsing/fallbacks are kept private.
mod fallbacks;
mod parsing;
mod search;
#[cfg(test)]
mod tests;

use crate::{api::models::SpotifyArtistStatsDto, app::AppContext};
use futures::FutureExt;
use parking_lot::Mutex;
use parsing::{cross_script_key, spotify_url};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

const APP_AGENT: &str = "SOYAM/2.5.2 (https://github.com/wlaster/SOYAM)";
static CATALOG_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let builder = reqwest::Client::builder();
    #[cfg(target_os = "windows")]
    let builder = builder.tls_backend_native();
    builder
        .http1_only()
        .brotli(false)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(7))
        .user_agent(APP_AGENT)
        .build()
        .unwrap_or_default()
});

static CACHE: LazyLock<Mutex<HashMap<String, (Instant, SpotifyArtistStatsDto)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static LOOKUP_LOCKS: LazyLock<Mutex<HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached_audience(artist_id: &str) -> Option<SpotifyArtistStatsDto> {
    CACHE
        .lock()
        .get(artist_id)
        .filter(|(when, _)| when.elapsed() < Duration::from_secs(3600))
        .map(|(_, v)| v.clone())
}

// Reuse the data already loaded for the Yandex artist page. Store only the
// identity and recording references, rather than another full artist DTO.
#[flutter_rust_bridge::frb(ignore)]
#[derive(Clone, Default)]
struct Repertoire {
    recordings: HashMap<String, Vec<u64>>,
    popular_titles: Vec<String>,
}
#[derive(Clone)]
struct ArtistSnapshot {
    created: Instant,
    name: String,
    links: Vec<String>,
    repertoire: Repertoire,
}
static PAGE_ARTISTS: LazyLock<Mutex<HashMap<String, ArtistSnapshot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[flutter_rust_bridge::frb(ignore)]
pub(crate) fn remember_artist_page(
    artist_id: &str,
    artist: &yandex_music::model::artist::Artist,
    tracks: &[yandex_music::model::track::Track],
) {
    let mut cache = PAGE_ARTISTS.lock();
    if cache.len() >= 64 {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, value)| value.created)
            .map(|(id, _)| id.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        artist_id.to_owned(),
        ArtistSnapshot {
            created: Instant::now(),
            name: artist.name.clone().unwrap_or_default(),
            links: artist.links.iter().map(|link| link.href.clone()).collect(),
            repertoire: track_references(tracks),
        },
    );
}

fn page_snapshot(artist_id: &str) -> Option<ArtistSnapshot> {
    PAGE_ARTISTS
        .lock()
        .get(artist_id)
        .filter(|value| value.created.elapsed() < Duration::from_secs(600))
        .cloned()
}

fn track_references(tracks: &[yandex_music::model::track::Track]) -> Repertoire {
    let mut repertoire = Repertoire::default();
    for track in tracks {
        if let Some(title) = track.title.as_deref() {
            if !repertoire
                .popular_titles
                .iter()
                .any(|known| cross_script_key(known) == cross_script_key(title))
            {
                repertoire.popular_titles.push(title.to_owned());
            }
            repertoire
                .recordings
                .entry(cross_script_key(title))
                .or_insert_with(Vec::new)
                .push(
                    track
                        .duration
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or_default(),
                );
        }
    }
    repertoire
}

async fn repertoire_for(api: &crate::http::ApiService, artist_id: &str) -> Option<Repertoire> {
    if let Some(snapshot) =
        page_snapshot(artist_id).filter(|snapshot| !snapshot.repertoire.recordings.is_empty())
    {
        return Some(snapshot.repertoire);
    }
    let (tracks, _) = api
        .fetch_artist_tracks_paginated(artist_id.to_owned(), 0, 20)
        .await
        .ok()?;
    Some(track_references(&tracks))
}

async fn discover_spotify(
    artist_id: &str,
    name: &str,
    api: Option<&crate::http::ApiService>,
) -> Option<String> {
    if let Some(url) = search::spotify_public_search(name, api.map(|api| (api, artist_id))).await {
        return Some(url);
    }
    let wikidata = async {
        if let Some(url) = fallbacks::wikidata_spotify(artist_id).await {
            return Some(url);
        }
        fallbacks::wikidata_spotify_by_name(artist_id, name).await
    };
    fallbacks::first_profile(vec![
        wikidata.boxed(),
        fallbacks::musicbrainz_spotify(name).boxed(),
    ])
    .await
}

async fn load_artist_spotify_stats(
    api: &crate::http::ApiService,
    artist_id: &str,
) -> SpotifyArtistStatsDto {
    let snapshot = if let Some(snapshot) = page_snapshot(artist_id) {
        snapshot
    } else {
        let Ok((artist, _)) = api.fetch_artist(artist_id.to_owned()).await else {
            return empty_stats();
        };
        ArtistSnapshot {
            created: Instant::now(),
            name: artist.name.unwrap_or_default(),
            links: artist.links.into_iter().map(|link| link.href).collect(),
            repertoire: Repertoire::default(),
        }
    };
    let mut url = snapshot.links.iter().find_map(|link| spotify_url(link));
    if url.is_none() {
        url = fallbacks::artist_hub_spotify(&snapshot.links).await;
    }
    if url.is_none() {
        url = discover_spotify(artist_id, &snapshot.name, Some(api)).await;
    }
    let monthly_listeners = if let Some(url) = &url {
        fetch_listeners(url).await
    } else {
        None
    };
    SpotifyArtistStatsDto {
        monthly_listeners,
        url,
    }
}

fn empty_stats() -> SpotifyArtistStatsDto {
    SpotifyArtistStatsDto {
        monthly_listeners: None,
        url: None,
    }
}

async fn within_deadline(
    artist_id: &str,
    future: impl std::future::Future<Output = SpotifyArtistStatsDto>,
    deadline: Duration,
) -> SpotifyArtistStatsDto {
    tokio::time::timeout(deadline, future)
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("Spotify lookup deadline for Yandex artist {artist_id}");
            empty_stats()
        })
}

enum ProfileResult {
    Audience(u64),
    Unavailable,
    Missing,
}

async fn fetch_profile(url: &str) -> ProfileResult {
    match CATALOG_CLIENT
        .get(url)
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => read_profile_audience(response)
            .await
            .map(ProfileResult::Audience)
            .unwrap_or(ProfileResult::Unavailable),
        Ok(response) if matches!(response.status().as_u16(), 404 | 410) => ProfileResult::Missing,
        Ok(response) => {
            tracing::warn!("Spotify audience HTTP {}", response.status());
            ProfileResult::Unavailable
        }
        Err(error) => {
            tracing::warn!("Spotify audience connection: {error}");
            ProfileResult::Unavailable
        }
    }
}

async fn fetch_listeners(url: &str) -> Option<u64> {
    match fetch_profile(url).await {
        ProfileResult::Audience(count) => Some(count),
        _ => None,
    }
}

async fn read_profile_audience(mut response: reqwest::Response) -> Option<u64> {
    const MAX_PAGE: usize = 2 * 1024 * 1024;
    let mut prefix = Vec::new();
    let mut next_scan = 0;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if chunk.len() > MAX_PAGE.saturating_sub(prefix.len()) {
                    return None;
                }
                prefix.extend_from_slice(&chunk);
                if prefix.len() <= 64 * 1024 || prefix.len() >= next_scan {
                    if let Some(count) = parsing::parse_audience(&String::from_utf8_lossy(&prefix))
                    {
                        return Some(count);
                    }
                    next_scan = prefix.len().max(1024).saturating_mul(2);
                }
            }
            Ok(None) => return parsing::parse_audience(&String::from_utf8_lossy(&prefix)),
            Err(error) => {
                tracing::warn!("Spotify audience response: {error}");
                return None;
            }
        }
    }
}

fn valid_saved_profile(entry: &(u64, String), now: u64) -> bool {
    entry.0 <= now && now - entry.0 < 30 * 86400 && spotify_url(&entry.1).is_some()
}

pub async fn get_artist_spotify_stats(
    ctx: &AppContext,
    artist_id: String,
) -> SpotifyArtistStatsDto {
    within_deadline(
        &artist_id,
        get_artist_spotify_stats_inner(ctx, &artist_id),
        Duration::from_secs(25),
    )
    .await
}

async fn get_artist_spotify_stats_inner(
    ctx: &AppContext,
    artist_id: &str,
) -> SpotifyArtistStatsDto {
    if let Some(cached) = cached_audience(artist_id) {
        return cached;
    }
    let lock = {
        let mut locks = LOOKUP_LOCKS.lock();
        if locks.len() >= 256 {
            locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        locks
            .entry(artist_id.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().await;
    if let Some(cached) = cached_audience(artist_id) {
        return cached;
    }
    let setting = format!("spotify_profile_{artist_id}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let stored = ctx
        .core
        .db
        .lock()
        .await
        .load_setting::<(u64, String)>(&setting)
        .await
        .ok()
        .flatten()
        .filter(|entry| valid_saved_profile(entry, now));
    let mut fresh_identity = stored.is_none();
    let result = if let Some((_, url)) = stored {
        match fetch_profile(&url).await {
            ProfileResult::Audience(count) => SpotifyArtistStatsDto {
                monthly_listeners: Some(count),
                url: Some(url),
            },
            ProfileResult::Unavailable => SpotifyArtistStatsDto {
                monthly_listeners: None,
                url: Some(url),
            },
            ProfileResult::Missing => {
                // A deleted profile is stale identity data, unlike a failed VPN/network request.
                let _ = ctx.core.db.lock().await.delete_setting(&setting).await;
                fresh_identity = true;
                load_artist_spotify_stats(&ctx.core.api, artist_id).await
            }
        }
    } else {
        load_artist_spotify_stats(&ctx.core.api, artist_id).await
    };
    if result.monthly_listeners.is_some() {
        // Persist identities only when discovered. Reading an audience must not
        // rewrite the same row or renew an old profile's resolution timestamp.
        if fresh_identity {
            if let Some(url) = &result.url {
                let _ = ctx
                    .core
                    .db
                    .lock()
                    .await
                    .save_setting(&setting, &(now, url))
                    .await;
            }
        }
        let mut cache = CACHE.lock();
        if cache.len() >= 100 {
            cache.retain(|_, (when, _)| when.elapsed() < Duration::from_secs(60));
        }
        cache.insert(artist_id.to_owned(), (Instant::now(), result.clone()));
    }
    result
}

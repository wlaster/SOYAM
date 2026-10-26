use super::parsing::*;
use super::{APP_AGENT, CATALOG_CLIENT};
use crate::lyrics::CLIENT;
use futures::StreamExt;
use parking_lot::Mutex;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::LazyLock,
    time::{Duration, Instant},
};

static WD_GATE: LazyLock<tokio::sync::Mutex<Option<Instant>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

static WD_BLOCKED: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

static WD_CACHE: LazyLock<Mutex<HashMap<String, Value>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) async fn wikidata_json(query: &[(&str, &str)]) -> Option<Value> {
    let key = serde_json::to_string(query).ok()?;
    if let Some(value) = WD_CACHE.lock().get(&key).cloned() {
        return Some(value);
    }
    let mut gate = WD_GATE.lock().await;
    if WD_BLOCKED
        .lock()
        .is_some_and(|until| until > Instant::now())
    {
        return None;
    }
    if let Some(last) = *gate {
        tokio::time::sleep(Duration::from_millis(1100).saturating_sub(last.elapsed())).await;
    }
    *gate = Some(Instant::now());
    let response = CATALOG_CLIENT
        .get("https://www.wikidata.org/w/api.php")
        .query(query)
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!("Wikidata request failed: {error}");
            return None;
        }
    };
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let delay = response
            .headers()
            .get("retry-after")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        *WD_BLOCKED.lock() = Some(Instant::now() + Duration::from_secs(delay));
        tracing::warn!("Wikidata rate limited; suspending requests for {delay}s");
        return None;
    }
    if !response.status().is_success() {
        tracing::warn!("Wikidata HTTP {}", response.status());
        return None;
    }
    let value: Value = response.json().await.ok()?;
    if value.get("error").is_some() {
        return None;
    }
    let mut cache = WD_CACHE.lock();
    if cache.len() >= 32 {
        cache.clear();
    }
    cache.insert(key, value.clone());
    Some(value)
}

pub(super) async fn wikidata_spotify(artist_id: &str) -> Option<String> {
    if artist_id.is_empty() || !artist_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let search = wikidata_json(&[
        ("action", "query"),
        ("list", "search"),
        ("srsearch", &format!("haswbstatement:P1553={artist_id}")),
        ("format", "json"),
    ])
    .await?;
    let matches = search["query"]["search"].as_array()?;
    if matches.len() != 1 {
        return None;
    }
    let id = matches[0]["title"].as_str()?;
    if !id.starts_with('Q') || !id[1..].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let entity = wikidata_json(&[
        ("action", "wbgetentities"),
        ("ids", id),
        ("props", "claims"),
        ("format", "json"),
    ])
    .await?;
    spotify_from_claims(&entity["entities"][id]["claims"], artist_id)
}

pub(super) fn spotify_from_claims(claims: &Value, artist_id: &str) -> Option<String> {
    let active_values = |property: &str| -> Vec<&str> {
        claims[property]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|claim| claim["rank"].as_str() != Some("deprecated"))
            .filter_map(|claim| claim["mainsnak"]["datavalue"]["value"].as_str())
            .collect()
    };
    if !active_values("P1553").contains(&artist_id) {
        return None;
    }
    let urls: std::collections::HashSet<_> = active_values("P1902")
        .into_iter()
        .filter_map(|id| spotify_url(&format!("https://open.spotify.com/artist/{id}")))
        .collect();
    // Multiple active Spotify identities require an explicit link, not a guess.
    if urls.len() == 1 {
        urls.into_iter().next()
    } else {
        None
    }
}

// Some Wikidata profiles have only a Spotify ID. Accept a name/alias lookup
// only when exactly one Spotify identity matches, and reject conflicting Yandex IDs.

pub(super) async fn wikidata_spotify_by_name(artist_id: &str, name: &str) -> Option<String> {
    let search = wikidata_json(&[
        ("action", "wbsearchentities"),
        ("search", name),
        ("language", "en"),
        ("limit", "10"),
        ("format", "json"),
    ])
    .await?;
    let ids: Vec<&str> = search["search"]
        .as_array()?
        .iter()
        .filter(|item| {
            item["match"]["text"]
                .as_str()
                .is_some_and(|text| normalized_artist_name(text) == normalized_artist_name(name))
        })
        .filter_map(|item| item["id"].as_str())
        .filter(|id| id.starts_with('Q') && id[1..].chars().all(|c| c.is_ascii_digit()))
        .collect();
    if ids.is_empty() {
        return None;
    }
    let entities = wikidata_json(&[
        ("action", "wbgetentities"),
        ("ids", &ids.join("|")),
        ("props", "claims"),
        ("format", "json"),
    ])
    .await?;
    let urls: std::collections::HashSet<_> = ids
        .iter()
        .filter_map(|id| {
            let claims = &entities["entities"][*id]["claims"];
            if claims["P1553"].is_array() {
                return spotify_from_claims(claims, artist_id);
            }
            // Reuse the strict identity/ambiguity checks with this already matched name.
            let mut claims = claims.clone();
            claims["P1553"] = serde_json::json!([{"mainsnak":{"datavalue":{"value":artist_id}}}]);
            spotify_from_claims(&claims, artist_id)
        })
        .collect();
    if urls.len() == 1 {
        return urls.into_iter().next();
    }
    if urls.is_empty() && ids.len() == 1 {
        let claims = &entities["entities"][ids[0]]["claims"];
        if let Some(values) = claims["P434"].as_array() {
            if values.len() == 1 {
                if let Some(id) = values[0]["mainsnak"]["datavalue"]["value"].as_str() {
                    return musicbrainz_spotify_by_id(id).await;
                }
            }
        }
    }
    None
}

pub(super) fn is_bandlink(value: &str) -> bool {
    reqwest::Url::parse(value).ok().is_some_and(|url| {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host == "band.link" || host.ends_with(".band.link"))
    })
}

pub(super) async fn artist_hub_spotify(links: &[String]) -> Option<String> {
    for link in links.iter().filter(|link| is_bandlink(link)).take(3) {
        let Ok(response) = CLIENT.get(link).send().await else {
            continue;
        };
        if !response.status().is_success() || !is_bandlink(response.url().as_str()) {
            continue;
        }
        if response
            .content_length()
            .is_some_and(|size| size > 2 * 1024 * 1024)
        {
            continue;
        }
        let Ok(html) = response.text().await else {
            continue;
        };
        if html.len() > 2 * 1024 * 1024 {
            continue;
        }
        if let Some(url) = spotify_from_hub_html(&html) {
            return Some(url);
        }
    }
    None
}

// Public Spotify web search, using the anonymous session published by its embed.
// No user cookies, Spotify login, stored credentials, or private API keys.

pub(super) async fn first_profile<'a>(
    lookups: Vec<futures::future::BoxFuture<'a, Option<String>>>,
) -> Option<String> {
    let mut sources: futures::stream::FuturesUnordered<_> = lookups.into_iter().collect();
    while let Some(result) = sources.next().await {
        if result.is_some() {
            return result;
        }
    }
    None
}

static MB_PREFERRED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

static MB_BAD_UNTIL: LazyLock<Mutex<[Option<Instant>; 2]>> =
    LazyLock::new(|| Mutex::new([None, None]));

static MB_CACHE: LazyLock<Mutex<HashMap<String, Value>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static MB_LAST_REQUEST: LazyLock<tokio::sync::Mutex<Option<Instant>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

pub(super) async fn musicbrainz_json(path: &str, query: &[(&str, String)]) -> Option<Value> {
    const HOSTS: [&str; 2] = ["https://musicbrainz.org", "https://beta.musicbrainz.org"];
    let key = format!("{path}{}", serde_json::to_string(query).ok()?);
    if let Some(value) = MB_CACHE.lock().get(&key).cloned() {
        return Some(value);
    }
    let preferred = MB_PREFERRED.load(std::sync::atomic::Ordering::Relaxed);
    for index in [preferred, 1 - preferred] {
        if MB_BAD_UNTIL.lock()[index].is_some_and(|until| until > Instant::now()) {
            continue;
        }
        for attempt in 0..2 {
            // Serialize all requests, including retries, to respect MusicBrainz's
            // one-request-per-second limit across concurrent artist cards.
            let mut last = MB_LAST_REQUEST.lock().await;
            if let Some(started) = *last {
                let remaining = Duration::from_millis(1100).saturating_sub(started.elapsed());
                tokio::time::sleep(remaining).await;
            }
            *last = Some(Instant::now());
            drop(last);
            if let Some(value) = MB_CACHE.lock().get(&key).cloned() {
                return Some(value);
            }
            let response = CATALOG_CLIENT
                .get(format!("{}{path}", HOSTS[index]))
                .header("User-Agent", APP_AGENT)
                .query(query)
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    tracing::warn!("MusicBrainz {} connection failed: {error}", HOSTS[index]);
                    MB_BAD_UNTIL.lock()[index] = Some(Instant::now() + Duration::from_secs(60));
                    break;
                }
            };
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|s| s.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                *MB_BAD_UNTIL.lock() = [Some(Instant::now() + Duration::from_secs(delay)); 2];
                return None;
            }
            if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE && attempt == 0 {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|s| s.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2);
                if delay > 5 {
                    return None;
                }
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            if !response.status().is_success() {
                tracing::warn!("MusicBrainz {} HTTP {}", HOSTS[index], response.status());
                MB_BAD_UNTIL.lock()[index] = Some(Instant::now() + Duration::from_secs(15));
                break;
            }
            if let Ok(value) = response.json::<Value>().await {
                MB_PREFERRED.store(index, std::sync::atomic::Ordering::Relaxed);
                let mut cache = MB_CACHE.lock();
                if cache.len() >= 32 {
                    cache.clear();
                }
                cache.insert(key.clone(), value.clone());
                return Some(value);
            }
            break;
        }
    }
    None
}

pub(super) async fn musicbrainz_spotify(name: &str) -> Option<String> {
    let search = musicbrainz_json(
        "/ws/2/artist",
        &[
            ("query", format!("artist:\"{}\"", name.replace('"', ""))),
            ("fmt", "json".into()),
            ("limit", "5".into()),
        ],
    )
    .await?;
    let matches: Vec<_> = search["artists"]
        .as_array()?
        .iter()
        .filter(|artist| {
            artist["name"]
                .as_str()
                .is_some_and(|n| normalized_artist_name(n) == normalized_artist_name(name))
        })
        .collect();
    if matches.len() != 1 {
        return None;
    }
    musicbrainz_spotify_by_id(matches[0]["id"].as_str()?).await
}

pub(super) async fn musicbrainz_spotify_by_id(id: &str) -> Option<String> {
    if id.len() != 36 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    let detail = musicbrainz_json(
        &format!("/ws/2/artist/{id}"),
        &[("fmt", "json".into()), ("inc", "url-rels".into())],
    )
    .await?;
    find_spotify_link(&detail["relations"])
}

// This is the complete production lookup, also exercised against real Yandex
// profiles in the live regression test (including links from brief-info).

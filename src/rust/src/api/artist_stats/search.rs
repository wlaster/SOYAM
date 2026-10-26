use super::parsing::*;
use super::{CATALOG_CLIENT, Repertoire, repertoire_for};
use futures::StreamExt;
use parking_lot::Mutex;
use serde_json::Value;
use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};

const PUBLIC_PLAYLIST: &str = "37i9dQZF1DXcBWIGoYBM5M";

const ARTIST_QUERY_HASH: &str = "903df2a65d8121e27d73a2be03c01e88ebe6021bb6d4eb82a389e35d87e51d27";

static SEARCH_SESSION: LazyLock<tokio::sync::Mutex<Option<(Instant, String)>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

static SEARCH_HASH: LazyLock<Mutex<std::collections::HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

static SEARCH_BLOCKED: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

pub(super) async fn anonymous_search_session() -> Option<String> {
    let mut session = SEARCH_SESSION.lock().await;
    if let Some((created, token)) = &*session {
        if *created > Instant::now() {
            return Some(token.clone());
        }
    }
    let html = CATALOG_CLIENT
        .get(format!(
            "https://open.spotify.com/embed/playlist/{PUBLIC_PLAYLIST}"
        ))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    let data = parse_embed_state(&html)?;
    let token = data
        .pointer("/props/pageProps/state/settings/session/accessToken")?
        .as_str()?
        .to_owned();
    if token.is_empty() || token.len() > 8192 {
        return None;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let ttl_ms = data
        .pointer("/props/pageProps/state/settings/session/accessTokenExpirationTimestampMs")
        .and_then(Value::as_u64)
        .map(|expires| expires.saturating_sub(now_ms).saturating_sub(60_000))
        .unwrap_or(2_400_000)
        .min(3_600_000);
    *session = Some((
        Instant::now() + Duration::from_millis(ttl_ms),
        token.clone(),
    ));
    Some(token)
}

struct SearchCandidates {
    best: Vec<String>,
    related: Vec<String>,
}

fn analyse_candidates(value: &Value, name: &str) -> SearchCandidates {
    let mut result = SearchCandidates {
        best: Vec::new(),
        related: Vec::new(),
    };
    let Some(items) = value
        .pointer("/data/searchV2/artists/items")
        .and_then(Value::as_array)
    else {
        return result;
    };
    let wanted = ArtistName::new(name);
    let mut best_score = 0;
    for item in items {
        let data = &item["data"];
        let Some(name) = data["profile"]["name"].as_str() else {
            continue;
        };
        let Some(url) = data["id"]
            .as_str()
            .and_then(|id| spotify_url(&format!("https://open.spotify.com/artist/{id}")))
        else {
            continue;
        };
        let candidate = ArtistName::new(name);
        let score = candidate.match_score(&wanted);
        if score > 0 {
            if score > best_score {
                result.best.clear();
                best_score = score;
            }
            if score == best_score && !result.best.contains(&url) {
                result.best.push(url.clone());
            }
        }
        let similar_script = if score == 0
            && (candidate.cyrillic || wanted.cyrillic)
            && !wanted.cross_script.is_empty()
        {
            let a = &wanted.cross_script;
            let b = &candidate.cross_script;
            a == b
                || (a.len() >= 6
                    && a.len().abs_diff(b.len()) * 5 <= a.len().max(b.len())
                    && crate::lyrics::providers::levenshtein(a, b) * 5 <= a.len().max(b.len()))
        } else {
            false
        };
        if (score > 0 || similar_script) && !result.related.contains(&url) {
            result.related.push(url);
        }
    }
    if result.related.is_empty() {
        // Aliases still require the two-recording identity check.
        result.related = items
            .iter()
            .take(4)
            .filter_map(|item| item["data"]["id"].as_str())
            .filter_map(|id| spotify_url(&format!("https://open.spotify.com/artist/{id}")))
            .collect();
    }
    result
}

#[cfg(test)]
pub(super) fn searched_artist_url(value: &Value, name: &str) -> Option<String> {
    let mut candidates = analyse_candidates(value, name).best;
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}

#[cfg(test)]
pub(super) fn related_artist_candidates(value: &Value, name: &str) -> Vec<String> {
    analyse_candidates(value, name).related
}

pub(super) async fn disambiguate_spotify_profiles(
    repertoire: &Repertoire,
    candidates: Vec<String>,
) -> Option<String> {
    let titles = &repertoire.recordings;
    // Search already limits the result set to twenty. Limit parallel requests,
    // not the number of same-name profiles that may be checked.
    let mut requests = futures::stream::iter(candidates)
        .map(|url| {
            let titles = titles;
            async move {
                let response = tokio::time::timeout(Duration::from_secs(4), async {
                    let id = url.rsplit('/').next().unwrap_or_default();
                    let html = CATALOG_CLIENT
                        .get(format!("https://open.spotify.com/embed/artist/{id}"))
                        .send()
                        .await
                        .ok()?
                        .error_for_status()
                        .ok()?
                        .text()
                        .await
                        .ok()?;
                    Some(matching_track_score(&html, titles))
                })
                .await
                .ok()
                .flatten();
                response.map(|score| {
                    tracing::debug!("Spotify profile {url}: {score} matching recordings");
                    (score, url)
                })
            }
        })
        .buffer_unordered(4);
    let mut scores = Vec::new();
    while let Some(result) = requests.next().await {
        if let Some(value) = result {
            scores.push(value);
        }
    }
    scores.sort_by(|a, b| b.0.cmp(&a.0));
    let (best, url) = scores.first()?;
    // Two different songs and a unique winner; neither case nor popularity is identity evidence.
    if *best >= 2 && scores.get(1).is_none_or(|(score, _)| score < best) {
        Some(url.clone())
    } else {
        None
    }
}

async fn refresh_query_hash(operation: &str) -> Option<()> {
    let html = CATALOG_CLIENT
        .get(format!(
            "https://open.spotify.com/playlist/{PUBLIC_PLAYLIST}"
        ))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    let script = {
        let doc = scraper::Html::parse_document(&html);
        let selector = scraper::Selector::parse("script[src]").ok()?;
        doc.select(&selector)
            .filter_map(|tag| tag.value().attr("src"))
            .find(|url| {
                url.starts_with(
                    "https://open.spotifycdn.com/cdn/build/mobile-web-player/mobile-web-player.",
                ) && url.ends_with(".js")
            })?
            .to_owned()
    };
    let js = CATALOG_CLIENT
        .get(script)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    let pattern = format!(
        r#"\("{}",\s*"query",\s*"([a-f0-9]{{64}})"#,
        regex::escape(operation)
    );
    let regex = regex::Regex::new(&pattern).ok()?;
    SEARCH_HASH
        .lock()
        .insert(operation.to_owned(), regex.captures(&js)?[1].to_owned());
    Some(())
}

async fn spotify_search_json(operation: &str, query: &str) -> Option<Value> {
    if SEARCH_BLOCKED
        .lock()
        .is_some_and(|until| until > Instant::now())
    {
        return None;
    }
    let mut token = anonymous_search_session().await?;
    let mut refreshed_session = false;
    let mut refreshed_document = false;
    for _ in 0..3 {
        let hash = SEARCH_HASH
            .lock()
            .get(operation)
            .cloned()
            .unwrap_or_else(|| ARTIST_QUERY_HASH.to_owned());
        let body = serde_json::json!({"operationName":operation, "variables":{"query":query,"limit":20,"offset":0},
            "extensions":{"persistedQuery":{"version":1,"sha256Hash":hash}}});
        let response = CATALOG_CLIENT
            .post("https://api-partner.spotify.com/pathfinder/v2/query")
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .ok()?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let delay = response
                .headers()
                .get("retry-after")
                .and_then(|s| s.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            *SEARCH_BLOCKED.lock() = Some(Instant::now() + Duration::from_secs(delay));
            return None;
        }
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            if refreshed_session {
                return None;
            }
            *SEARCH_SESSION.lock().await = None;
            token = anonymous_search_session().await?;
            refreshed_session = true;
            continue;
        }
        if !response.status().is_success() {
            return None;
        }
        let value: Value = response.json().await.ok()?;
        let stale_document = value["errors"].as_array().is_some_and(|errors| {
            errors.iter().any(|e| {
                e["message"].as_str().is_some_and(|s| {
                    s.contains("PersistedQueryNotFound") || s.contains("PersistedQueryNotSupported")
                })
            })
        });
        if !refreshed_document && stale_document && refresh_query_hash(operation).await.is_some() {
            refreshed_document = true;
            continue;
        }
        return Some(value);
    }
    None
}

pub(super) fn recording_search_candidates(
    value: &Value,
    name: &str,
    repertoire: &Repertoire,
) -> Vec<String> {
    let Some(items) = value
        .pointer("/data/searchV2/tracksV2/items")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let wanted = ArtistName::new(name);
    let mut urls = Vec::new();
    for item in items {
        let Some(track) = item.pointer("/item/data") else {
            continue;
        };
        let Some(title) = track["name"].as_str() else {
            continue;
        };
        if !repertoire.recordings.contains_key(&cross_script_key(title)) {
            continue;
        }
        let Some(artists) = track.pointer("/artists/items").and_then(Value::as_array) else {
            continue;
        };
        for artist in artists {
            let Some(artist_name) = artist["profile"]["name"].as_str() else {
                continue;
            };
            let candidate = ArtistName::new(artist_name);
            if candidate.match_score(&wanted) == 0 && candidate.cross_script != wanted.cross_script
            {
                continue;
            }
            if let Some(url) = artist["id"]
                .as_str()
                .and_then(|id| spotify_url(&format!("https://open.spotify.com/artist/{id}")))
            {
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }
    }
    urls
}

async fn search_by_recordings(name: &str, repertoire: &Repertoire) -> Option<String> {
    let mut candidates = Vec::new();
    for title in repertoire.popular_titles.iter().take(3) {
        let Some(value) = spotify_search_json("findTracks", &format!("{name} {title}")).await
        else {
            continue;
        };
        for url in recording_search_candidates(&value, name, repertoire) {
            if !candidates.contains(&url) {
                candidates.push(url);
            }
        }
    }
    if candidates.is_empty() {
        None
    } else {
        disambiguate_spotify_profiles(repertoire, candidates).await
    }
}

pub(super) async fn spotify_public_search(
    name: &str,
    reference: Option<(&crate::http::ApiService, &str)>,
) -> Option<String> {
    let value = spotify_search_json("findArtists", name).await?;
    let candidates = analyse_candidates(&value, name);
    let short_name = normalized_artist_name(name).chars().count() <= 4;
    if !short_name
        && candidates.related.len() == 1
        && candidates.best.len() == 1
        && candidates.related == candidates.best
    {
        return candidates.best.into_iter().next();
    }
    let Some((api, id)) = reference else {
        return if candidates.related.len() == 1
            && candidates.best.len() == 1
            && candidates.related == candidates.best
        {
            candidates.best.into_iter().next()
        } else {
            None
        };
    };
    let repertoire = repertoire_for(api, id).await?;
    if short_name || candidates.best.is_empty() {
        if let Some(url) = search_by_recordings(name, &repertoire).await {
            return Some(url);
        }
    }
    if !candidates.related.is_empty() {
        if let Some(url) = disambiguate_spotify_profiles(&repertoire, candidates.related).await {
            return Some(url);
        }
    }
    if !short_name && !candidates.best.is_empty() {
        search_by_recordings(name, &repertoire).await
    } else {
        None
    }
}

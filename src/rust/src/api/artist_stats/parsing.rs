use serde_json::Value;
use std::{collections::HashMap, sync::LazyLock};
use unicode_normalization::UnicodeNormalization;

static META: LazyLock<scraper::Selector> = LazyLock::new(|| {
    scraper::Selector::parse("meta[name=description], meta[property='og:description']").unwrap()
});
static LISTENERS: LazyLock<scraper::Selector> =
    LazyLock::new(|| scraper::Selector::parse("[data-testid='monthly-listeners-label']").unwrap());
static NEXT_DATA: LazyLock<scraper::Selector> =
    LazyLock::new(|| scraper::Selector::parse("script#__NEXT_DATA__").unwrap());
static JSON_COUNT: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#""monthlyListeners"\s*:\s*"?([0-9]+)"#).unwrap());
static TEXT_COUNT: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)([0-9][0-9,\x{00a0}\x{202f} ]*)\s+monthly listeners").unwrap()
});
static COMPACT: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(?:^|[^0-9.,])([0-9]+(?:[.,][0-9]+)?)\s*([kmb])\s+monthly listeners\b")
        .unwrap()
});

fn text_count(text: &str, compact: bool) -> Option<u64> {
    if compact {
        if let Some(caps) = COMPACT.captures(text) {
            let value: f64 = caps[1].replace(',', ".").parse().ok()?;
            let multiplier = match caps[2].to_ascii_lowercase().as_str() {
                "k" => 1_000.0,
                "m" => 1_000_000.0,
                "b" => 1_000_000_000.0,
                _ => return None,
            };
            let count = value * multiplier;
            return (count.is_finite() && count >= 0.0 && count < u64::MAX as f64)
                .then(|| count.round() as u64);
        }
    }
    let caps = TEXT_COUNT.captures(text)?;
    caps[1]
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn metadata_count(doc: &scraper::Html, compact: bool) -> Option<u64> {
    doc.select(&META).find_map(|element| {
        text_count(element.value().attr("content").unwrap_or_default(), compact)
    })
}

fn exact_fallback(doc: &scraper::Html, html: &str) -> Option<u64> {
    doc.select(&LISTENERS)
        .find_map(|element| text_count(&element.text().collect::<String>(), false))
        .or_else(|| {
            JSON_COUNT
                .captures(html)
                .and_then(|caps| caps[1].parse().ok())
        })
}

/// One DOM parse per examined prefix; descriptions take priority over body data.
pub(super) fn parse_audience(html: &str) -> Option<u64> {
    let doc = scraper::Html::parse_document(html);
    metadata_count(&doc, true).or_else(|| exact_fallback(&doc, html))
}

#[cfg(test)]
pub(super) fn parse_listeners(html: &str) -> Option<u64> {
    let doc = scraper::Html::parse_document(html);
    metadata_count(&doc, false).or_else(|| exact_fallback(&doc, html))
}

#[cfg(test)]
pub(super) fn parse_description_listeners(html: &str) -> Option<u64> {
    metadata_count(&scraper::Html::parse_document(html), true)
}

pub(super) fn parse_embed_state(html: &str) -> Option<Value> {
    let doc = scraper::Html::parse_document(html);
    serde_json::from_str(&doc.select(&NEXT_DATA).next()?.inner_html()).ok()
}

pub(super) fn normalized_artist_name(name: &str) -> String {
    name.nfkd()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub(super) fn spotify_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    if url.host_str()? != "open.spotify.com" {
        return None;
    }
    let parts: Vec<_> = url.path_segments()?.collect();
    let index = parts.iter().position(|part| *part == "artist")?;
    let id = *parts.get(index + 1)?;
    (id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| format!("https://open.spotify.com/artist/{id}"))
}

pub(super) fn find_spotify_link(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => spotify_url(s),
        Value::Array(items) => items.iter().find_map(find_spotify_link),
        Value::Object(items) => items.values().find_map(find_spotify_link),
        _ => None,
    }
}

pub(super) fn spotify_from_hub_html(html: &str) -> Option<String> {
    static LINKS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"https://open\.spotify\.com/(?:intl-[a-z]+/)?artist/[A-Za-z0-9]{22}\b")
            .unwrap()
    });
    let normalized = html.replace(r"\/", "/");
    let urls: std::collections::HashSet<_> = LINKS
        .find_iter(&normalized)
        .filter_map(|link| spotify_url(link.as_str()))
        .collect();
    if urls.len() == 1 {
        urls.into_iter().next()
    } else {
        None
    }
}

pub(super) struct ArtistName {
    exact: String,
    punctuation: String,
    folded: String,
    pub(super) cross_script: String,
    pub(super) cyrillic: bool,
}

impl ArtistName {
    pub(super) fn new(name: &str) -> Self {
        let exact: String = name.trim().nfkc().flat_map(char::to_lowercase).collect();
        let punctuation = exact
            .chars()
            .map(|c| match c {
                '‐' | '‑' | '–' | '—' | '−' => '-',
                c => c,
            })
            .collect();
        Self {
            exact,
            punctuation,
            folded: normalized_artist_name(name),
            cross_script: cross_script_key(name),
            cyrillic: name.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)),
        }
    }
    pub(super) fn match_score(&self, wanted: &Self) -> u8 {
        if self.exact == wanted.exact {
            3
        } else if self.punctuation == wanted.punctuation {
            2
        } else if self.folded == wanted.folded {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
pub(super) fn artist_match_score(candidate: &str, wanted: &str) -> u8 {
    ArtistName::new(candidate).match_score(&ArtistName::new(wanted))
}

pub(super) fn cross_script_key(text: &str) -> String {
    let mut result = String::new();
    for c in text.nfkd().flat_map(char::to_lowercase) {
        let mapped = match c {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' | 'э' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' | 'й' => "i",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "sch",
            'ъ' | 'ь' => "",
            'ы' => "y",
            'ю' => "yu",
            'я' => "ya",
            _ => {
                if c.is_alphanumeric() {
                    result.push(c);
                }
                continue;
            }
        };
        result.push_str(mapped);
    }
    result
}

pub(super) fn matching_track_score(html: &str, titles: &HashMap<String, Vec<u64>>) -> usize {
    let Some(data) = parse_embed_state(html) else {
        return 0;
    };
    let Some(tracks) = data
        .pointer("/props/pageProps/state/data/entity/trackList")
        .and_then(Value::as_array)
    else {
        return 0;
    };
    let names: std::collections::HashSet<_> = tracks
        .iter()
        .filter_map(|track| {
            let key = cross_script_key(track["title"].as_str()?);
            let durations = titles.get(&key)?;
            let duration = track["duration"].as_u64().unwrap_or_default();
            durations
                .iter()
                .any(|reference| {
                    duration == 0 || *reference == 0 || duration.abs_diff(*reference) <= 5000
                })
                .then_some(key)
        })
        .collect();
    names.len()
}

use futures::{StreamExt, stream::FuturesUnordered};
use serde::Deserialize;

use crate::lyrics::{CLIENT, ParsedLine, ParsedWord, lrc};

const SERVERS: &[&str] = &[
    "https://lyricsplus.prjktla.my.id",
    "https://lyricsplus.atomix.one",
    "https://lyricsplus.binimum.org",
    "https://lyricsplus.prjktla.workers.dev",
    "https://lyricsplus-seven.vercel.app",
    "https://lyrics-plus-backend.vercel.app",
];

#[derive(Debug, Deserialize)]
struct LyricsResponse {
    #[serde(rename = "type")]
    sync_type: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    lyrics: Option<Vec<LyricsItem>>,
}

#[derive(Debug, Deserialize)]
struct LyricsItem {
    text: Option<String>,
    time: Option<f64>,
    syllabus: Option<Vec<Syllable>>,
}

#[derive(Debug, Deserialize)]
struct Syllable {
    text: Option<String>,
    time: Option<f64>,
}

/// Converts the provider's own JSON line/syllable format into the shared
/// `ParsedLine` representation.
fn convert_items(items: &[LyricsItem]) -> Option<Vec<ParsedLine>> {
    if items.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for item in items {
        let valid_time = |time: Option<f64>| time.filter(|v| v.is_finite() && *v >= 0.0);
        let start = valid_time(item.time)
            .or_else(|| {
                item.syllabus
                    .as_ref()?
                    .iter()
                    .filter_map(|s| valid_time(s.time))
                    .min_by(f64::total_cmp)
            })
            .map(|v| v / 1000.0)
            .unwrap_or(-1.0);
        let mut words = Vec::new();
        if let Some(syllabus) = &item.syllabus {
            for (i, syl) in syllabus.iter().enumerate() {
                let text = syl.text.as_deref().unwrap_or("").trim();
                let Some(word_start) = valid_time(syl.time).map(|v| v / 1000.0) else {
                    continue;
                };
                if text.is_empty() {
                    continue;
                }
                let word_end = syllabus
                    .get(i + 1)
                    .and_then(|next| valid_time(next.time))
                    .map(|v| v / 1000.0)
                    .filter(|end| *end > word_start)
                    .unwrap_or(word_start + 2.0);
                words.push(ParsedWord {
                    text: text.to_owned(),
                    start: word_start,
                    end: word_end,
                });
            }
        }
        let text = item
            .text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                item.syllabus
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|syl| syl.text.as_deref())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
        if !text.trim().is_empty() {
            lines.push(ParsedLine { start, text, words });
        }
    }
    if lines.is_empty() {
        return None;
    }
    // A plain response commonly carries time:0 for every line. Missing times
    // must not become 0 either: binary search would select the final line.
    if lines.iter().any(|line| line.start < 0.0)
        || (lines.len() > 1 && lines.iter().all(|line| line.start == lines[0].start))
    {
        make_plain(&mut lines);
    } else {
        lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    Some(lines)
}

fn make_plain(lines: &mut [ParsedLine]) {
    for line in lines {
        line.start = -1.0;
        line.words.clear();
    }
}

fn parse_response(parsed: LyricsResponse) -> Option<Vec<ParsedLine>> {
    let plain = parsed.sync_type.as_deref().is_some_and(|kind| {
        matches!(
            kind.to_ascii_lowercase().as_str(),
            "none" | "plain" | "unsynced"
        )
    });
    if !plain {
        if let Some(synced) = parsed
            .synced_lyrics
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            let lines = lrc::parse(synced);
            if !lines.is_empty() {
                return Some(lines);
            }
        }
    }
    if let Some(mut lines) = parsed.lyrics.as_deref().and_then(convert_items) {
        if plain {
            make_plain(&mut lines);
        }
        return Some(lines);
    }
    parsed
        .plain_lyrics
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            let mut lines = lrc::parse(&text);
            make_plain(&mut lines);
            lines
        })
        .filter(|lines| !lines.is_empty())
}

async fn fetch_from_server(
    base_url: &str,
    title: &str,
    artist: &str,
    duration: i32,
    album: Option<&str>,
    id: Option<&str>,
) -> Option<Vec<ParsedLine>> {
    let url = format!("{}/v2/lyrics/get", base_url.trim_end_matches('/'));

    let mut req = CLIENT.get(url).query(&[
        ("title", title),
        ("artist", artist),
        ("duration", &duration.to_string()),
    ]);
    if let Some(album) = album {
        req = req.query(&[("album", album)]);
    }
    if let Some(id) = id {
        req = req.query(&[("id", id)]);
    }

    let resp = req.send().await.ok()?.error_for_status().ok()?;
    let parsed: LyricsResponse = resp.json().await.ok()?;
    parse_response(parsed)
}

pub async fn get_lyrics(
    title: &str,
    artist: &str,
    duration: i32,
    album: Option<&str>,
    id: Option<&str>,
) -> Option<Vec<ParsedLine>> {
    let mut pending: FuturesUnordered<_> = SERVERS
        .iter()
        .map(|server| fetch_from_server(server, title, artist, duration, album, id))
        .collect();
    let mut plain = None;
    while let Some(result) = pending.next().await {
        if let Some(lines) = result {
            if lines.iter().any(|line| line.start >= 0.0) {
                return Some(lines);
            }
            if plain.is_none() {
                plain = Some(lines);
            }
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(json: &str) -> Vec<ParsedLine> {
        parse_response(serde_json::from_str(json).unwrap()).unwrap()
    }

    #[test]
    fn plain_response_with_zero_times_is_not_synced() {
        let lines = response(
            r#"{"type":"None","lyrics":[{"text":"first","time":0},{"text":"last","time":0}]}"#,
        );
        assert_eq!(lines.len(), 2);
        assert!(
            lines
                .iter()
                .all(|line| line.start == -1.0 && line.words.is_empty())
        );
    }

    #[test]
    fn absent_timestamps_do_not_become_zero_or_select_the_last_line() {
        let lines = response(r#"{"lyrics":[{"text":"first"},{"text":"last"}]}"#);
        assert!(lines.iter().all(|line| line.start == -1.0));
        let lines = response(
            r#"{"type":"Line","lyrics":[{"text":"first","time":0},{"text":"last","time":0}]}"#,
        );
        assert!(lines.iter().all(|line| line.start == -1.0));
    }

    #[test]
    fn actual_millisecond_times_are_preserved_and_sorted() {
        let lines = response(
            r#"{"type":"Line","lyrics":[{"text":"last","time":90000},{"text":"first","time":1250.5}]}"#,
        );
        assert_eq!(lines[0].start, 1.2505);
        assert_eq!(lines[1].start, 90.0);
        assert_eq!(lines[0].text, "first");
    }

    #[test]
    fn missing_line_time_can_use_real_word_timing() {
        let lines = response(
            r#"{"type":"Word","lyrics":[{"text":"first","syllabus":[{"text":"first","time":10000}]},{"text":"last","time":30000}]}"#,
        );
        assert_eq!(lines[0].start, 10.0);
        assert_eq!(lines[1].start, 30.0);
    }

    #[test]
    fn explicitly_plain_response_never_exposes_synthetic_timing() {
        let lines = response(
            r#"{"type":"Plain","lyrics":[{"text":"first","time":1000},{"text":"last","time":2000}]}"#,
        );
        assert!(lines.iter().all(|line| line.start == -1.0));
    }

    #[tokio::test]
    #[ignore = "requires external YouLyPlus mirrors"]
    async fn live_youlyplus_timing() {
        let lines = get_lyrics("Believer", "Imagine Dragons", 204, None, None)
            .await
            .expect("YouLyPlus response");
        let last = lines.iter().map(|line| line.start).fold(-1.0, f64::max);
        println!("YouLyPlus: {} lines, last timestamp {}s", lines.len(), last);
        assert!(lines.iter().all(|line| line.start < 0.0) || last > 30.0);
    }

    fn syllable(text: &str, time: i64) -> Syllable {
        Syllable {
            text: Some(text.to_string()),
            time: Some(time as f64),
        }
    }

    #[test]
    fn convert_items_plain_line_without_syllables() {
        let items = vec![LyricsItem {
            text: Some("hello world".to_string()),
            time: Some(1_000.0),
            syllabus: None,
        }];
        let lines = convert_items(&items).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].start, 1.0);
        assert_eq!(lines[0].text, "hello world");
        assert!(lines[0].words.is_empty());
    }

    #[test]
    fn convert_items_builds_words_from_syllables() {
        let items = vec![LyricsItem {
            text: None,
            time: Some(0.0),
            syllabus: Some(vec![syllable("hel", 0), syllable("lo", 300)]),
        }];
        let lines = convert_items(&items).unwrap();
        assert_eq!(lines[0].text, "hel lo");
        assert_eq!(lines[0].words.len(), 2);
        assert_eq!(lines[0].words[0].start, 0.0);
        assert_eq!(lines[0].words[0].end, 0.3);
        // Last syllable has no closing tag, so its end is estimated.
        assert_eq!(lines[0].words[1].end, 2.3);
    }

    #[test]
    fn convert_items_returns_none_for_empty_items() {
        assert_eq!(convert_items(&[]), None);
    }
}

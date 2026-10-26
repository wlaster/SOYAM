use serde::Deserialize;

use super::{clean_artist, clean_title, string_similarity};
use crate::lyrics::{ParsedLine, lrc};
use std::{io::Write, sync::LazyLock, time::Duration};

// Decode each received gzip chunk ourselves: a buffered HTTP decoder can wait
// for more network bytes even when the complete LRC prefix has already arrived.
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let builder = reqwest::Client::builder();
    #[cfg(target_os = "windows")]
    let builder = builder.tls_backend_native();
    builder
        .http1_only()
        .pool_max_idle_per_host(0)
        .gzip(false)
        .brotli(false)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .user_agent("SOYAM/2.5.2 (https://github.com/wlaster/SOYAM)")
        .build()
        .expect("lyrics HTTP client")
});

struct Body {
    gzip: Option<flate2::write::GzDecoder<Vec<u8>>>,
    plain: Vec<u8>,
}
impl Body {
    fn new(response: &reqwest::Response) -> Self {
        Self {
            gzip: response
                .headers()
                .get("content-encoding")
                .is_some_and(|value| value == "gzip")
                .then(|| flate2::write::GzDecoder::new(Vec::new())),
            plain: Vec::new(),
        }
    }
    fn push(&mut self, chunk: &[u8]) -> Option<()> {
        if let Some(decoder) = &mut self.gzip {
            decoder.write_all(chunk).ok()?;
            decoder.flush().ok()?;
        } else {
            self.plain.extend_from_slice(chunk);
        }
        (self.bytes().len() <= 2 * 1024 * 1024).then_some(())
    }
    fn bytes(&self) -> &[u8] {
        self.gzip.as_ref().map_or(self.plain.as_slice(), |decoder| {
            decoder.get_ref().as_slice()
        })
    }
}

#[derive(Debug, Deserialize)]
struct Track {
    #[serde(rename = "trackName")]
    track_name: String,
    #[serde(rename = "artistName")]
    artist_name: String,
    duration: f64,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

// Stop once complete metadata/plain/LRC fields have arrived. Recent responses
// append a large lyricsfile field; a stalled tail must not discard complete
// lyric fields already received. Every consumed key/value is parsed by serde.
fn track_prefix(bytes: &[u8]) -> Option<Track> {
    let mut offset = 0;
    let skip_space = |offset: &mut usize| {
        while bytes.get(*offset).is_some_and(u8::is_ascii_whitespace) {
            *offset += 1;
        }
    };
    skip_space(&mut offset);
    if bytes.get(offset) != Some(&b'{') {
        return None;
    }
    offset += 1;
    let mut fields = serde_json::Map::new();
    loop {
        skip_space(&mut offset);
        let mut key_stream =
            serde_json::Deserializer::from_slice(bytes.get(offset..)?).into_iter::<String>();
        let key = key_stream.next()?.ok()?;
        offset += key_stream.byte_offset();
        skip_space(&mut offset);
        if bytes.get(offset) != Some(&b':') {
            return None;
        }
        offset += 1;
        skip_space(&mut offset);
        let mut value_stream = serde_json::Deserializer::from_slice(bytes.get(offset..)?)
            .into_iter::<serde_json::Value>();
        let value = value_stream.next()?.ok()?;
        offset += value_stream.byte_offset();
        skip_space(&mut offset);
        let delimiter = *bytes.get(offset)?;
        if !matches!(delimiter, b',' | b'}') {
            return None;
        }
        fields.insert(key, value);
        if [
            "trackName",
            "artistName",
            "duration",
            "plainLyrics",
            "syncedLyrics",
        ]
        .iter()
        .all(|key| fields.contains_key(*key))
        {
            return serde_json::from_value(serde_json::Value::Object(fields)).ok();
        }
        if delimiter == b'}' {
            return None;
        }
        offset += 1;
    }
}

async fn read_track(mut response: reqwest::Response) -> Option<Track> {
    let mut body = Body::new(&response);
    while let Some(chunk) = response.chunk().await.ok()? {
        body.push(&chunk)?;
        if let Some(track) = track_prefix(body.bytes()) {
            return Some(track);
        }
    }
    serde_json::from_slice(body.bytes()).ok()
}

// Search responses can also carry large unused lyricsfile fields. Read complete
// metadata/LRC prefixes as they arrive, without waiting for the unused tail.
fn search_prefix(bytes: &[u8]) -> Vec<Track> {
    let mut starts = Vec::new();
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for (offset, &byte) in bytes.iter().enumerate() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' => {
                    if depth == 0 {
                        starts.push(offset);
                    }
                    depth += 1;
                }
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    starts
        .into_iter()
        .filter_map(|start| track_prefix(&bytes[start..]))
        .collect()
}

async fn read_search(
    mut response: reqwest::Response,
    title: &str,
    artist: &str,
    duration: i32,
) -> Vec<Track> {
    let mut body = Body::new(&response);
    let mut tracks = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if body.push(&chunk).is_none() {
            break;
        }
        tracks = search_prefix(body.bytes());
        if tracks.iter().any(|track| {
            metadata_matches(track, title, artist)
                && (duration <= 0 || (track.duration - duration as f64).abs() <= 5.0)
                && track
                    .synced_lyrics
                    .as_deref()
                    .is_some_and(|text| lrc::parse(text).iter().any(|line| line.start >= 0.0))
        }) {
            return tracks;
        }
    }
    tracks
}

async fn search(
    query: Option<&str>,
    track_name: Option<&str>,
    artist_name: Option<&str>,
    album_name: Option<&str>,
    title: &str,
    artist: &str,
    duration: i32,
) -> Vec<Track> {
    let mut req = CLIENT
        .get("https://lrclib.net/api/search")
        .header("Accept-Encoding", "gzip")
        .header(
            "User-Agent",
            "SOYAM/2.5.2 (https://github.com/wlaster/SOYAM)",
        );
    if let Some(q) = query {
        req = req.query(&[("q", q)]);
    }
    if let Some(t) = track_name {
        req = req.query(&[("track_name", t)]);
    }
    if let Some(a) = artist_name {
        req = req.query(&[("artist_name", a)]);
    }
    if let Some(a) = album_name {
        req = req.query(&[("album_name", a)]);
    }

    let Ok(resp) = req
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
    else {
        return Vec::new();
    };
    read_search(resp, title, artist, duration).await
}

async fn query_lyrics(artist: &str, title: &str, album: Option<&str>, duration: i32) -> Vec<Track> {
    let cleaned_title = clean_title(title);
    let cleaned_artist = clean_artist(artist);
    let has_lyrics = |t: &Track| {
        t.synced_lyrics
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
            || t.plain_lyrics
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
    };
    let combined = format!("{cleaned_artist} {cleaned_title}");
    let trimmed_title = title.trim();
    let trimmed_artist = artist.trim();

    // Progressively looser fallbacks, tried in priority order until one yields lyrics.
    let mut attempts = vec![
        (
            None,
            Some(cleaned_title.as_str()),
            Some(cleaned_artist.as_str()),
            album,
        ),
        (
            None,
            Some(cleaned_title.as_str()),
            Some(cleaned_artist.as_str()),
            None,
        ),
        (None, Some(cleaned_title.as_str()), None, None),
        (Some(combined.as_str()), None, None, None),
        (Some(cleaned_title.as_str()), None, None, None),
    ];
    if cleaned_title != trimmed_title {
        attempts.push((None, Some(trimmed_title), Some(trimmed_artist), None));
    }

    let mut plain = Vec::new();
    for (query, track_name, artist_name, album_name) in attempts {
        let mut results = search(
            query,
            track_name,
            artist_name,
            album_name,
            title,
            artist,
            duration,
        )
        .await;
        results.retain(has_lyrics);
        // Title-only searches must not return another artist's song just
        // because its duration happens to match.
        results.retain(|t| metadata_matches(t, &cleaned_title, &cleaned_artist));
        if results.iter().any(|t| {
            t.synced_lyrics
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
        }) {
            return results;
        }
        if plain.is_empty() {
            plain = results;
        }
    }

    plain
}

fn metadata_matches(track: &Track, title: &str, artist: &str) -> bool {
    string_similarity(&clean_title(title), &clean_title(&track.track_name)) >= 0.7
        && string_similarity(&clean_artist(artist), &clean_artist(&track.artist_name)) >= 0.6
}

fn best_matching(tracks: &[Track], duration: i32, title: &str, artist: &str) -> Option<usize> {
    if tracks.is_empty() {
        return None;
    }

    if duration <= 0 {
        let normalized_title = title.trim().to_lowercase();
        let normalized_artist = artist.trim().to_lowercase();
        // Rank by title/artist similarity, with a bonus for having synced
        // lyrics so it wins ties against an
        // equally-similar plain-lyrics-only match.
        let score = |t: &Track| {
            let base = (string_similarity(&normalized_title, &t.track_name)
                + string_similarity(&normalized_artist, &t.artist_name))
                / 2.0;
            if t.synced_lyrics.is_some() {
                base + 0.01
            } else {
                base
            }
        };
        return tracks
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| score(a).total_cmp(&score(b)))
            .map(|(i, _)| i);
    }

    let within_tolerance = |t: &&Track| (t.duration as i32 - duration).abs() <= 5;

    // Prefer the closest line-synced version within +-5s.
    let synced_best = || {
        tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.synced_lyrics.is_some())
            .min_by_key(|(_, t)| (t.duration as i32 - duration).abs())
            .filter(|(_, t)| within_tolerance(t))
            .map(|(i, _)| i)
    };

    synced_best().or_else(|| {
        tracks
            .iter()
            .enumerate()
            .min_by_key(|(_, t)| (t.duration as i32 - duration).abs())
            .filter(|(_, t)| within_tolerance(t))
            .map(|(i, _)| i)
    })
}

pub async fn get_lyrics(
    title: &str,
    artist: &str,
    duration: i32,
    album: Option<&str>,
) -> Option<Vec<ParsedLine>> {
    // Prefer the small exact-match response over a search array containing
    // many complete lyric files (which can stall on restricted connections).
    let mut request = CLIENT
        .get("https://lrclib.net/api/get")
        .header("Accept-Encoding", "gzip")
        .header(
            "User-Agent",
            "SOYAM/2.5.2 (https://github.com/wlaster/SOYAM)",
        )
        .query(&[
            ("track_name", clean_title(title)),
            ("artist_name", clean_artist(artist)),
        ]);
    if duration > 0 && duration <= 3600 {
        request = request.query(&[("duration", duration)]);
    }
    let mut plain = None;
    if let Ok(response) = request.send().await {
        if response.status().is_success() {
            if let Some(track) = read_track(response).await {
                if metadata_matches(&track, title, artist)
                    && (duration <= 0 || (track.duration - duration as f64).abs() <= 5.0)
                {
                    if let Some(lines) = track_lines(&track) {
                        if lines.iter().any(|line| line.start >= 0.0) {
                            return Some(lines);
                        }
                        plain = Some(lines);
                    }
                }
            }
        }
    }
    let mut tracks = tokio::time::timeout(
        std::time::Duration::from_secs(7),
        query_lyrics(artist, title, album, duration),
    )
    .await
    .unwrap_or_default();
    tracks.retain(|track| metadata_matches(track, title, artist));
    best_matching(&tracks, duration, title, artist)
        .and_then(|index| track_lines(&tracks[index]))
        .or(plain)
}

fn track_lines(track: &Track) -> Option<Vec<ParsedLine>> {
    let lines = track
        .synced_lyrics
        .as_deref()
        .map(lrc::parse)
        .filter(|lines| !lines.is_empty())
        .or_else(|| track.plain_lyrics.as_deref().map(lrc::parse))?;
    if lines.is_empty() { None } else { Some(lines) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_reads_lyric_prefix_before_unused_tail_and_handles_braces_in_text() {
        let bytes = br#"[{"trackName":"Title","artistName":"Artist","duration":200,"plainLyrics":"A {verse}","syncedLyrics":"[00:01]A {verse}","lyricsfile":"unfinished"#;
        let tracks = search_prefix(bytes);
        assert_eq!(tracks.len(), 1);
        assert_eq!(track_lines(&tracks[0]).unwrap()[0].start, 1.0);
    }

    #[tokio::test]
    #[ignore = "requires an internet connection"]
    async fn live_kukla_synced() {
        let lines = get_lyrics(
            "Кукла колдуна",
            "Король и Шут",
            203,
            Some("Акустический альбом"),
        )
        .await
        .expect("no lyrics");
        assert!(
            lines.iter().filter(|line| line.start >= 0.0).count() > 10,
            "expected real timestamps"
        );
        println!("Kukla: {} timed lines", lines.len());
    }

    #[test]
    fn complete_lyric_fields_survive_a_stalled_unused_tail() {
        let bytes = br#"{"trackName":"Title","artistName":"Artist","duration":200,"plainLyrics":"First\nSecond","syncedLyrics":"[00:01]First\n[00:02]Second","lyricsfile":"unfinished"#;
        let track = track_prefix(bytes).unwrap();
        let lines = track_lines(&track).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].start, 2.0);
    }

    #[test]
    fn incomplete_lyric_field_is_never_accepted() {
        let bytes = br#"{"trackName":"Title","artistName":"Artist","duration":200,"plainLyrics":"First","syncedLyrics":"[00:01]Fir"#;
        assert!(track_prefix(bytes).is_none());
        assert!(track_prefix(b"<html>blocked</html>").is_none());
    }

    fn track(name: &str, artist: &str, duration: f64, synced: bool, plain: bool) -> Track {
        Track {
            track_name: name.to_string(),
            artist_name: artist.to_string(),
            duration,
            plain_lyrics: plain.then(|| "plain".to_string()),
            synced_lyrics: synced.then(|| "[00:01.00]synced".to_string()),
        }
    }

    fn word_synced_track(name: &str, artist: &str, duration: f64) -> Track {
        Track {
            track_name: name.to_string(),
            artist_name: artist.to_string(),
            duration,
            plain_lyrics: None,
            synced_lyrics: Some("[00:01.00]<00:01.00>hello <00:01.40>world".to_string()),
        }
    }

    #[test]
    fn returns_none_for_empty_tracks() {
        assert_eq!(best_matching(&[], 200, "Title", "Artist"), None);
    }

    #[test]
    fn rejects_different_artist_even_with_identical_duration_and_title() {
        let wrong = track("Song", "Unrelated Performer", 200.0, true, true);
        assert!(!metadata_matches(&wrong, "Song", "Original Artist"));
        let correct = track(
            "Song (Remastered)",
            "Original Artist feat. Guest",
            200.0,
            true,
            true,
        );
        assert!(metadata_matches(&correct, "Song", "Original Artist"));
    }

    #[test]
    fn prefers_synced_lyrics_within_duration_tolerance() {
        let tracks = [
            track("Title", "Artist", 200.0, false, true),
            track("Title", "Artist", 201.0, true, true),
        ];
        assert_eq!(best_matching(&tracks, 200, "Title", "Artist"), Some(1));
    }

    #[test]
    fn falls_back_to_any_track_within_tolerance_when_none_synced() {
        let tracks = [track("Title", "Artist", 203.0, false, true)];
        assert_eq!(best_matching(&tracks, 200, "Title", "Artist"), Some(0));
    }

    #[test]
    fn returns_none_when_no_track_is_within_duration_tolerance() {
        let tracks = [track("Title", "Artist", 400.0, true, true)];
        assert_eq!(best_matching(&tracks, 200, "Title", "Artist"), None);
    }

    #[test]
    fn unknown_duration_picks_best_title_artist_similarity_match() {
        let tracks = [
            track(
                "Completely Unrelated Song",
                "Some Other Band",
                0.0,
                false,
                true,
            ),
            track("Exact Title", "Exact Artist", 0.0, false, true),
        ];
        assert_eq!(
            best_matching(&tracks, -1, "Exact Title", "Exact Artist"),
            Some(1)
        );
    }

    #[test]
    fn unknown_duration_breaks_similarity_ties_toward_synced_lyrics() {
        let tracks = [
            track("Exact Title", "Exact Artist", 0.0, false, true),
            track("Exact Title", "Exact Artist", 0.0, true, true),
        ];
        assert_eq!(
            best_matching(&tracks, -1, "Exact Title", "Exact Artist"),
            Some(1)
        );
    }

    #[test]
    fn closest_synced_submission_wins_regardless_of_word_timestamps() {
        let tracks = [
            track("Title", "Artist", 200.0, true, false),
            word_synced_track("Title", "Artist", 201.0),
        ];
        assert_eq!(best_matching(&tracks, 200, "Title", "Artist"), Some(0));
    }

    #[test]
    fn falls_back_to_plain_synced_when_no_word_synced_submission_in_tolerance() {
        let tracks = [
            track("Title", "Artist", 200.0, true, false),
            word_synced_track("Title", "Artist", 400.0),
        ];
        assert_eq!(best_matching(&tracks, 200, "Title", "Artist"), Some(0));
    }
}

pub mod lrc;
pub mod providers;
pub mod ttml;

use crate::api::models::{LyricsLineDto};
use std::sync::LazyLock;
use std::time::Duration;

/// A single parsed lyric line with optional word-level (karaoke) timing, in
/// seconds. The common intermediate representation every provider converges
/// on before crossing the FRB boundary as [`LyricsLineDto`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLine {
    pub start: f64,
    pub text: String,
    pub words: Vec<ParsedWord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Sorts by start time and converts to the FRB-exposed DTO shape (millisecond ints).
pub fn to_lines_dto(mut lines: Vec<ParsedLine>) -> Vec<LyricsLineDto> {
    lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    lines
        .into_iter()
        .map(|line| LyricsLineDto {
            start_ms: (line.start * 1000.0).round() as i64,
            text: line.text,
            words: Vec::new(),
        })
        .collect()
}

/// Shared HTTP client for every third-party lyrics provider, so the app
/// keeps one connection pool instead of one per source. A browser-like
/// User-Agent is required — some providers (e.g. lrclib.net) sit behind a
/// WAF that 403s reqwest's default UA string.
pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let builder = reqwest::Client::builder();
    #[cfg(target_os = "windows")]
    let builder = builder.tls_backend_native();
    builder
        .http1_only()
        .brotli(false)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .build()
        .unwrap_or_default()
});

/// A track's search-relevant metadata, used to query third-party providers
/// that can't look tracks up by Yandex track id.
#[derive(Debug, Clone)]
pub struct LyricsQuery {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Duration in whole seconds, or -1 if unknown.
    pub duration: i32,
    pub album: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    Yandex,
    LrcLib,
    BetterLyrics,
    YouLyPlus,
    LyricsOvh,
    Genius,
}

impl ProviderId {
    /// Primary sources first, then concurrently queried fallbacks.
    /// Also used to list the enable/disable settings.
    pub const ALL: [ProviderId; 6] = [
        ProviderId::Yandex,
        ProviderId::LrcLib,
        ProviderId::LyricsOvh,
        ProviderId::Genius,
        ProviderId::BetterLyrics,
        ProviderId::YouLyPlus,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            ProviderId::Yandex => "yandex",
            ProviderId::LrcLib => "lrclib",
            ProviderId::BetterLyrics => "betterlyrics",
            ProviderId::YouLyPlus => "youlyplus",
            ProviderId::LyricsOvh => "lyricsovh",
            ProviderId::Genius => "genius",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::Yandex => "Яндекс Музыка",
            ProviderId::LrcLib => "LrcLib",
            ProviderId::BetterLyrics => "BetterLyrics",
            ProviderId::YouLyPlus => "YouLyPlus",
            ProviderId::LyricsOvh => "Lyrics.ovh",
            ProviderId::Genius => "Genius",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

}

/// Queries a single third-party provider (everything except [`ProviderId::Yandex`],
/// which needs the authenticated `ApiService` and is handled by the caller).
pub async fn fetch_from_provider(
    provider: ProviderId,
    query: &LyricsQuery,
) -> Option<Vec<ParsedLine>> {
    let album = query.album.as_deref();
    match provider {
        ProviderId::Yandex => None,
        ProviderId::Genius => providers::genius::get_lyrics(&query.title, &query.artist).await,
        ProviderId::LyricsOvh => {
            providers::lyricsovh::get_lyrics(&query.title, &query.artist).await
        }
        ProviderId::LrcLib => {
            providers::lrclib::get_lyrics(&query.title, &query.artist, query.duration, album).await
        }
        ProviderId::BetterLyrics => {
            providers::betterlyrics::get_lyrics(&query.title, &query.artist, query.duration, album)
                .await
        }
        ProviderId::YouLyPlus => {
            providers::youlyplus::get_lyrics(
                &query.title,
                &query.artist,
                query.duration,
                album,
                Some(&query.id),
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_lines_dto_sorts_by_start_and_converts_to_milliseconds() {
        let lines = vec![
            ParsedLine {
                start: 2.0,
                text: "second".to_string(),
                words: Vec::new(),
            },
            ParsedLine {
                start: 1.0,
                text: "first".to_string(),
                words: vec![ParsedWord {
                    text: "first".to_string(),
                    start: 1.0,
                    end: 1.5,
                }],
            },
        ];
        let dto = to_lines_dto(lines);
        assert_eq!(dto[0].text, "first");
        assert_eq!(dto[0].start_ms, 1000);
        assert!(dto.iter().all(|line| line.words.is_empty()));
        assert_eq!(dto[1].text, "second");
        assert_eq!(dto[1].start_ms, 2000);
    }

    #[test]
    fn provider_key_round_trips_through_from_key() {
        for provider in ProviderId::ALL {
            assert_eq!(ProviderId::from_key(provider.key()), Some(provider));
        }
    }

    #[test]
    fn from_key_rejects_unknown_keys() {
        assert_eq!(ProviderId::from_key("not-a-real-provider"), None);
    }

    #[test]
    fn untimed_dto_keeps_original_order_and_negative_timestamp() {
        let lines = lrc::parse("Первый куплет\nВторая строка");
        let dto = to_lines_dto(lines);
        assert_eq!(dto[0].text, "Первый куплет");
        assert_eq!(dto[1].text, "Вторая строка");
        assert!(
            dto.iter()
                .all(|line| line.start_ms == -1000 && line.words.is_empty())
        );
    }


    async fn check_live_source(provider: ProviderId, title: &str, artist: &str) {
        let query = LyricsQuery {
            id: String::new(),
            title: title.into(),
            artist: artist.into(),
            duration: -1,
            album: None,
        };
        let lines = tokio::time::timeout(
            Duration::from_secs(15),
            fetch_from_provider(provider, &query),
        )
        .await
        .expect("lyrics source timed out")
        .expect("lyrics source returned no usable text");
        assert!(lines.iter().any(|line| !line.text.trim().is_empty()));
        println!("{}: {} parsed lines", provider.display_name(), lines.len());
    }

    #[tokio::test]
    #[ignore = "requires an internet connection"]
    async fn live_lrclib_russian_track() {
        check_live_source(ProviderId::LrcLib, "Группа крови", "Кино").await;
    }


    #[tokio::test]
    #[ignore = "requires an internet connection"]
    async fn live_lyricsovh() {
        check_live_source(ProviderId::LyricsOvh, "Believer", "Imagine Dragons").await;
    }

    #[tokio::test]
    #[ignore = "requires an internet connection"]
    async fn live_genius() {
        check_live_source(ProviderId::Genius, "Believer", "Imagine Dragons").await;
    }

    #[tokio::test]
    #[ignore = "requires an internet connection"]
    async fn live_all_public_sources() {
        use futures::future::join_all;
        let checks = ProviderId::ALL.into_iter().filter(|p| *p != ProviderId::Yandex).map(|provider| async move {
            let query = LyricsQuery { id: String::new(), title: "Believer".into(), artist: "Imagine Dragons".into(), duration: 204, album: None };
            let result = tokio::time::timeout(Duration::from_secs(20), fetch_from_provider(provider, &query)).await;
            match result {
                Ok(Some(lines)) if lines.iter().any(|line| !line.text.trim().is_empty()) => {
                    let dto = to_lines_dto(lines);
                    assert!(dto.iter().all(|line| line.words.is_empty()));
                    let timed = dto.iter().filter(|line| line.start_ms >= 0).count();
                    println!("SOURCE {}: {} lines; {} timed; last={}ms", provider.display_name(), dto.len(), timed, dto.last().unwrap().start_ms);
                }
                Ok(_) => println!("SOURCE {}: unavailable or no matching lyrics", provider.display_name()),
                Err(_) => println!("SOURCE {}: timeout", provider.display_name()),
            }
        });
        join_all(checks).await;
    }

    #[tokio::test]
    #[ignore = "requires an internet connection"]
    async fn live_connectivity_diagnostics() {
        let urls = std::env::var("LYRICS_DIAGNOSTIC_URLS").unwrap_or_else(|_| "https://genius.com/api/search/multi?q=Imagine%20Dragons%20Believer&per_page=3|https://genius.com/Imagine-dragons-believer-lyrics|https://lrclib.net/api/search?track_name=Believer&artist_name=Imagine%20Dragons".into());
        for url in urls.split('|') {
            let begin = std::time::Instant::now();
            let mut request = CLIENT.get(url);
            if let Ok(agent) = std::env::var("LYRICS_DIAGNOSTIC_AGENT") {
                request = request.header("User-Agent", agent);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    println!(
                        "{url}: {status}; bytes={}; original={}; containers={}; elapsed={:?}",
                        body.len(),
                        body.matches("class=\"original\"").count(),
                        body.matches("data-lyrics-container=\"true\"").count(),
                        begin.elapsed()
                    );
                }
                Err(error) => println!("{url}: {error:?}; elapsed={:?}", begin.elapsed()),
            }
        }
    }
}

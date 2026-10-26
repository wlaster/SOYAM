//! Public Genius search and song pages. Genius supplies no timing data.
use super::{clean_artist, clean_title, string_similarity};
use crate::lyrics::{CLIENT, ParsedLine, lrc};
use scraper::{Html, Selector};
use serde_json::Value;

fn choose_url(data: &Value, title: &str, artist: &str) -> Option<String> {
    data["response"]["sections"]
        .as_array()?
        .iter()
        .filter(|section| section["type"] == "song")
        .flat_map(|section| section["hits"].as_array().into_iter().flatten())
        .filter_map(|hit| {
            let song = &hit["result"];
            let name = song["title"].as_str()?;
            let performer = song["primary_artist"]["name"].as_str()?;
            let title_score = string_similarity(&clean_title(title), &clean_title(name));
            let artist_score = string_similarity(&clean_artist(artist), &clean_artist(performer));
            if title_score < 0.7 || artist_score < 0.6 {
                return None;
            }
            let url = reqwest::Url::parse(song["url"].as_str()?).ok()?;
            if url.scheme() != "https"
                || url.host_str() != Some("genius.com")
                || !url.path().ends_with("-lyrics")
            {
                return None;
            }
            Some((title_score + artist_score, url.to_string()))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, url)| url)
}

fn parse_html(html: &str) -> Option<Vec<ParsedLine>> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("[data-lyrics-container=\"true\"]").ok()?;
    let mut text = String::new();
    for container in document.select(&selector) {
        for node in container.descendants() {
            if node.ancestors().any(|parent| {
                parent.value().as_element().is_some_and(|e| {
                    e.attr("data-exclude-from-selection") == Some("true")
                        || matches!(e.name(), "script" | "style")
                })
            }) {
                continue;
            }
            if let Some(value) = node.value().as_text() {
                text.push_str(value);
            }
            if node.value().as_element().is_some_and(|e| e.name() == "br") {
                text.push('\n');
            }
        }
        text.push('\n');
    }
    let lines = lrc::parse(&text);
    (!lines.is_empty()).then_some(lines)
}

pub async fn get_lyrics(title: &str, artist: &str) -> Option<Vec<ParsedLine>> {
    let query = format!("{} {}", clean_artist(artist), clean_title(title));
    let response = CLIENT
        .get("https://genius.com/api/search/multi")
        .query(&[("q", query.as_str()), ("per_page", "5")])
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let url = choose_url(&response.json::<Value>().await.ok()?, title, artist)?;
    let html = CLIENT
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    parse_html(&html)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_breaks_inline_links_and_skips_headers() {
        let html = r#"<div>Advertisement</div><div data-lyrics-container="true">
          <div data-exclude-from-selection="true">Contributors</div>First <a>line</a><br/>Second &amp; third
          </div><div data-lyrics-container="true">Last line</div>"#;
        let lines = parse_html(html).unwrap();
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["First line", "Second & third", "Last line"]
        );
        assert!(lines.iter().all(|line| line.start < 0.0));
        assert!(parse_html("<h1>Access denied</h1>").is_none());
    }
    #[test]
    fn rejects_wrong_artist_and_external_urls() {
        let data = serde_json::json!({"response":{"sections":[{"type":"song","hits":[
            {"result":{"title":"Believer","primary_artist":{"name":"Wrong Band"},"url":"https://genius.com/wrong-lyrics"}},
            {"result":{"title":"Believer","primary_artist":{"name":"Imagine Dragons"},"url":"https://example.com/fake-lyrics"}},
            {"result":{"title":"Believer","primary_artist":{"name":"Imagine Dragons"},"url":"https://genius.com/Imagine-dragons-believer-lyrics"}}
        ]}]}});
        assert_eq!(
            choose_url(&data, "Believer", "Imagine Dragons").as_deref(),
            Some("https://genius.com/Imagine-dragons-believer-lyrics")
        );
    }
}

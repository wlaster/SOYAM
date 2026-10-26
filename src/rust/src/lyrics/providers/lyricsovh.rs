use super::{clean_artist, clean_title};
use crate::lyrics::{CLIENT, ParsedLine, lrc};
use serde::Deserialize;

#[derive(Deserialize)]
struct Response {
    lyrics: String,
}

pub async fn get_lyrics(title: &str, artist: &str) -> Option<Vec<ParsedLine>> {
    let artist = clean_artist(artist);
    let title = clean_title(title);
    if artist.is_empty() || title.is_empty() {
        return None;
    }
    let url = format!(
        "https://api.lyrics.ovh/v1/{}/{}",
        urlencoding::encode(&artist),
        urlencoding::encode(&title),
    );
    let response = CLIENT.get(url).send().await.ok()?.error_for_status().ok()?;
    let data = response.json::<Response>().await.ok()?;
    let lines = lrc::parse(&data.lyrics);
    (!lines.is_empty()).then_some(lines)
}

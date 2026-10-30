use std::time::Duration;
use yandex_music::model::artist::Artist;
use yandex_music::model::playlist::PlaylistTracks;
use yandex_music::model::track::Track;

/// Builds a minimal `Track` from locally cached DB metadata, for offline playback
/// where we can't hit the network to fetch the full track object.
pub fn track_from_metadata(m: &crate::storage::db::TrackMetadata) -> Track {
    let artists: Vec<Artist> = m
        .artists
        .iter()
        .map(|a| Artist {
            id: Some(a.id.clone()),
            error: None,
            reason: None,
            name: Some(a.name.clone()),
            cover: None,
            various: None,
            composer: None,
            genres: None,
            og_image: None,
            op_image: None,
            counts: None,
            available: None,
            ratings: None,
            links: Vec::new(),
            tickets_available: None,
            likes_count: None,
            popular_tracks: Vec::new(),
            regions: Vec::new(),
            decomposed: Vec::new(),
            description: None,
            countries: Vec::new(),
            en_wikipedia_link: None,
            db_aliases: Vec::new(),
            aliases: Vec::new(),
            init_date: None,
            end_date: None,
        })
        .collect();

    Track {
        id: m.id.clone(),
        title: Some(m.title.clone()),
        // The metadata cache does not store availability. An unknown value
        // must not overwrite a confirmed server-side substitution.
        available: if crate::external_music::is_unavailable(&m.id) { Some(false) } else { None },
        artists,
        albums: if m.album_id.is_some() || m.album.is_some() {
            serde_json::from_value(serde_json::json!({
                "id": m.album_id, "title": m.album, "coverUri": m.cover_url,
            })).ok().into_iter().collect()
        } else { Vec::new() },
        available_for_premium_users: None,
        lyrics_available: None,
        best: None,
        real_id: m.id.clone(),
        og_image: None,
        item_type: None,
        cover_uri: m.cover_url.clone(),
        major: None,
        duration: Some(std::time::Duration::from_millis(m.duration_ms)),
        storage_dir: None,
        file_size: None,
        substituted: None,
        matched_track: None,
        normalization: Vec::new(),
        error: None,
        can_publish: None,
        state: None,
        desired_visibility: None,
        filename: None,
        user_info: None,
        meta_data: None,
        regions: Vec::new(),
        available_as_rbt: None,
        content_warning: None,
        explicit: m.is_explicit,
        preview_duration: None,
        available_full_without_permission: None,
        version: m.version.clone(),
        remember_position: None,
        background_video_uri: None,
        short_description: None,
        is_suitable_for_children: None,
        track_source: None,
        available_for_options: Vec::new(),
        r128: None,
        lyrics_info: None,
        track_sharing_flag: None,
        disclaimers: Vec::new(),
        derived_colors: None,
        fade: None,
        special_audio_resources: Vec::new(),
        player_id: None,
        play_count: None,
    }
}

pub trait CleanId {
    fn to_base_id(&self) -> &str;
}

impl CleanId for String {
    fn to_base_id(&self) -> &str {
        self.split(':').next().unwrap_or(self)
    }
}

impl CleanId for str {
    fn to_base_id(&self) -> &str {
        self.split(':').next().unwrap_or(self)
    }
}

/// Builds the `track-ids` entity id for like/unlike HTTP requests.
///
/// Mirrors the original web client (`toggleTrackLike`: `albumId ? "id:albumId" : id`):
/// the backend keys album versions separately, so the full `"id:album"` pair must
/// go over HTTP, while the local [`LikedCache`](crate::audio::liked::LikedCache)/DB
/// keep keying by base id via [`CleanId::to_base_id`].
pub fn like_entity_id(track_id: &str, album_id: Option<&str>) -> String {
    // Already a composite entity id — send as is, never double-append.
    if track_id.contains(':') {
        return track_id.to_string();
    }
    match album_id.map(str::trim).filter(|a| !a.is_empty()) {
        Some(album) => format!("{}:{}", track_id.to_base_id(), album),
        None => track_id.to_string(),
    }
}

pub fn extract_ids(playlist_tracks: &PlaylistTracks) -> Vec<String> {
    match playlist_tracks {
        PlaylistTracks::Full(tracks) => tracks
            .iter()
            .map(|t| {
                if let Some(album_id) = t.albums.first().and_then(|a| a.id) {
                    format!("{}:{}", t.id, album_id)
                } else {
                    t.id.clone()
                }
            })
            .collect(),
        PlaylistTracks::WithInfo(tracks) => tracks
            .iter()
            .map(|t| {
                if let Some(album_id) = t.track.albums.first().and_then(|a| a.id) {
                    format!("{}:{}", t.track.id, album_id)
                } else {
                    t.track.id.clone()
                }
            })
            .collect(),
        PlaylistTracks::Partial(partial) => partial
            .iter()
            .map(|p| {
                if let Some(album_id) = p.album_id {
                    format!("{}:{}", p.id, album_id)
                } else {
                    p.id.clone()
                }
            })
            .collect(),
    }
}

/// Asynchronously retrieves full track data from a PlaylistTracks enum.
/// If the data is partial, it performs a bulk fetch through the API in bounded chunks.
pub async fn fetch_full_tracks(
    api: &crate::http::ApiService,
    playlist_tracks: PlaylistTracks,
) -> Vec<Track> {
    match playlist_tracks {
        PlaylistTracks::Full(tracks) => tracks,
        PlaylistTracks::WithInfo(tracks) => tracks.into_iter().map(|t| t.track).collect(),
        PlaylistTracks::Partial(partial) => {
            let ids: Vec<String> = partial.into_iter().map(|p| p.id.to_string()).collect();

            let mut fetched = Vec::new();
            // Keep requests sequential and bounded.  Apart from avoiding a burst of
            // requests, this also keeps the API's batch limit explicit.
            for chunk in ids.chunks(50) {
                let chunk_ids = chunk.to_vec();
                for attempt in 0..3 {
                    let result = tokio::time::timeout(
                        Duration::from_secs(10),
                        api.fetch_tracks(chunk_ids.clone()),
                    )
                    .await;
                    match result {
                        Ok(Ok(tracks)) => {
                            fetched.extend(tracks);
                            break;
                        }
                        Ok(Err(_)) | Err(_) if attempt < 2 => {
                            tokio::time::sleep(Duration::from_millis(250 * (1 << attempt))).await;
                        }
                        Ok(Err(_)) | Err(_) => break,
                    }
                }
            }
            fetched
        }
    }
}

/// Minimal `Track` for unit tests. Only `id`/`realId` are required by the
/// model's deserializer; everything else defaults.
#[cfg(test)]
pub fn test_track(id: &str) -> Track {
    serde_json::from_value(serde_json::json!({ "id": id, "realId": id }))
        .expect("test track JSON must deserialize")
}

#[cfg(test)]
mod like_entity_id_tests {
    use super::*;

    #[test]
    fn appends_album_when_missing() {
        assert_eq!(like_entity_id("123", Some("456")), "123:456");
    }

    #[test]
    fn keeps_bare_id_without_album() {
        assert_eq!(like_entity_id("123", None), "123");
        assert_eq!(like_entity_id("123", Some("")), "123");
        assert_eq!(like_entity_id("123", Some("   ")), "123");
    }

    #[test]
    fn never_double_appends_suffix() {
        assert_eq!(like_entity_id("123:456", Some("456")), "123:456");
        assert_eq!(like_entity_id("123:456", Some("789")), "123:456");
        assert_eq!(like_entity_id("123:456", None), "123:456");
    }
}

/// Resolve mixed base/composite Yandex ids without changing visible list order.
pub(crate) fn ordered_playback_tracks(
    ids: &[String], selected: &str, fetched: &std::collections::HashMap<String, Track>,
) -> Result<(im::Vector<Track>, usize), String> {
    let tracks: im::Vector<_> = ids.iter().filter_map(|id| fetched.get(id.to_base_id()).cloned()).collect();
    let index = tracks.iter().position(|track| track.id.to_base_id() == selected.to_base_id())
        .ok_or_else(|| "Selected track unavailable".to_string())?;
    Ok((tracks, index))
}

#[cfg(test)]
mod list_playback_hotfix_tests {
    use super::*;
    #[test]
    fn resolves_album_ids_preserves_order_and_previous_tracks() {
        let tracks: Vec<Track> = ["11", "22", "33"].into_iter().map(|id| serde_json::from_value(serde_json::json!({"id":id,"realId":id,"title":id})).unwrap()).collect();
        let fetched = tracks.into_iter().map(|t| (t.id.clone(), t)).collect();
        let (ordered, index) = ordered_playback_tracks(&["11:9".into(),"22:8".into(),"33".into()], "22:7", &fetched).unwrap();
        assert_eq!(index, 1);
        assert_eq!(ordered.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["11","22","33"]);
        assert!(ordered_playback_tracks(&["11".into()], "missing", &fetched).is_err());
    }
}

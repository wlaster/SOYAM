use super::*;
use super::{fallbacks::*, parsing::*, search::*};
#[test]
fn exact_listeners_only_never_followers_or_rounded_numbers() {
    assert_eq!(
        parse_listeners(r#"{"monthlyListeners":1234567,"followers":999}"#),
        Some(1234567)
    );
    assert_eq!(
        parse_listeners(
            r#"<meta name="description" content="Artist · 1,234,567 monthly listeners.">"#
        ),
        Some(1234567)
    );
    assert_eq!(
        parse_listeners(r#"<meta name="description" content="Artist · 1.2M monthly listeners.">"#),
        None
    );
    assert_eq!(
        parse_listeners(r#"{"followers":999,"popularity":70}"#),
        None
    );
    assert_eq!(
        parse_listeners(
            r#"<meta name="description" content="Artist · 52.3M monthly listeners"><div data-testid="monthly-listeners-label">52,280,673 monthly listeners</div>"#
        ),
        Some(52_280_673)
    );
    assert_eq!(
        parse_listeners(
            r#"<div data-testid="monthly-listeners-label">1&#160;234&#160;567 monthly listeners</div>"#
        ),
        Some(1_234_567)
    );
}
#[test]
fn description_supports_compact_audience_but_never_followers() {
    for (text, expected) in [
        ("52.3M monthly listeners", Some(52_300_000)),
        ("1.2K monthly listeners", Some(1200)),
        ("1,5M monthly listeners", Some(1_500_000)),
        ("1B monthly listeners", Some(1_000_000_000)),
        ("1.2M followers", None),
    ] {
        let html = format!("<meta name=\"description\" content=\"Artist · {text}\">");
        assert_eq!(parse_description_listeners(&html), expected);
    }
}

#[test]
fn only_actual_spotify_artist_links_are_accepted() {
    assert!(spotify_url("https://open.spotify.com/artist/53XhwfbYqKCa1cC15pYq2q?si=x").is_some());
    assert!(spotify_url("https://fake.com/artist/53XhwfbYqKCa1cC15pYq2q").is_none());
    assert!(spotify_url("https://open.spotify.com/track/53XhwfbYqKCa1cC15pYq2q").is_none());
}
#[test]
fn wikidata_requires_matching_identity_and_unambiguous_spotify_id() {
    let mut claims = serde_json::json!({
        "P1553": [{"rank":"normal","mainsnak":{"datavalue":{"value":"675068"}}}],
        "P1902": [{"rank":"normal","mainsnak":{"datavalue":{"value":"53XhwfbYqKCa1cC15pYq2q"}}}]
    });
    assert!(spotify_from_claims(&claims, "675068").is_some());
    assert!(spotify_from_claims(&claims, "111").is_none());
    claims["P1902"][0]["rank"] = "deprecated".into();
    assert!(spotify_from_claims(&claims, "675068").is_none());
}

#[test]
fn hub_links_must_be_unique_artist_profiles() {
    assert_eq!(
        spotify_from_hub_html(
            r#"{"url":"https:\/\/open.spotify.com\/artist\/5wOtcLBeZC9t4M9Y39vch7"}"#
        ),
        Some("https://open.spotify.com/artist/5wOtcLBeZC9t4M9Y39vch7".into())
    );
    assert!(
        spotify_from_hub_html("https://open.spotify.com/track/5wOtcLBeZC9t4M9Y39vch7").is_none()
    );
    assert!(spotify_from_hub_html("https://open.spotify.com/artist/5wOtcLBeZC9t4M9Y39vch7 https://open.spotify.com/artist/21tYJ2uMe1GhcyNM2xOktv").is_none());
    assert!(is_bandlink("https://band.link/alexeykrivdin"));
    assert!(!is_bandlink("https://band.link.evil.test/path"));
    assert!(!is_bandlink("http://localhost/path"));
}

#[test]
fn spotify_search_requires_one_exact_artist_identity() {
    let mut value = serde_json::json!({"data":{"searchV2":{"artists":{"items":[
        {"data":{"id":"6e7RZl3HAKI6lxDWuVo1nH","profile":{"name":"Soultans"}}},
        {"data":{"id":"5EP020iZcwBqHRnJftibXX","profile":{"name":"Sultans"}}}
    ]}}}});
    assert_eq!(
        searched_artist_url(&value, "Soultans").as_deref(),
        Some("https://open.spotify.com/artist/6e7RZl3HAKI6lxDWuVo1nH")
    );
    assert!(searched_artist_url(&value, "Missing").is_none());
    value["data"]["searchV2"]["artists"]["items"][1]["data"]["profile"]["name"] = "Soultans".into();
    assert!(searched_artist_url(&value, "Soultans").is_none());
}

#[test]
fn exact_spelling_wins_over_other_profiles_with_similar_names() {
    let data = serde_json::json!({"data":{"searchV2":{"artists":{"items":[
        {"data":{"id":"0WLS16S6MJvMgWXrUezleN","profile":{"name":"De-Phazz"}}},
        {"data":{"id":"2IgIIjRdiM4M9apR9JdmCP","profile":{"name":"De Phazz"}}}
    ]}}}});
    assert_eq!(
        searched_artist_url(&data, "De-Phazz").as_deref(),
        Some("https://open.spotify.com/artist/0WLS16S6MJvMgWXrUezleN")
    );
    assert_eq!(artist_match_score("Télépopmusik", "Télépopmusik"), 3);
    assert_eq!(artist_match_score("Telepopmusik", "Télépopmusik"), 1);
    assert_eq!(artist_match_score("De‐Phazz", "De-Phazz"), 2);
}

#[test]
fn duplicate_embed_tracks_do_not_fake_repertoire_identity() {
    let titles = [
        ("breathe".to_owned(), vec![0]),
        ("smile".to_owned(), vec![0]),
    ]
    .into_iter()
    .collect();
    let html = r#"<script id="__NEXT_DATA__">{"props":{"pageProps":{"state":{"data":{"entity":{"trackList":[{"title":"Breathe"},{"title":"Breathe"},{"title":"Smile"}]}}}}}}</script>"#;
    assert_eq!(matching_track_score(html, &titles), 2);
    assert_eq!(matching_track_score("<html>Unavailable</html>", &titles), 0);
}

#[test]
fn cyrillic_empty_profiles_do_not_hide_the_latin_artist() {
    let value = serde_json::json!({"data":{"searchV2":{"artists":{"items":[
        {"data":{"id":"40edmg1pVhEKklKU7BCo1n","profile":{"name":"Timur Rodriguez"}}},
        {"data":{"id":"2jHNBVku1UahHGofZHQZaa","profile":{"name":"Тимур Родригез"}}},
        {"data":{"id":"79dJwkh2IaTDhoVbF2Ycbb","profile":{"name":"Тимур Родригез"}}}
    ]}}}});
    let candidates = related_artist_candidates(&value, "Тимур Родригез");
    assert_eq!(candidates.len(), 3);
    assert!(
        candidates.contains(&"https://open.spotify.com/artist/40edmg1pVhEKklKU7BCo1n".to_owned())
    );
}

#[test]
fn repertoire_matches_transliteration_but_rejects_different_recording_lengths() {
    let titles = [
        (cross_script_key("О тебе"), vec![221280]),
        (cross_script_key("Без тебя легче"), vec![218130]),
    ]
    .into_iter()
    .collect();
    let good = r#"<script id="__NEXT_DATA__">{"props":{"pageProps":{"state":{"data":{"entity":{"trackList":[{"title":"O tebe","duration":221286},{"title":"Без тебя легче","duration":218135}]}}}}}}</script>"#;
    assert_eq!(matching_track_score(good, &titles), 2);
    let cover = good.replace("221286", "100000").replace("218135", "300000");
    assert_eq!(matching_track_score(&cover, &titles), 0);
}

#[test]
fn punctuation_and_diacritics_do_not_lose_artist_profiles() {
    assert_eq!(
        normalized_artist_name("De‐Phazz"),
        normalized_artist_name("De-Phazz")
    );
    assert_eq!(
        normalized_artist_name("Télépopmusik"),
        normalized_artist_name("TELEPOPMUSIK")
    );
    assert_ne!(
        normalized_artist_name("Soultans"),
        normalized_artist_name("Sultans")
    );
}

#[tokio::test]
async fn stalled_source_never_delays_a_successful_other_source() {
    let stalled = futures::future::pending::<Option<String>>().boxed();
    let good =
        async { Some("https://open.spotify.com/artist/6e7RZl3HAKI6lxDWuVo1nH".into()) }.boxed();
    let result =
        tokio::time::timeout(Duration::from_secs(1), first_profile(vec![stalled, good])).await;
    assert!(result.unwrap().is_some());
    let empty = futures::future::ready(None).boxed();
    let good = futures::future::ready(Some("valid".to_owned())).boxed();
    assert_eq!(
        first_profile(vec![empty, good]).await.as_deref(),
        Some("valid")
    );
}

#[tokio::test]
#[ignore = "saved local Yandex authorization and internet; no playback or UI"]
async fn live_reported_artist_profiles() {
    let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
        .join("darkplayoff.yayma.app/yayma/yamusic_v2.db");
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let raw: String = db
        .query_row(
            "SELECT value FROM app_settings WHERE key='auth_token'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let (token, uid): (String, u64) = serde_json::from_str(&raw).unwrap();
    drop(db);
    let api = crate::http::ApiService::new(token, Some(uid))
        .await
        .unwrap();
    for (id, name, spotify_id, minimum, maximum) in [
        (
            "7329854",
            "Castle Heat",
            "21tYJ2uMe1GhcyNM2xOktv",
            1000,
            2000,
        ),
        (
            "6012070",
            "Alexey Krivdin",
            "5wOtcLBeZC9t4M9Y39vch7",
            20000,
            35000,
        ),
    ] {
        let started = Instant::now();
        let result = load_artist_spotify_stats(&api, id).await;
        assert_eq!(
            result.url.as_deref(),
            Some(format!("https://open.spotify.com/artist/{spotify_id}").as_str()),
            "wrong/missing profile for {name}"
        );
        let count = result
            .monthly_listeners
            .expect("monthly audience must reach the production DTO");
        assert!(
            (minimum..maximum).contains(&count),
            "wrong audience for {name}: {count}"
        );
        println!(
            "PRODUCTION {name}: url={:?}, monthly_listeners={count}, elapsed={:?}",
            result.url,
            started.elapsed()
        );
    }
}

#[tokio::test]
#[ignore = "saved local account and real network; artist audience audit"]
async fn live_varied_artist_audit() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
        .join("darkplayoff.yayma.app/yayma/yamusic_v2.db");
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let raw: String = db
        .query_row(
            "SELECT value FROM app_settings WHERE key='auth_token'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let (token, uid): (String, u64) = serde_json::from_str(&raw).unwrap();
    drop(db);
    let api = crate::http::ApiService::new(token, Some(uid))
        .await
        .unwrap();
    let mut rows = Vec::new();
    for (id, name) in [
        ("16045253", "Rob"),
        ("17258227", "Lewis Hanton"),
        ("161012", "Тимур Родригез"),
        ("41092", "Найк Борзов"),
        ("673006", "Мураками"),
        ("425646", "Мои Ракеты Вверх"),
        ("156428", "Павел Кашин"),
        ("41094", "Ундервуд"),
        ("708460", "Зимавсегда"),
        ("216596", "Soultans"),
        ("19600", "De-Phazz"),
        ("87889", "Télépopmusik"),
        ("29912", "Morcheeba"),
        ("159387", "Hooverphonic"),
        ("278033", "Belleruche"),
        ("7329854", "Castle Heat"),
        ("6012070", "Alexey Krivdin"),
    ] {
        let started = Instant::now();
        let cold = within_deadline(
            id,
            load_artist_spotify_stats(&api, id),
            Duration::from_secs(25),
        )
        .await;
        let cold_elapsed = started.elapsed().as_millis() as u64;
        let ((artist, _), (tracks, _)) = tokio::try_join!(
            api.fetch_artist(id.to_owned()),
            api.fetch_artist_tracks_paginated(id.to_owned(), 0, 30)
        )
        .unwrap();
        remember_artist_page(id, &artist, &tracks);
        let started = Instant::now();
        let result = within_deadline(
            id,
            load_artist_spotify_stats(&api, id),
            Duration::from_secs(25),
        )
        .await;
        assert_eq!(
            result.url, cold.url,
            "page snapshot changed the Spotify identity for {name}"
        );
        assert!(
            result.monthly_listeners.is_some(),
            "missing audience with page metadata for {name}"
        );
        if id == "17258227" {
            assert_eq!(
                result.url.as_deref(),
                Some("https://open.spotify.com/artist/7C2uVXwFKLlwHteHGnMLhZ"),
                "Lewis Hanton must resolve to the catalogue with the matching recordings"
            );
        }
        if id == "16045253" {
            assert_eq!(
                result.url.as_deref(),
                Some("https://open.spotify.com/artist/5UvEAbRD8ppSm1hQ7YaQux"),
                "Rob must be found via his songs, not a popular namesake"
            );
            assert!(result.monthly_listeners.unwrap() > 0);
        }
        let row = serde_json::json!({"name":name,"yandex_id":id,"spotify_url":result.url,"monthly_listeners":result.monthly_listeners,"elapsed_ms":started.elapsed().as_millis() as u64,"cold_elapsed_ms":cold_elapsed,"reused_page_metadata":true});
        println!("AUDIT {row}");
        rows.push(row);
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/spotify-varied-artists.json");
    std::fs::write(path, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    let failed: Vec<_> = rows
        .iter()
        .filter(|row| row["monthly_listeners"].is_null())
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert!(failed.is_empty(), "missing audience: {failed:?}");
}

#[tokio::test]
#[ignore = "requires internet and Spotify connectivity"]
async fn live_spotify_profile() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    for (yandex_id, name, expected_id) in [
        ("675068", "Imagine Dragons", "53XhwfbYqKCa1cC15pYq2q"),
        ("9262", "Nirvana", "6olE6TJLqED3rqDCT0FyPh"),
        ("975", "Linkin Park", "6XyY86QOPPrYVGvF9ch6wz"),
    ] {
        let url = discover_spotify(yandex_id, name, None)
            .await
            .expect("artist profile must resolve");
        assert_eq!(
            url,
            format!("https://open.spotify.com/artist/{expected_id}")
        );
        println!("Resolved {name}: {url}");
        let started = Instant::now();
        let count = fetch_listeners(&url)
            .await
            .expect("Spotify must return monthly audience");
        println!("Compact profile request: {:?}", started.elapsed());
        assert!(count > 1000, "unexpected audience: {count}");
        println!("{name}: {url} -> {count} monthly listeners");
    }
}
#[test]
fn saved_profile_age_is_not_renewed_by_reading_audience() {
    let url = "https://open.spotify.com/artist/6e7RZl3HAKI6lxDWuVo1nH".to_owned();
    let now = 100 * 86400;
    assert!(valid_saved_profile(&(now - 86400, url.clone()), now));
    assert!(!valid_saved_profile(&(now - 31 * 86400, url.clone()), now));
    assert!(!valid_saved_profile(&(now + 86400, url), now));
}

#[tokio::test]
async fn lookup_deadline_includes_waiting_for_an_in_flight_request() {
    let lock = tokio::sync::Mutex::new(());
    let _held = lock.lock().await;
    let blocked = async {
        let _guard = lock.lock().await;
        SpotifyArtistStatsDto {
            monthly_listeners: Some(999),
            url: None,
        }
    };
    let result = within_deadline("same-artist", blocked, Duration::from_millis(20)).await;
    assert!(result.monthly_listeners.is_none());
}

#[tokio::test]
async fn audience_reader_returns_before_the_unused_response_tail() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (release, hold) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 4096];
        let _ = socket.read(&mut request);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n<head><title>Artist</title>").unwrap();
        socket.flush().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        socket
            .write_all(
                b"<meta name=\"description\" content=\"Artist - 14.8K monthly listeners\"></head>",
            )
            .unwrap();
        socket.flush().unwrap();
        let _ = hold.recv_timeout(Duration::from_secs(3));
    });
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let response = CATALOG_CLIENT
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap();
        read_profile_audience(response).await
    })
    .await;
    let _ = release.send(());
    server.join().unwrap();
    assert_eq!(result.unwrap(), Some(14800));
}

#[tokio::test]
async fn missing_profile_is_distinct_from_temporary_service_failure() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for status in ["404 Not Found", "503 Service Unavailable"] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request);
            write!(
                socket,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        }
    });
    let url = format!("http://{address}");
    assert!(matches!(fetch_profile(&url).await, ProfileResult::Missing));
    assert!(matches!(
        fetch_profile(&url).await,
        ProfileResult::Unavailable
    ));
    server.join().unwrap();
}

#[test]
fn seven_same_name_profiles_are_all_kept_for_recording_checks() {
    let ids = [
        "7C2uVXwFKLlwHteHGnMLhZ",
        "65auZ0lfxYAmRBH18DyLou",
        "4FemjcQd5KnCmgdVgotRwh",
        "5hwx2VNecNI851UBtDWjzh",
        "35S9ic5Bygk6Fbyx7hufUf",
        "1h6vFRD40epJNfULDxsNLB",
        "7BknCxbHDFbCWwJyN704T0",
    ];
    let items: Vec<_> = ids
        .iter()
        .map(|id| serde_json::json!({"data":{"id":id,"profile":{"name":"Lewis Hanton"}}}))
        .collect();
    let value = serde_json::json!({"data":{"searchV2":{"artists":{"items":items}}}});
    let candidates = related_artist_candidates(&value, "Lewis Hanton");
    assert_eq!(candidates.len(), 7);
    assert!(
        ids.iter()
            .all(|id| candidates.contains(&format!("https://open.spotify.com/artist/{id}")))
    );
    assert!(
        searched_artist_url(&value, "Lewis Hanton").is_none(),
        "a name alone cannot select one of seven profiles"
    );
}

#[test]
fn recording_search_recovers_a_small_artist_missing_from_name_results() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/rob-track-search.json")).unwrap();
    let repertoire = Repertoire {
        recordings: [(cross_script_key("Пальмы"), vec![135560])]
            .into_iter()
            .collect(),
        popular_titles: vec!["Пальмы".to_owned()],
    };
    let urls = search::recording_search_candidates(&value, "Rob", &repertoire);
    assert_eq!(
        urls,
        vec!["https://open.spotify.com/artist/5UvEAbRD8ppSm1hQ7YaQux".to_owned()]
    );
    assert!(search::recording_search_candidates(&value, "Another artist", &repertoire).is_empty());
}

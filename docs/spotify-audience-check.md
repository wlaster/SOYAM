# Spotify audience: cross-script artist lookup regression

Date: 2026-10-26 (Moscow). Figures are rounded live snapshots read from Spotify artist-page metadata.

## Reported failure

Yandex artist 161012 is named Тимур Родригез. Spotify search returned three relevant profiles:

- `40edmg1pVhEKklKU7BCo1n`: Timur Rodriguez, with the actual catalogue.
- `2jHNBVku1UahHGofZHQZaa`: Тимур Родригез, empty embed catalogue.
- `79dJwkh2IaTDhoVbF2Ycbb`: Тимур Родригез, one unrelated listed recording.

The old name matcher discarded the Latin-script main profile before comparing recordings. The number reader already reads Spotify's own HTML description and was not the cause.

## Changes

Candidate matching now includes Cyrillic/Latin transliteration variants, keeping the main profile alongside literal-name duplicates. When identity remains ambiguous, the app compares distinct recording titles and durations from Yandex against Spotify artist embeds. Durations must agree within five seconds when available; repeated titles cannot inflate the score. Two different matching recordings and one unique winner are required. Search aliases with no literal name match can also be checked against recordings instead of being discarded immediately.

No artist-specific mapping or listener count was added to production code. Temporary public Spotify web sessions remain in memory; no user login or private credentials are needed. Public web search, the artist's BandLink, cached validated profiles, and catalogue fallbacks remain available.

## Full production lookup audit

| Yandex artist | Artist | Spotify monthly listeners | Lookup | Spotify profile |
| --- | --- | ---: | ---: | --- |
| 161012 | Тимур Родригез | 14,800 | 5.14 s | https://open.spotify.com/artist/40edmg1pVhEKklKU7BCo1n |
| 41092 | Найк Борзов | 112,300 | 2.62 s | https://open.spotify.com/artist/6Y00vC4OI2oTt5ClApQXun |
| 673006 | Мураками | 74,200 | 2.20 s | https://open.spotify.com/artist/3AFKHQNN2UFXPBsKRNljNk |
| 425646 | Мои Ракеты Вверх | 32 | 1.86 s | https://open.spotify.com/artist/5CnFVBB9vvP9NOpSCxXv32 |
| 156428 | Павел Кашин | 7,200 | 2.87 s | https://open.spotify.com/artist/26fX0bzuVA60v3flSfkINj |
| 41094 | Ундервуд | 36,600 | 2.23 s | https://open.spotify.com/artist/72B99GBEFdbbEAfmKMdYrr |
| 708460 | Зимавсегда | 3,400 | 1.92 s | https://open.spotify.com/artist/7Kk4RfdR6TXiBFxkLa32Rt |
| 216596 | Soultans | 62,200 | 1.67 s | https://open.spotify.com/artist/6e7RZl3HAKI6lxDWuVo1nH |
| 19600 | De-Phazz | 462,000 | 2.09 s | https://open.spotify.com/artist/0WLS16S6MJvMgWXrUezleN |
| 87889 | Télépopmusik | 820,600 | 1.83 s | https://open.spotify.com/artist/3aKCo8gLJfuPYtr88aWKjF |
| 29912 | Morcheeba | 2,200,000 | 1.78 s | https://open.spotify.com/artist/6bWxFw65IEJzBYjx3SxUXd |
| 159387 | Hooverphonic | 1,100,000 | 1.60 s | https://open.spotify.com/artist/5EP020iZcwBqHRnJftibXX |
| 278033 | Belleruche | 609,600 | 1.41 s | https://open.spotify.com/artist/4N9nNkpjGuBlGWnYubQI8t |
| 7329854 | Castle Heat | 1,400 | 1.13 s | https://open.spotify.com/artist/21tYJ2uMe1GhcyNM2xOktv |
| 6012070 | Alexey Krivdin | 26,000 | 1.89 s | https://open.spotify.com/artist/5wOtcLBeZC9t4M9Y39vch7 |

All 15 artists returned a profile and audience through the same full lookup function used by the app. The audit used actual Yandex artist data and did not preload artist mappings. The Spotify monthly count is read from the public profile page, not inferred from followers or track streams.

## Validation

- Rust full suite: 136 passed, 12 external tests ignored.
- Explicit fifteen-artist live audit: passed in 32.27 seconds.
- Regression tests cover the Latin main profile hidden by Cyrillic duplicates, cross-script recording titles, duration mismatches, ambiguous names and stalled fallback sources.
- Windows release build: succeeded. Delivery and ZIP: all 40 files verified against the release by SHA-256. Desktop shortcut updated.
- Native GUI testing remains unavailable because the Computer Use tool previously rejected access to YAYMA. This report describes backend production-path verification, not a visual run of the app.

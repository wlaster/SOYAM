# Rob: name + popular recording search

Date: 2026-10-26 (Moscow), VPN enabled.

## Reported failure and cause

Yandex artist `16045253` is Rob. Spotify's first twenty artist-name search results contained more prominent names such as Robin Packalen, Robyn and Robbie Williams, but did not contain the required Rob profile. Validating those results could not recover an artist absent from the list.

A live search for `Rob Пальмы` returned the correct recording with credited Spotify artist `5UvEAbRD8ppSm1hQ7YaQux`. Its public artist page reported `577 monthly listeners`, agreeing with the supplied screenshot.

## Implemented behavior

- Keep popular recording titles, in their original spelling/order, together with existing normalized recording references from the already loaded Yandex artist page.
- For short or unresolved/ambiguous artist names, use Spotify's public `findTracks` search with `artist name + popular song title`. Try up to three popular recordings so a missing or unavailable first recording does not prevent discovery.
- Extract credited artist identities only from matching recording titles; confirm names and then compare at least two distinct recordings and their durations through the artist embed before accepting a profile.
- Keep the existing direct path for clear artist-name matches and the existing bounded parallel checks. No artist-specific mapping or audience count was added to production.
- Share the HTTP/session/retry logic between artist and recording search. Query-document refresh is tracked per operation, rather than duplicating request code or confusing artist/recording documents.

## Validation

- Offline regression fixture is a reduced public response for `Rob Пальмы`; it must recover the correct Rob identity and reject unrelated artist names.
- Full normal Rust suite: 142 passed, 12 opt-in network tests ignored.
- Explicit live audit: all 17 artists returned a matching profile and audience with and without page metadata. The test specifically asserts Rob's Spotify identity. Runtime: 81.05 seconds for both paths and page preparation.
- Native GUI was not controlled; the computer-use tool has not granted access to YAYMA. Validation exercises the app's production data path.
- Windows release build succeeded. All 40 delivered files and archive entries verified against the release by SHA-256; desktop shortcut updated.

| Artist | Monthly listeners | Cold lookup | With page metadata |
| --- | ---: | ---: | ---: |
| Rob | 577 | 6.165 s | 4.131 s |
| Lewis Hanton | 1,500,000 | 2.394 s | 2.455 s |
| Тимур Родригез | 14,800 | 9.688 s | 1.378 s |
| Найк Борзов | 112,300 | 2.310 s | 1.496 s |
| Мураками | 74,200 | 1.474 s | 2.049 s |
| Мои Ракеты Вверх | 32 | 0.865 s | 1.230 s |
| Павел Кашин | 7,200 | 2.115 s | 4.849 s |
| Ундервуд | 36,600 | 3.201 s | 2.495 s |
| Зимавсегда | 3,400 | 1.049 s | 0.977 s |
| Soultans | 62,200 | 1.374 s | 1.204 s |
| De-Phazz | 462,000 | 1.916 s | 1.516 s |
| Télépopmusik | 820,600 | 7.475 s | 1.360 s |
| Morcheeba | 2,200,000 | 1.001 s | 0.700 s |
| Hooverphonic | 1,100,000 | 1.540 s | 1.120 s |
| Belleruche | 609,600 | 1.511 s | 1.055 s |
| Castle Heat | 1,400 | 1.274 s | 1.082 s |
| Alexey Krivdin | 26,000 | 1.690 s | 1.162 s |

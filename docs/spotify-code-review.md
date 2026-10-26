# Spotify code review and optimization

Date: 2026-10-26 (Moscow). Scope: Spotify audience orchestration, matching/search, page parsing, fallbacks, caching, artist-page integration and the Flutter audience widget.

## Comparison with Yandex

Yandex's `lastMonthListeners` arrives in the artist brief response that already loads the page. Spotify has no corresponding identifier in that response for most artists. First lookup therefore still needs profile discovery and sometimes recording comparisons. Once a Spotify identity is known, the normal path reads the stored URL and makes one Spotify page request; successful audience results are kept in memory for one hour.

## Findings and completed changes

| Finding | Change |
| --- | --- |
| Spotify requested Yandex artist metadata a second time and could request the same artist tracks again. | The first artist-page response registers a small, bounded ten-minute snapshot containing the name, links and recording references. Spotify reuses it without repeating those requests. The Yandex response and refresh behavior are unchanged. |
| Search prepared the same artist names repeatedly and calculated edit distance even for Latin-only names. | One candidate analysis per response; prepare the query name once, preserve exact-spelling priority, calculate edit distance only when cross-script matching needs it. |
| Each response chunk could cause two full HTML parses and repeated CSS selector compilation. | One combined audience parser, static selectors/regexes, shared embed JSON extraction and reuse of the Spotify HTTP client. Short head fragments are checked immediately; large unhelpful responses are scanned at bounded, increasing sizes. |
| Every successful audience refresh rewrote the same profile row and renewed its resolution timestamp indefinitely. | Write only newly resolved identities; enforce the real thirty-day age of the mapping. |
| Cached profiles returning 404/410 stayed stuck until the cache expired. | Remove deleted mappings and rediscover; temporary network/503 errors keep the validated identity. |
| The deadline did not include waiting for another lookup or database access. | The public API now has one deadline covering the entire operation, including request deduplication. The existing Flutter thirty-second guard remains. |
| Anonymous sessions had a fixed lifetime and a 401 immediately forced slow fallbacks. | Use the published expiration time with a safety margin; refresh once on 401, still respecting 429 pauses and bounded retries. |
| Raw fallback catalogue caches could hold 256 large JSON documents each. | Bound each catalogue cache to 32 entries. |
| Search, parsing, cache orchestration, fallbacks and tests were intertwined in one file. | Separate private `search.rs`, `parsing.rs`, `fallbacks.rs` and `tests.rs`; keep the public bridge function/model unchanged. |

The identity checks for homonyms, Cyrillic/Latin aliases, distinct songs and recording durations are retained. They are needed to avoid selecting empty or unrelated Spotify profiles such as the reported Timur Rodriguez duplicates. The fallback services remain cold paths rather than being deleted for a smaller line count.

## Validation

- `cargo check --locked`: passed without code warnings.
- Full Rust suite: 140 passed, 12 opt-in network tests ignored.
- Explicit live audit: all 15 artists returned the same Spotify identity with and without reusing page metadata; all returned audiences.
- Live audit ran 30 lookups. Median without a page snapshot: 1.888 s; with the snapshot: 1.265 s. This is an observational network run, not a controlled before/after benchmark. Some individual requests were slower because network latency varies. Artist-page loading itself is excluded from the snapshot lookup time.
- Regression checks include a description split across response chunks with a stalled unused body, timeout while waiting for an in-flight lookup, mapping age, and distinct handling of 404 versus 503.
- Native UI was not exercised; Computer Use access to YAYMA remains unavailable. The public bridge signature and Flutter widget behavior are unchanged.

## Artist audit

| Artist | Monthly listeners | Without snapshot | With snapshot |
| --- | ---: | ---: | ---: |
| Тимур Родригез | 14,800 | 3.602 s | 1.061 s |
| Найк Борзов | 112,300 | 3.086 s | 3.753 s |
| Мураками | 74,200 | 3.031 s | 1.540 s |
| Мои Ракеты Вверх | 32 | 0.827 s | 1.115 s |
| Павел Кашин | 7,200 | 2.387 s | 1.718 s |
| Ундервуд | 36,600 | 1.888 s | 1.024 s |
| Зимавсегда | 3,400 | 1.025 s | 0.783 s |
| Soultans | 62,200 | 1.407 s | 1.225 s |
| De-Phazz | 462,000 | 3.294 s | 1.265 s |
| Télépopmusik | 820,600 | 2.144 s | 1.634 s |
| Morcheeba | 2,200,000 | 1.418 s | 0.687 s |
| Hooverphonic | 1,100,000 | 1.541 s | 1.322 s |
| Belleruche | 609,600 | 1.421 s | 1.358 s |
| Castle Heat | 1,400 | 1.376 s | 1.414 s |
| Alexey Krivdin | 26,000 | 2.091 s | 1.172 s |

Windows release: succeeded. All 40 delivered files and ZIP entries verified against the release using SHA-256. Desktop shortcut updated.

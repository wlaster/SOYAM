# Lewis Hanton: Spotify audience regression

Date: 2026-10-26 (Moscow), VPN enabled.

## Cause

Yandex artist `17258227` is Lewis Hanton. Spotify public search returned seven exact-name profiles. The previous disambiguation function returned `None` immediately when there were more than six candidates, so no recording comparisons were made for this artist.

## Fix

Removed the reject-on-count condition. All candidates in the bounded Spotify search response can now be compared, with at most four concurrent embed requests and a four-second timeout per candidate. The whole lookup retains its existing deadline. Selection still requires two distinct matching recordings with compatible durations and a unique best match; no count or artist mapping is hardcoded in production.

## Result

Resolved the Spotify profile `7C2uVXwFKLlwHteHGnMLhZ`. Its public artist-page description returned `1.5M monthly listeners`, which the application formats as `1,5 млн в месяц`.

The live regression test now explicitly asserts this Spotify identity, rather than merely accepting any nonempty listener value. It also compares cold discovery with reuse of the actual Yandex page metadata.

## Verification

- Artist-statistics unit tests: 17 passed; 3 opt-in network tests excluded from that run.
- Full normal Rust suite: 141 passed, 12 opt-in network tests ignored.
- Explicit live audit: all 16 artists returned a profile and audience on both cold and page-metadata paths; passed in 58.18 seconds.
- Native UI was not exercised because the computer-control tool does not have access to YAYMA.
- Windows release build succeeded. All 40 delivered files and archive entries verified against the release by SHA-256; desktop shortcut updated.

| Artist | Monthly listeners | Cold lookup | With page metadata |
| --- | ---: | ---: | ---: |
| Lewis Hanton | 1,500,000 | 4.083 s | 2.136 s |
| Тимур Родригез | 14,800 | 2.488 s | 1.759 s |
| Найк Борзов | 112,300 | 1.783 s | 2.770 s |
| Мураками | 74,200 | 1.483 s | 1.281 s |
| Мои Ракеты Вверх | 32 | 0.941 s | 0.938 s |
| Павел Кашин | 7,200 | 2.552 s | 3.275 s |
| Ундервуд | 36,600 | 3.799 s | 1.523 s |
| Зимавсегда | 3,400 | 0.977 s | 1.253 s |
| Soultans | 62,200 | 1.001 s | 0.938 s |
| De-Phazz | 462,000 | 1.822 s | 1.657 s |
| Télépopmusik | 820,600 | 1.364 s | 1.542 s |
| Morcheeba | 2,200,000 | 0.998 s | 0.843 s |
| Hooverphonic | 1,100,000 | 1.318 s | 1.131 s |
| Belleruche | 609,600 | 1.663 s | 1.195 s |
| Castle Heat | 1,400 | 1.387 s | 0.995 s |
| Alexey Krivdin | 26,000 | 2.341 s | 1.100 s |

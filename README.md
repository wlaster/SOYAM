# <img src="src/assets/icons/logo.svg" width="48" align="middle" alt="YAYMA" /> YAYMA (Yet Another Yandex Music App)

![License](https://img.shields.io/github/license/DarkPlayOff/YAYMA)
![Release](https://img.shields.io/github/v/release/DarkPlayOff/YAYMA)
![Downloads](https://img.shields.io/github/downloads/DarkPlayOff/YAYMA/total)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-lightgrey)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/DarkPlayOff/YAYMA)

Альтернативный клиент для Яндекс Музыки

<table>
  <tr>
    <td align="center"><img src="docs/screenshots/ui_home.webp" width="280" alt="Главный экран"/></td>
    <td align="center"><img src="docs/screenshots/ui_search.webp" width="280" alt="Поиск"/></td>
    <td align="center"><img src="docs/screenshots/ui_artist.webp" width="280" alt="Страница исполнителя"/></td>
  </tr>
</table>

## Предупреждение

**YAYMA** основан на [YaMusic](https://github.com/yamusic/yamusic) и использует неофициальное API, не предназначенное для публичного использования. Проект находится в стадии активной разработки. Пользуйтесь на свой страх и риск. Проект написан с помощью ИИ.

## Возможности

- **Приватность**: никакой аналитики и слежки со стороны клиента
- **Тексты песен**: Яндекс Музыка, LRCLIB, Lyrics.ovh и Genius; выбор включённых источников прямо в панели текста. Тексты с таймкодами следуют воспроизведению и перемотке, обычные прокручиваются вручную. BetterLyrics и YouLyPlus доступны как дополнительные источники в настройках.
- **Интеграция с системой**: управление из панели задач Windows, глобальные горячие клавиши

## Благодарности ❤️

- [Vyfor](https://github.com/vyfor) — за исходный TUI-клиент [YaMusic](https://github.com/yamusic/yamusic) и библиотеку [yandex-music-rs](https://github.com/vyfor/yandex-music-rs) для работы с API Яндекса
- [LrcLib](https://lrclib.net), [BetterLyrics](https://github.com/better-lyrics/better-lyrics), [YouLyPlus](https://github.com/ibratabian17/YouLyPlus) — за базы текстов песен
- [Lyrics.ovh](https://lyrics.ovh) и [Genius](https://genius.com) — источники обычных текстов

## Локальная сборка Windows и проверка текстов

См. [инструкцию](docs/windows-build.md). Проверить доступность источников с вашего подключения можно командой `powershell -File scripts/Test-LyricsSources.ps1`.

## Лицензия

Проект распространяется под лицензией [GPL-3.0](LICENSE).

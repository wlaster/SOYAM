# <img src="src/assets/icons/logo.svg" width="48" align="middle" alt="SOYAM" /> SOYAM

![License](https://img.shields.io/github/license/wlaster/SOYAM)
![Release](https://img.shields.io/github/v/release/wlaster/SOYAM)
![Downloads](https://img.shields.io/github/downloads/wlaster/SOYAM/total)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-lightgrey)

Альтернативный клиент Яндекс Музыки на Flutter и Rust. Основная проверенная сборка — Windows x64.

<table>
  <tr>
    <td align="center"><img src="docs/screenshots/ui_home.webp" width="280" alt="Главный экран"/></td>
    <td align="center"><img src="docs/screenshots/ui_search.webp" width="280" alt="Поиск"/></td>
    <td align="center"><img src="docs/screenshots/ui_artist.webp" width="280" alt="Страница исполнителя"/></td>
  </tr>
</table>

## Предупреждение

**SOYAM** — форк [YAYMA](https://github.com/DarkPlayOff/YAYMA). Приложение основано на [YaMusic](https://github.com/yamusic/yamusic) и использует неофициальное API, не предназначенное для публичного использования. Проект находится в стадии активной разработки. Пользуйтесь на свой страх и риск. Проект написан с помощью ИИ.

## Возможности

- **Воспроизведение**: Моя волна, поиск, библиотека, альбомы, плейлисты, скачивание треков и выбор качества звука.
- **Рекомендации**: настройка «Меньше ИИ» синхронизируется с аккаунтом Яндекс Музыки.
- **Исполнители**: отдельные счётчики слушателей Яндекс Музыки и Spotify. При недоступности сервиса отображается прочерк.
- **Приватность**: никакой аналитики и слежки со стороны клиента
- **Тексты песен**: Яндекс Музыка, LRCLIB, Lyrics.ovh и Genius; выбор включённых источников прямо в панели текста. Тексты с таймкодами следуют воспроизведению и перемотке, обычные прокручиваются вручную. BetterLyrics и YouLyPlus доступны как дополнительные источники в настройках.
- **Интеграция с системой**: управление из панели задач Windows, глобальные горячие клавиши

## Установка

[![Windows x64 — скачать установщик](https://img.shields.io/badge/Windows_x64-Скачать_установщик-1DB954?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/wlaster/SOYAM/releases/latest/download/soyam-windows-setup.exe)
![Android — скоро](https://img.shields.io/badge/Android-Скоро-777777?style=for-the-badge&logo=android&logoColor=white)
![Linux — скоро](https://img.shields.io/badge/Linux-Скоро-777777?style=for-the-badge&logo=linux&logoColor=white)

Скачайте установщик последней версии для Windows и запустите его.
Портативная версия доступна на [странице релизов](https://github.com/wlaster/SOYAM/releases/latest).

## Благодарности ❤️

- [DarkPlayOff](https://github.com/DarkPlayOff/YAYMA) — исходный проект YAYMA

- [Vyfor](https://github.com/vyfor) — за исходный TUI-клиент [YaMusic](https://github.com/yamusic/yamusic) и библиотеку [yandex-music-rs](https://github.com/vyfor/yandex-music-rs) для работы с API Яндекса
- [LrcLib](https://lrclib.net), [BetterLyrics](https://github.com/better-lyrics/better-lyrics), [YouLyPlus](https://github.com/ibratabian17/YouLyPlus) — за базы текстов песен
- [Lyrics.ovh](https://lyrics.ovh) и [Genius](https://genius.com) — источники обычных текстов

## Локальная сборка Windows и проверка текстов

См. [инструкцию](docs/windows-build.md). Полная сборка: `powershell -File scripts/Build-Windows.ps1`. Упаковка готовой Release-сборки с библиотеками MSVC x64: `powershell -File scripts/Package-Windows.ps1`. Скрипт проверяет SHA-256 каждого файла внутри ZIP и создаёт файл `.zip.sha256`. Проверить доступность источников с вашего подключения можно командой `powershell -File scripts/Test-LyricsSources.ps1`.

Собрать установщик из готовой Release-сборки: `powershell -File scripts/Build-Installer.ps1`. Требуется Inno Setup 6 и библиотека MSVC x64 из Visual Studio Build Tools. Скрипт проверяет совпадение версии EXE с `pubspec.yaml` и создаёт установщик с файлом SHA-256; ZIP при этом не создаётся.

## Лицензия

Проект распространяется под лицензией [GPL-3.0](LICENSE).

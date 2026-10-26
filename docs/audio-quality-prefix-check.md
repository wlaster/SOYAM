# Проверка кодеков и битрейта

Authenticated API + at most 512 KiB of unencrypted stream for independent header inspection. No UI/user tests. API accepts codec and quality tier, not an exact bitrate. FLAC bitrate varies.

| Исполнитель — трек | Запрошено, кбит/с | Ответ API | Заголовок файла | Совпало |
|---|---|---|---|---|
| 3 Doors Down — Believer | aac 256 | aac-mp4 192 | — — | нет |
| 3 Doors Down — Believer | flac 1000 | mp3 192 | — — | нет |
| 3 Doors Down — Believer | mp3 320 | mp3 192 | — — | нет |
| 3 Doors Down — Believer | aac 128 | aac 192 | — — | нет |
| 3 Doors Down — Believer | aac 192 | aac-mp4 192 | — — | да |

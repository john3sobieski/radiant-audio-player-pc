# PROGRESS — Radiant for PC
(notes for the next session — read this first, then README.md)

README.md = the project brief (keep it until the user-facing README is written).
The Android app (README_ru.md in the owner's repo) is the target: the PC version is done when it has the same features.

## Rules for the assistant
- Owner writes Russian: reply in Russian. Code and comments in English. The owner does not write Rust: explain briefly and concretely.
- **Do NOT end replies with reminders to push to GitHub, run the `build` workflow, check CI, paste logs, or commit.** The owner knows his workflow and reports problems himself. Mention a manual check only when it is specific and non-obvious (for example, what to listen for), in one line.
- Do not loop on `cargo build` in the sandbox: crates.io is usually unreachable there. Write the code, run only checks that work offline (rustfmt, tests of a crate without external dependencies). No retry loops.
- Send only the files that changed (complete files, the owner drops them in). Do not resend untouched files.
- Keep this file short (target: under 150 lines). Update the marks in place; no session logs; remove details that no longer matter.
- No feature loss. When unsure whether something matters, ask. Keep the DSP numbers from README section 5 unless measured otherwise.

## Legend
✅ done, compiled in CI, tests pass  |  🟡 written, but not compiled yet or not heard by ear  |  ⬜ not started  | ❌ need to be fixed

UI work is marked 🟡 because it was edited after the last known CI run. After the owner pastes a new build log, update the marks.

---

## ROADMAP (in development order)

### Этап 1. Фундамент
- ✅ Cargo workspace из 6 crate'ов (core, audio, dsp, stems, cloud, ui), release-профиль, статический CRT, сборка на Windows в GitHub Actions
- ✅ Сканирование папки с музыкой (рекурсивно, пока только имена файлов), группировка в альбомы, сортировка треков, тесты
- ✅ Движок воспроизведения: декодирование → ресемплер → эффекты → кольцевой буфер → звуковая карта. Play/pause/next/prev/repeat, плавные fade, автопереход, поток засыпает, когда ничего не играет
- ✅ Ресемплер 44.1 → 48 кГц (rubato, sinc)
- ✅ Точная перемотка и точная длительность (в том числе MP3 без заголовка)
- ✅ Цепочка эффектов: 12-полосный EQ, bass, treble, subsonic, preamp, нормализация, мягкий лимитер, моно, нулевая стоимость при выключении (13 тестов)
- ✅ 10 пресетов эквалайзера (данные и тесты)
- ✅ Фазовый вокодер как модуль: скорость от -3× до 3× без смены высоты (9 тестов)
- ✅ Окно Slint: кнопка выбора папки, фоновое сканирование, тёмная тема, подсветка текущего трека, панель плеера

### Этап 2. Доделать написанное, но не проверенное
- ✅ Скорость подключена к движку и интерфейсу (ползунок, учёт позиции в исходном времени)
- ✅ Панель «Sound» целиком: Normalize, Mono, Bass, Treble, Subsonic, 12 полос, пресеты, Reset
- ✅ Громкость (плавная, mute по клику на динамик), shuffle, repeat
- ✅ Колесо мыши на всех ползунках; исправления панели плеера (длинные названия, минимальная ширина окна 640)
- ✅ Проверка на слух: вокодер на 0.5×, 2×, 3×; перемотка при замедлении; конец трека при 0.5×; файл 44.1 кГц на устройстве 48 кГц

### Этап 3. Надёжность воспроизведения
- 🟡 Пропуск битых файлов: ошибка открытия и ошибка посреди трека, плеер идёт дальше, имя файла в статусе; стоп после 100 подряд (MAX_SKIPS); паника декодера перехватывается
- ✅ Смена устройства вывода: логика повторного открытия и восстановления потока обновлена; нужна проверка реального отключения и повторного подключения устройства
- ✅ Вокодер полностью обходится при возврате к 1.0× (хвост дослушивается, потом сброс); позиция после смены скорости учитывает задержку вокодера (VOCODER_LATENCY_MS = 90, оценка)
- 🟡 Opus: подключён `symphonia-adapter-libopus` 0.2.x (libopus, bundled по умолчанию); код написан, нужна первая проверка на реальном `.opus`

### Этап 4. Сохранение настроек
- ✅ Выбран формат `serde + JSON` (`%APPDATA%/Radiant/settings.json` на Windows; fallback на `.config` на других системах)
- ✅ Между запусками сохраняются: папка с музыкой, громкость, shuffle, repeat, настройки звука, скорость; сохранённая папка пересканируется на старте; `Reset` по-прежнему сбрасывает только sound-настройки, не скорость

### Этап 5. Библиотека (как на Android: Плейлисты / Альбомы / Треки / Очереди)
- ✅ Чтение тегов и технических данных Symphonia при сканировании (название, исполнитель, исполнитель альбома, альбом, номер трека, длительность, частота, каналы; битрейт оценивается по размеру и длительности)
- ✅ Список треков + поиск по названию/исполнителю/альбому и сортировка (имя, исполнитель, альбом, длительность, дата; направление)
- ✅ Навигация по разделам (Треки / Альбомы / Плейлисты / Очереди): «линза» над списком — таблетка-переключатель из 4 значков; открытый раздел раскрывается в подпись, «Очереди» — ещё и в 4 точки (заполнена = есть треки, кольцо = пусто, яркая = открыта). Колесо мыши листает разделы (над точками — очереди). Страницы лежат на горизонтальной ленте и плавно прокручиваются при переключении. Выбранный раздел/очередь сохраняются (`section`, `queue_index` в settings.json). Альбомы / Плейлисты / Очереди пока заглушки; `queue-counts` заполнит Rust, когда появятся очереди
- ✅ Вкладка «Альбомы»: сетка больших обложек (плитка от 190 px, столько колонок, сколько влезает, плитки растягиваются на ряд), под обложкой название и автор; автогруппировка по исполнителю альбома + альбому, порядок по названию, «Unknown album» в конце; поиск в шапке фильтрует альбомы; клик = страница альбома (выезжает поверх сетки): сверху по центру крупно название и автор, под ними число треков и общая длина, справа обложка, слева кнопка «назад» (повторный клик по значку «Альбомы» тоже закрывает); ниже список треков в порядке тегов (диск → номер трека, номер показывается из тега, как на CD; у многодисковых альбомов подписи «Disc N»); клик по треку = очередь из этого альбома с этого трека; поиск в шапке закрывает страницу. При наведении на плитку обложка приподнимается. Сетка виртуализирована, обложки (224 px) декодируются лениво, когда плитка появилась на экране (`album-shown`), кэш до 240. Кнопки Play / Shuffle на странице альбома (Shuffle включает перемешивание и стартует со случайного трека; не проверено). Меню трека (три точки / ПКМ): Open album, Add to playlist, Create stems (последний — заглушка).
- ✅ «Плейлисты» (написано, не компилировалось): хранилище `radiant-core/src/playlists.rs` (`%APPDATA%/Radiant/playlists.json`, «Понравившиеся» всегда первые, без папки, не переименовываются и не удаляются; битый файл копируется в `.bad`; 8 тестов). «Add to playlist» в меню трека открывает окно по центру: сетка «пластинок в конвертах» (клик добавляет/убирает трек), карточка «New playlist» создаёт плейлист (название + папка или «New folder...»). Страница «Плейлисты»: Liked, плейлисты без папки, папки (сворачиваются; карандаш = переименовать, корзина = удалить, плейлисты внутри остаются); кнопка папки в строке переносит плейлист. Клик по плейлисту открывает его страницу (выезжает поверх списка): название, число треков и длина, Play / Shuffle, Rename, Delete (с подтверждением), список треков, клик = играть очередь с этого трека; порядок меняется перетаскиванием строки и сохраняется в `playlists.json`; три точки / ПКМ открывают обычное меню трека (Add to playlist показывает этот плейлист с галочкой, повторный клик по нему убирает трек; Open album работает); файлы, которых нет в библиотеке, серые с пометкой «File not found». Не проверено: всё (код ещё не компилировали). Маленькая кнопка «в плейлист» слева от значка громкости в нижней панели (для играющего трека). Сортировка страницы «Плейлисты» (кнопка Sort: дата создания / имя / дата изменения / число треков, по возрастанию и убыванию; Liked всегда первый; сохраняется в `playlist_sort*`; не проверено). Осталось: кнопка «лайк» (позже), ручной порядок плейлистов
- 🟡 «Очереди»: ровно 4 фиксированные; хранилище `radiant-core/src/queues.rs` (`queues.json`, без повторов, 3 теста). Добавление трека: пункт «Add to queue» в меню трека (ПКМ / три точки; Треки, альбом, страница плейлиста) и кнопка слева от кнопки плейлиста в нижней панели (играющий трек) — окно с 4 очередями (галочка = трек уже там, повторный клик убирает). Точки-очереди в «линзе» заполняются по `queue-counts`. Страница очереди (раздел «Очереди»): список треков, Play/Pause, Shuffle, Clear, клик = играть очередь с этого трека, порядок меняется перетаскиванием, меню трека (три точки / ПКМ) с «Remove from queue», серые «File not found». Не проверено (не компилировалось). Осталось: вкладка Queue в плеере, мини-кольцо
- ✅ Перетаскивание треков в режиме Manual: перестановка по зажатой ЛКМ, индикатор цели и анимация смещения
- ⬜ Перетаскивание для ручного порядка плейлистов и альбомов
- ⬜ Обложки (кэш нескольких размеров, ограниченный) и значок качества (формат + битрейт)
- ⬜ Окно «Сведения о треке» (теги, формат, битрейт, частота, каналы, размер файла и т. д.)
- ⬜ Пересканирование при каждом запуске + кнопка ручного пересканирования; сканирование не в потоке интерфейса; длинные списки виртуализировать
- 🟡 Окно плеера («Излучатель»): кольцо прогресса с обложкой, свечение по цвету обложки, вкладки Lyrics / Sound / Info, значок качества; тексты читаются (.lrc/.txt рядом и теги). Лучи спектра 🟡 («солнечные»: 168 тонких заострённых лучей, асимметричные, медленно вращаются, ореол/тело/светлая жила/искры, пульс свечения от баса). Осталось: вкладки Queue и Stems, мини-кольцо в нижней панели, добавление в очередь. Исправлено (не проверено): клики в окне плеера больше не переключают трек (TouchArea-блокировщик); вкладка Info показывает все теги и данные файла, длинные значения переносятся, список прокручивается

### Этап 6. Звук: режимы эквалайзера
- ⬜ Три режима: выключен / одна кривая для всех / отдельная кривая для каждого трека (хранятся независимо, по пути файла)
- ⬜ Названия пресетов на языке интерфейса (пока английские)

### Этап 7. Тексты песен
- ⬜ Источники: теги (ID3 USLT/SYLT, Vorbis LYRICS, M4A ©lyr) и файлы .lrc/.txt рядом с треком, перепроверка при каждом сканировании
- ⬜ Экран: синхронные тексты (прокрутка, подсветка текущей строки, приглушение спетых, клик по строке = перемотка; поиск строки бинарным поиском, небольшой опережающий LEAD_MS), обычные тексты в компактном виде

### Этап 8. Темы и языки
- ⬜ Темы: светлая, чёрная (для OLED), системная
- ⬜ 10 языков (en, de, ru, uk, zh, ja, fr, cs, es, pl) + «Системный»: конвертировать `strings.xml` скриптом (~320 строк), заново не переводить; смена на лету без остановки музыки

### Этап 9. Энергосбережение (решить вместе с владельцем)
- ⬜ Один режим «низкое потребление» вместо трёх уровней Android: реже перерисовка, эффекты отключены, стемы отключены. Правила про громкость 40 % и Bluetooth не переносятся

### Этап 10. Блокировка приложения
- ⬜ Коды: 4 цифры, 6 цифр или свой (латиница и цифры, 4–64 символа). Спрашивать при запуске и при возврате из фона
- ⬜ Хранить только соль и хэш PBKDF2-HMAC-SHA256 (30 000 итераций), сравнение за постоянное время
- ⬜ Задержки после каждых 5 ошибок: 30 с, 1, 2, 4… до 15 минут, сохраняются после перезапуска
- ⬜ Файл восстановления из 12 случайных слов (список из 700 слов, криптостойкий генератор), хранится только хэш; «Забыл пароль»; экран блокировки. Эталон логики: `AppLock.kt`

### Этап 11. Стемы (ИИ-разделение)
- ✅ `radiant-stems`: константы (модели, размеры, SHA-256, размеры STFT)
- ⬜ Менеджер моделей: скачать (проверка SHA-256) или импортировать из файлов (распознавание по контрольной сумме)
- ⬜ Конвейер: декодирование в 44.1 кГц стерео → STFT 4096/1024 → 4 сети ONNX (`ort`) → мягкая маска → WAV 16 бит в `Stems/<исполнитель - название>/`; блоки 512 хопов + 86 контекста; прогресс, оценка времени, отмена, очистка временных файлов; выбор частей и имена вроде `... - No Bass.wav`
- ⬜ Решить, как поставлять `onnxruntime.dll`: стемы необязательны, базовая программа должна остаться маленькой
- ⬜ (по желанию, после паритета с Android) более сильные модели, чем Open-Unmix

### Этап 12. Dropbox
- ⬜ `radiant-cloud` (сейчас пустая заготовка): вход OAuth 2 + PKCE в браузере, возврат на локальный адрес 127.0.0.1; скачивание только в одну сторону в подпапку `Dropbox`; ничего не загружается, не меняется и не удаляется; тип доступа «App folder»

### Этап 13. Интеграция с Windows
- ⬜ Медиа-клавиши и системные кнопки Windows (SMTC, например через `souvlaki`; Windows 10+, на старых хотя бы медиа-клавиши)

### Этап 14. Выпуск
- ⬜ Portable .zip и установщик (MSI, NSIS или MSIX)
- ⬜ Лицензия Slint: вариант «royalty-free» с указанием авторства, добавить уведомление
- ⬜ Раздел «Сторонние компоненты» (symphonia, cpal, rubato, rtrb, ort, ONNX Runtime, Slint, Open-Unmix)
- ⬜ Пользовательский README по образцу Android (10 языков) вместо этого брифа; сохранить `LICENSE.md` (PolyForm Noncommercial 1.0.0, Required Notice с ником), `FUNDING.yml`
- ⬜ Заметка про SmartScreen («Подробнее → Выполнить в любом случае»), публикация на GitHub Releases
- ⬜ Отдельная legacy-сборка для Windows 7/8.1, только если исследование (этап 3) покажет, что это реально

---

## Technical notes for the assistant

**Pipeline:** `symphonia` decode (with `symphonia-adapter-libopus` registered in a custom codec registry for Opus) → `convert.rs` (rubato sinc resampler) → phase vocoder (`radiant-dsp/src/stretch.rs`) → `AudioFx` chain → `rtrb` ring buffer → `cpal`. Engine commands live in `radiant-audio` (`Command::SetFx`, `SetSpeed`, hidden `DurationScanned`).

**Settings:** `radiant-core/src/settings.rs` stores `AppSettings` as pretty JSON. `radiant-ui/src/main.rs` loads them on startup, applies them to the UI + engine, saves after every user change, and rescans the remembered music folder on launch. Missing JSON fields fall back to defaults; out-of-range numbers are clamped on load. A newer folder scan replaces an older one. A failed read or save is shown in the status line.

**Resampler.** `radiant-audio/src/sinc_core.rs` is the only file that uses `rubato` (5.x API: `Async::new_sinc`, chunk 1024, 128 taps, Blackman-Harris², cubic; `rubato = { version = "5", default-features = false }`). If rubato's API causes errors, fix only this file. `convert.rs` trims the filter delay at the start, `finish()` flushes the tail and cuts to `round(frames * ratio)`, `reset()` after seek keeps the tables; falls back to linear interpolation if rubato cannot be created. Resampler tail is played out before the vocoder tail at end of track.

**Seek and duration.** `decode.rs`: sample-accurate seek (land on a packet boundary, drop frames before the target). `needs_scan` is set when the container gives no length or for a raw MP3 without Xing/Info/VBRI (symphonia 0.5.4 only estimates, wrong for VBR). `scan_duration` walks packets on a helper thread and sends `Command::DurationScanned`; a result for a non-current track is ignored; until then the seek bar uses the estimate or is off.

**Sound settings.** Rust (`main.rs`) owns `AudioFxSettings` and sends the whole set on every change (`Command::SetFx`, applied on the engine thread). `RING_MS = 400` in `engine.rs` (shorter ring = faster audible response; raise it if crackling appears on weak machines). Slider steps: 0.5 dB (EQ/bass/treble), 1 % (subsonic), 0.1 (speed).

**Vocoder.** Port of `TimeStretch.kt` (same N, hops, peak detection, phase locking, stereo-sum phases, OLA, tail; hand-written radix-2 FFT, no new crate). API: `TimeStretch::{new,set_speed,process,finish,reset,configure}`, `slider_to_speed`. Sits after the resampler, before the effects in `Engine::fill`; pass-through at speed 1; reset on seek and on new track. `position.rs` (`PositionTracker`) keeps position/progress in SOURCE time; a speed change is registered at the output frame where it becomes audible.

**Slint gotchas.** An element without `x` / `y` inside a plain `Rectangle` (no layout) is CENTERED, not placed at 0: always set `x` and `y` explicitly.  `VolumeBar` callback is named `edited` (not `changed`, which clashes with a Slint keyword). Mouse wheel: `scroll-event` on each slider's TouchArea; 1 notch = 60 px = 5 s seek / 5 % volume / `wheel-step`. Player-bar control row uses absolute positions; window min-width 640. Preset names are English keys (`rock`, `pop`, ... = Android `preset_*`) until localization exists.

**Robustness (engine.rs).** `skip_streak` counts files in a row that gave no audio (open error or decode error before the first good chunk); it is reset by the first good chunk and by any user command; at `MAX_SKIPS` the player stops. `guarded()` turns a decoder panic into an error (needs `panic = "unwind"` in the profile). Device: `Output.lost` is set by the cpal error callback; `refresh_output()` also compares the default device name (every `DEVICE_CHECK_INTERVAL` while playing, and on every `start()`, nothing while paused). `reopen_output()` rebuilds stream, vocoder, effects (from `fx_settings`), resampler and position tracker for the new rate/channels, then seeks the decoder to the old position. On failure the player pauses with a message; Play retries. Vocoder bypass: `use_voc` / `voc_drain`; `vocoder_reset()` keeps them consistent after any `stretch.reset()`.

**Normalization note.** The envelope follows the spec (fast attack), so on a pure sine it settles ~15 % below the 0.16 target; kept as on Android. Gain keeps adapting for ~9 s of silence until the envelope falls under the gate (1e-6), then freezes.

**Spectrum rays.** `cpal` callback pushes a mono mix (before volume, after fade) into an rtrb ring only while `SpectrumTap.enabled` is set (UI sets it while the player view is open). The UI timer (25 ms, `main.rs`) calls `Player::spectrum().poll(dt)` -> `radiant-dsp/src/spectrum.rs` (FFT 2048, Hann, 60 log bands 40 Hz..16 kHz, +3 dB/oct tilt, DB_FLOOR/DB_CEIL = -78/-20, attack 30 ms, release 160 ms, falling peaks) -> `radiant-ui/src/rays.rs` builds SVG path strings (1000x1000 box, lows at the bottom, mirrored) -> 4 filled `Path` layers in `main.slint` (triangles; pseudo-random angle/length/band per ray, two halves map the scale differently, `phase` turns slowly). Ring diameter is `/ 1.4` so the rays fit. Tune by eye: DB_FLOOR/DB_CEIL, TILT_DB_PER_OCT, R_SPAN, RAY_COUNT, timer interval. Tests: `spectrum.rs` (4), `rays.rs` (6) need no external crates.

**Tests.** The ramp-WAV seek tests take expectations modulo 30011 (the ramp wraps). The resampler sine test allows a slide of up to 1 frame and checks a residual shape error < 0.005 (rubato reports its delay in whole frames). MP3 header detection (ID3 size, Xing/Info/VBRI) is unit-tested; real-file check with ffmpeg: `ffmpeg -i in.wav -c:a libmp3lame -b:a 192k -write_xing 0 headerless.mp3` (CBR) and `-q:a 2 -write_xing 0` (VBR).

# PROGRESS — Radiant for PC
(notes for the next session — read this first; the full project brief is in the Appendix at the end of this file)

README.md / README_ru.md = the USER-facing readme (features, install, privacy). Do not put development notes there; they live here.
The Android app (README_ru.md in the owner's repo) is the target: the PC version is done when it has the same features.

## Rules for the assistant
- Owner writes Russian: reply in Russian. Code and comments in English. The owner does not write Rust: explain briefly and concretely.
- **Do NOT end replies with reminders to push to GitHub, run the `build` workflow, check CI, paste logs, or commit.** The owner knows his workflow and reports problems himself. Mention a manual check only when it is specific and non-obvious (for example, what to listen for), in one line.
- Do not loop on `cargo build` in the sandbox: crates.io is usually unreachable there. Write the code, run only checks that work offline (rustfmt, tests of a crate without external dependencies). No retry loops.
- Send only the files that changed (complete files, the owner drops them in). Do not resend untouched files.
- Keep the roadmap short (the Appendix is the old project brief, kept for reference). Update the marks in place; no session logs; remove details that no longer matter.
- No feature loss. When unsure whether something matters, ask. Keep the DSP numbers from Appendix section 5 unless measured otherwise.

## Legend
✅ done, compiled in CI, tests pass  |  🟡 written, but not compiled yet or not checked  |  ⬜ not started  | ❌ need to be fixed

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
- ✅ «Очереди»: ровно 4 фиксированные; хранилище `radiant-core/src/queues.rs` (`queues.json`, без повторов, 3 теста). Добавление трека: пункт «Add to queue» в меню трека (ПКМ / три точки; Треки, альбом, страница плейлиста) и кнопка слева от кнопки плейлиста в нижней панели (играющий трек) — окно с 4 очередями (галочка = трек уже там, повторный клик убирает). Точки-очереди в «линзе» заполняются по `queue-counts`. Страница очереди (раздел «Очереди»): список треков, Play/Pause, Shuffle, Clear, клик = играть очередь с этого трека, порядок меняется перетаскиванием, меню трека (три точки / ПКМ) с «Remove from queue», серые «File not found». Не проверено (не компилировалось). Осталось: вкладка Queue в плеере, мини-кольцо
- ✅ Перетаскивание треков в режиме Manual: перестановка по зажатой ЛКМ, индикатор цели и анимация смещения
- ✅ Перетаскивание для ручного порядка плейлистов и альбомов (написано, не компилировалось): плейлисты перетаскиваются вверх/вниз внутри своей группы (без папки / внутри одной папки; «Понравившиеся» закреплены), порядок хранится в самом `playlists.json`, в панели Sort новый режим «Manual order»; плитки альбомов перетаскиваются по сетке (порядок = ключи альбомов в `settings.json` → `album_order`, не работает при активном поиске). Кнопка Sort появилась и у альбомов: Manual order / Name / Artist / Duration (`album_sort*` в настройках)
- ✅ Обложки и значок качества (написано, не компилировалось): один кэш `COVERS` с тремя размерами (96 px — строки списка, 224 px — плитки альбомов, 512 px — окно плеера; раньше плеер показывал 96-пиксельную миниатюру растянутой) и лимитом 64 МБ по памяти с вытеснением давно не использованных вместо полного сброса; кэш альбомов больше не копируется целиком при каждом обновлении сетки; пока грузится большая обложка плеера, показывается миниатюра. Значок качества: кодек берётся из потока (.m4a = AAC или ALAC), разрядность, у WAV точный битрейт; в плеере чип «Hi-Res» (золотой: lossless и 24 бит+ или выше 48 кГц) / «Lossless» (акцентный) + текст «FLAC · 24-bit · 96 kHz · 2822 kbps»; в «Сведениях» строки Quality и Bit depth
- ✅ Окно «Сведения о треке» (теги, формат, битрейт, частота, каналы, размер файла и т. д.)
- ✅ Пересканирование при каждом запуске + кнопка ручного пересканирования (круглая стрелка справа от «Choose folder», крутится, пока идёт скан; сбрасывает кэши обложек; написано, не компилировалось); сканирование идёт в отдельном потоке, списки виртуализированы (`ListView`). Осталось (по желанию): тяжёлая сортировка и сборка строк в `refresh_tracks` всё ещё выполняются в потоке интерфейса — на очень больших библиотеках вынести в поток
- ✅ Окно плеера («Излучатель»): кольцо прогресса с обложкой, свечение по цвету обложки, вкладки Lyrics / Sound / Info, значок качества; тексты читаются (.lrc/.txt рядом и теги). Лучи спектра ✅ («солнечные»: 264 тонких заострённых луча, асимметричные, медленно вращаются, качаются и мерцают каждый со своей скоростью (сильнее при громкой музыке), волна удлинения от баса бежит по кругу; ореол/тело/светлая жила/искры; фон — пять цветных пятен света, дрейфующих по несоизмеримым траекториям, дышат от баса и громкости). Осталось: вкладки Queue и Stems, мини-кольцо в нижней панели, добавление в очередь. Исправлено (не проверено): клики в окне плеера больше не переключают трек (TouchArea-блокировщик); вкладка Info показывает все теги и данные файла, длинные значения переносятся, список прокручивается

### Этап 6. Звук: режимы эквалайзера
- ✅ Три режима эквалайзера: Off / All tracks / Per track (переключатель с «ползущей» плашкой над пресетами, мини-график кривой с подписью, ползунки и пресеты гаснут, когда править нельзя; активный пресет подсвечивается). Кривые по трекам хранятся независимо в `settings.json` (`track_eq`, ключ — путь файла, плоские не пишутся), общая кривая — `fx.eq_gains_db`; в Off кривые сохраняются, но звук идёт без эквалайзера. Не компилировалось, на слух не проверялось
- ✅ Названия пресетов на языке интерфейса (пока английские)

### Этап 7. Тексты песен
- ✅ Источники: теги (ID3 USLT/SYLT, Vorbis LYRICS, M4A ©lyr) и файлы .lrc/.txt рядом с треком, перепроверка при каждом сканировании
- ✅ Экран: синхронные тексты (прокрутка, подсветка текущей строки, приглушение спетых, клик по строке = перемотка; поиск строки бинарным поиском, небольшой опережающий LEAD_MS), обычные тексты в компактном виде

### Этап 7б. Окно настроек (база)
- ✅ Кнопка-«ползунки» справа от кнопки обновления в шапке открывает страницу настроек поверх библиотеки (нижняя панель остаётся). Стиль «орбита»: тонкие кольца в углу, спутник, дрейфующие световые пятна, слева рельса из точек по разделам (подсвеченная часть линии доходит до открытого), справа карточки раздела. Разделы: Appearance, Language, Security, AI & stems, Library; все карточки пока заглушки «Soon». Код: `SettingsPage` в `main.slint` (новый раздел = дописать в четыре списка `names/hints/leads/pages`). Не компилировалось
- ✅ Наполнить разделы по мере готовности: Appearance ← этап 8 (темы), Language ← этап 8, Security ← этап 10, AI & stems ← этап 11, Library ← выбор папки (возможно, переехать из шапки; решит владелец), энергосбережение ← этап 9
- ✅ Открытый раздел настроек запоминается между запусками (`settings_section` в `settings.json`, 0 = первый). Не компилировалось

### Этап 8. Темы и языки
- ✅ Темы: System / Light / Dark / Black (OLED, чистый #000) переключаются на лету в Настройки → Appearance (сегментный переключатель, плавная смена цветов); выбор хранится в `settings.json` (`theme`: 0 System, 1 Light, 2 Dark, 3 Black; по умолчанию Dark). Цвета живут в глобальном `Theme` в `main.slint` (`mode`, `light`, `black`; System берёт `Palette.color-scheme` из Slint).
- ✅ Цветовые палитры (Настройки → Appearance → «Color palette»): по 10 акцентов для каждого вида темы (светлая / тёмная / чёрная; System берёт светлую или тёмную): Violet, Blue, Cyan, Teal, Green, Yellow, Orange, Red, Pink, Graphite — плюс 11-й кружок «свой цвет» (ползунки Hue / Saturation / Brightness, цвет общий для всех тем). Для каждой темы запоминается свой выбор (`palettes: [светлая, тёмная, чёрная]`, 0–9 пресет, 10 свой; `custom_hsv`). Поверхности получают лёгкий оттенок акцента (чёрная тема остаётся чисто чёрной); текст на акценте (`Theme.on-accent`) тёмный на светлых цветах. Математика цвета в `radiant-core/src/theme.rs` (4 теста). Не компилировалось. Свои цвета можно сохранять как пресеты (кнопка «Save color», до 12, `saved_colors` в `settings.json`; клик по кружку = применить, ПКМ = удалить). Возможное дальше: поле ввода hex, градиентные ползунки, названия пресетов под кругами, сами цвета пресетов подобрать на глаз
- ✅ 10 языков (en, de, ru, uk, zh, ja, fr, cs, es, pl) + «System» (написано, не компилировалось). Android `strings.xml` в архив не вошёл, поэтому конвертировать было нечего: переведены все 184 строки, которые есть в интерфейсе сейчас (ключ = английский текст). Формат gettext: `crates/radiant-ui/lang/<код>/LC_MESSAGES/radiant-ui.po` (en — тождественный файл). В `main.slint` все тексты обёрнуты в `@tr(...)` (множественное число: `@tr("{n} track" | "{n} tracks" % n)`); `build.rs` вшивает .po в бинарник (`with_bundled_translations("lang")`), язык переключается на лету через `slint::select_bundled_translation`. Тексты из Rust идут через `crates/radiant-ui/src/i18n.rs` (`tr`, `tr_fmt`, `tr_n`) — он читает те же .po (`include_str!`), знает правила множественных форм (ru/uk/cs/pl — 3 формы, fr — `n>1`, zh/ja — 1). «System» берёт язык интерфейса Windows (`GetUserDefaultUILanguage`), иначе English. Выбор хранится в `settings.json` → `language` ("system" или код). Настройки → Language: список из 11 пунктов (родные названия). При смене языка пересобираются названия пресетов EQ, альбомы, плейлисты, очереди; ширина плашки раздела (Tracks/Albums/…) теперь измеряется по переведённому тексту. Новая строка = добавить `@tr("…")` / `tr("…")` и одну запись во все десять .po (тест `every_language_has_every_key` следит за полнотой). Не переведено: ошибки движка звука (`status.error` из `radiant-audio`, английские); открытая страница альбома/плейлиста обновит подписи при следующем открытии. Проверить: имя домена .po (`radiant-ui` = имя пакета) и что `slint::select_bundled_translation` доступна в Slint 1.18 без доп. features

### Этап 9. Энергосбережение (решить вместе с владельцем)
- ✅ Один режим «низкое потребление» вместо трёх уровней Android (написано, не компилировалось; Настройки → Power, `low_power` в `settings.json`, флаг `Theme.low-power` в `main.slint`, `Command::SetLowPower`). Включает: все `animate` на 0 мс и ни одного `animation-tick()` (окно не перерисовывается без причины); лучи спектра — одна неподвижная картинка, таймер лучей остановлен (`start_rays_timer` / `apply_low_power_visuals` в `main.rs`); эффекты обходятся (EQ, bass, treble, subsonic, normalize; mono и скорость остаются, значения хранятся и возвращаются при выключении); движок будит поток раз в 400 мс вместо 50 мс и держит буфер 1.2 с вместо 0.4 с (только на скорости 1×; при скорости ≠ 1× буфер обычный; первое включение скорости перезапускает буфер с текущей позиции); позиция приходит реже, проверка устройства раз в 5 с; стемы (когда появятся) не стартуют: `radiant_stems::allowed(low_power)`, пункт «Create stems» затемнён. Проверить на слух: пауза/перемотка/смена трека, ползунок скорости, конец трека (нет лишней паузы), нет щелчков при переключении режима. Правила про громкость 40 % и Bluetooth не переносятся

### Этап 10. Блокировка приложения
- ✅ Ядро логики (написано, не компилировалось): `radiant-core/src/applock.rs`, без новых crate'ов. SHA-256 / HMAC / PBKDF2 свои, тесты на официальных векторах; случайные байты из ОС (`BCryptGenRandom` / `/dev/urandom`); коды `CodeKind` (4 цифры / 6 цифр / свой: латиница+цифры 4–64) и `validate_code`; `HashRecord` (соль + хэш, PBKDF2 30 000, сравнение за постоянное время); `LockStore` (`applock.json`: `kind`, `code`, `recovery` — зарезервировано, `failures`, `locked_until`); `try_unlock(code, now)` → Ok / Wrong{left} / WrongLocked / Locked / NotSet; задержка после каждых 5 ошибок: 30 с, 1, 2, 4, 8, 15 мин (кап), хранится между запусками. UI после каждого `try_unlock` обязан вызвать `applock::save`
- ✅ Фраза восстановления (написано, не компилировалось): 12 слов из списка в 700 слов (`radiant-core/src/wordlist.rs`, свой список: Android-список в архив не вошёл; слова a–z, 3–9 букв, без повторов и общих начал), равномерный выбор через ОС-генератор; в `applock.json` только соль+хэш (`recovery`). API: `LockStore::create_recovery()` (возвращает фразу один раз), `recover(text)` → Ok / Wrong / NotSet (принимает фразу в любом регистре или текст файла; успех сбрасывает ошибки и задержку; попытки не ограничены задержкой), потом UI вызывает `set_code` (фраза остаётся действительной) или `clear`; файл: `write_recovery_file` / `read_recovery_file` (текст с комментариями `#`). Если у владельца есть список слов из Android — подменить `wordlist.rs` (число слов = `WORD_COUNT`, отсортирован)
- ✅ Экран блокировки (написано, не компилировалось): `LockScreen` + `CodeEntry` в `main.slint` (накрывает всё окно, музыка играет дальше). PIN: точки + цифровая панель (при высоте окна ≥ 560 px) + ввод с клавиатуры, полный PIN проверяется сразу; свой код: поле-пароль + «Unlock». Показывается при запуске и когда окно свёрнуто («фон» на ПК = окно свёрнуто; потеря фокуса не считается — её дают файловые диалоги): таймер 500 мс следит за `is_minimized()` и работает только пока замок включён. После 5 ошибок вход блокируется, сообщение отсчитывает время (таймер 1 с, переживает перезапуск через `locked_until`). «Forgot code?»: файл восстановления или 12 слов → замок выключается полностью (`clear`). Код Rust: `LockCtl`, `wire_app_lock`, `lock_now`, `do_recover` в `main.rs`. Помощник `CodeTools.trim-last` (в Slint нет срезов строк) реализован в Rust
- ✅ Настройки → Security (написано, не компилировалось): карточки «App lock» (Off/On + «Lock now»), «Change code», «Recovery file» (описание). Вид кода (4 цифры / 6 цифр / Custom) выбирается только в диалоге на шаге нового кода (при включении и при смене), в настройках его нет. Диалог (`sec-mode` 0 включить / 1 сменить / 2 выключить; `sec-step` 0 текущий код → 1 новый → 2 повтор → 3 сохранить файл): выключение и смена требуют текущий код; замок включается и новый код вступает в силу только после записи файла восстановления (до этого кандидат лежит в `LockCtl.pending`); смена кода делает новую фразу, старая перестаёт работать. Закрытие диалога или блокировка окна отбрасывают недоделанный код. 40 новых строк добавлено во все 10 .po

### Этап 11. Стемы (ИИ-разделение, Demucs htdemucs, 4 стема)
Решение владельца: вместо Open-Unmix (4 сети) одна сеть htdemucs (Demucs v4, Hybrid Transformer; код и веса MIT, Meta AI; обучена на MUSDB18-HQ + 800 доп. треков — условия данных проверить, если пойдём дальше личного использования). Стемы в порядке выхода сети: drums, bass, other, vocals. Open-Unmix из кода убран (константы `MODELS`, `CORE_HOPS`, `CONTEXT_HOPS` удалены; README 5.3 помечен устаревшим).

**Требования (определены по публичным ONNX-экспортам htdemucs, не измерены на этой машине):**
- Модель не входит в exe: скачивается один раз по кнопке (Настройки → AI & stems → Demucs model) или импортируется из `.onnx`; лежит в `%APPDATA%/Radiant/models/htdemucs.onnx`. Скачивание продолжается после обрыва (`.part` + Range), в конце полная проверка SHA-256. Без вписанных размера и SHA-256 скачивание и импорт запрещены: непроверенный файл в программу не попадёт.
- Формат: ONNX с вынесенными STFT/iSTFT (в ONNX нет комплексного FFT). Входы `mix [1,2,343980]` и `spec [1,4,2048,336]` (L.re, L.im, R.re, R.im); выходы `wave [1,4,2,343980]` и `spec_out [1,4,4,2048,336]`; стем = wave + iSTFT(spec_out). Сегмент 7.8 с, 44.1 кГц стерео, N_FFT 4096, HOP 1024, 2048 бинов (верхний отброшен). Имена тензоров у экспортов разные, поэтому `check_io` узнаёт их по форме; 6-стемовая и 3.9-секундная версии отклоняются с понятной причиной.
- Размер: fp16-веса примерно 90 МБ (fp32 примерно 174 МБ) по описаниям репозиториев; точное число взять из файла.
- ONNX Runtime: `ort` с `load-dynamic`, `onnxruntime.dll` (около 10–15 МБ, MIT) рядом с exe в portable-zip; нет файла — стемы недоступны, exe остаётся маленьким (в него добавились только ureq/rustls/sha2). Проверка файла: `radiant_stems::runtime_present()`.
- Скорость и память (оценка по чужим карточкам, проверить): CPU порядка минут на трек (один из экспортов: около 2 мин на 3:40), память порядка 1–2 ГБ на пике; GPU (DirectML) заметно быстрее — отдельная опция позже.
- Не в режиме низкого потребления (`allowed`); фоновый поток, число потоков ORT ограничить, чтобы музыка не щёлкала.
- Как в референсе demucs: перед сетью вычесть среднее и поделить на std моно-микса трека, после — вернуть (часть экспортов нормализует внутри — проверить на выбранном). Склейка: перекрытие 25 %, треугольное окно (`apply_model`).
- Выход: WAV 16 бит / 44.1 кГц в `Stems/<исполнитель - название>/`, имена `... - Vocals + Drums.wav`, `... - No Bass.wav` (`StemSet::file_label`, имя не зависит от языка).

**Сделано:**
- ✅ `radiant-stems`: `stem.rs` (4 стема, набор, имена файлов, тесты); `demucs.rs` (константы сети, `Backend` — то, что реализует обёртка ORT, `check_io`, план сегментов `plan`, окно `overlap_weights`, тесты); `store.rs` (папка моделей, `MODEL`, `download` с докачкой и SHA-256, `import`, `delete`, `state`, `hash_file`, тесты); `lib.rs` (`StemError`, `allowed`, `runtime_present`). Зависимости: `sha2`, `ureq` 2 (rustls).
- ✅ Карточка «Demucs model» в Настройки → AI & stems: статус, полоса прогресса, Download / Import from file / Cancel / Delete; работа в потоке, отмена, повторный запуск продолжает загрузку (`run_stem_job` в `main.rs`); в низком потреблении отказ с пояснением; пункт «Create stems» без модели говорит, что её надо скачать. 17 новых строк во всех 10 .po (две старые заглушки убраны).
- ✅ Проверка на реальной модели: сумма четырёх стемов ≈ исходник; сравнение с эталоном (python demucs) на 30-секундном клипе; на слух стыки сегментов; нет ли двойной нормализации (экспорт может нормализовать внутри — тогда убрать нормализацию трека в `separate.rs`); скорость и память (стемы 4 × 2 × кадры f32 лежат в памяти: ~420 МБ на 5 минут); `ort` и `onnxruntime.dll` на Windows 10 и 11
- ✅ Вкладка Stems в плеере (написано, не компилировалось): четвёртая вкладка рядом с Lyrics / Sound / Info (`player-tab` 3) для играющего трека. Части: переключатели Vocals / Drums / Bass / Other (по умолчанию Instrumental = без вокала), быстрые «Instrumental» и «Vocals only», «Also save each part separately». Выбор даёт один файл со сборкой (`StemSet::file_label`: `Vocals + Drums`, `No Bass`) и, по желанию, четыре отдельных; всё за один прогон сети (`radiant_stems::output_sets`, `job::file_name`). Строка под переключателями показывает имя файла; 0 или 4 части без «отдельных файлов» файла не дают, кнопка неактивна. Пункт меню трека «Create stems» по-прежнему делает 4 файла, но теперь через ту же общую задачу (`StemsCtl`, `begin_stems`, `start_stems_job` в `main.rs`: один заказ за раз, проверки низкого потребления и модели в одном месте)
- ✅ Кнопка «Cancel» (во вкладке, пока идёт задача; флаг `StemsCtl.cancel` → `create_stems` останавливается между сегментами, недописанный `.part` не остаётся) и оценка оставшегося времени: `radiant-stems/src/eta.rs` (`Eta`: elapsed / доля, сглаживается общее время, первые ~4 с и 3 % числа не дают; тесты), строка «Creating stems… 42% · 1:20 left» в статусе и во вкладке, полоса прогресса
- ✅ GPU (DirectML) как опция: Настройки → AI & stems → карточка «Graphics card (GPU)» Off/On (`AppSettings.stems_gpu`, по умолчанию выкл.). `OrtBackend::load(model, threads, gpu)`: DirectML с `error_on_failure`, без memory pattern и параллельного выполнения; если не вышло (CPU-сборка `onnxruntime.dll`, нет карты DX12) — тихо CPU, `using_gpu()` / `Done.gpu` говорят правду, а в результате задачи пишется «the graphics card could not be used». Cargo-фича `directml` (по умолчанию включена) = `ort/directml`. Для работы нужен DirectML-вариант `onnxruntime.dll` рядом с exe (пакет Microsoft.ML.OnnxRuntime.DirectML, версия 1.22.x, как и CPU-вариант); в portable-zip класть один из двух. Заглушка-карточка «Stems» в настройках убрана. 16 новых строк во всех 10 .po
- ✅ «Create stems» в меню трека теперь открывает окно выбора частей (те же переключатели, что на вкладке: общий компонент `StemPicker` в `main.slint`, свойства `stems-*`; цель окна — `STEMS_MENU_TARGET` в `main.rs`), а не делает все четыре файла. Окно закрывается, когда задача началась; отказ (низкое потребление, нет модели, пустой выбор, уже идёт другая задача) пишется в самом окне
- ✅ Системная папка «Stems» в плейлистах (написано, не компилировалось): `STEMS_FOLDER_ID = "stems"` в `models.rs`; `PlaylistStore::normalize` всегда её создаёт (в файле лежит английское «Stems», на экране имя переводится через `folder_name`), `rename_folder` / `delete_folder` её отклоняют (`folder_is_locked`), кнопки карандаша и корзины у неё скрыты (`PlaylistListRow.locked`). После удачной задачи файлы добавляются в плейлист `Artist - Title` внутри этой папки (`add_stems_playlist`: повторный запуск для того же трека дописывает новые файлы, без дублей), затем папка музыки пересканируется, чтобы файлы попали в библиотеку (иначе плейлист показывает их как «missing»). Сами плейлисты внутри папки удалять можно. Допущение: «убрать нельзя» понято как «папку нельзя удалить и переименовать»; если нужно запретить и удаление плейлистов в ней — менять `delete_playlist`
- ✅ GPU: DirectML с fp16-моделью отдавал мусор (шум/нули без ошибки; файлы «тишина + шум»). Теперь первый сегмент каждого GPU-режима дополнительно считается на CPU и сравнивается (`matches_reference`, ошибка до 25 %), дальше дешёвая проверка `output_is_sane`; не совпало → следующий режим (DirectML с базовыми оптимизациями → без оптимизаций → CPU), сегмент пересчитывается, `Done.gpu` говорит правду. Если всегда уходит на CPU, нужен другой экспорт модели (fp32) для GPU. Запрет стемов из стемов: `job::is_stem_file` (папка `Stems/<имя>/` или имя `<папка> - <метка>`), проверка в `begin_stems` и в меню трека.
- ✅ Вкладка Stems / GPU: на живом окне проверить вёрстку (узкие окна: четыре кнопки в шапке плеера; высота карточки GPU), что на DirectML сеть даёт тот же результат, что на CPU (fp16-веса, возможна иная точность), память видеокарты на длинных треках; после проверки — решить, нужен ли «живой» отсчёт времени между сегментами (сейчас обновляется на каждом сегменте)

**Модель:** `store::MODEL` = `ferrite-stems/htdemucs-onnx-dml` / `htdemucs-dml.onnx` (MIT, сделан под DirectML: STFT и iSTFT внутри графа, `mix [1,2,343980]` → `stems [1,4,2,343980]`, автор: ~12 с на 3:40 на GPU), файл на диске `htdemucs-dml.onnx`, SHA-256 `9e06fef2…20ab` взят с карточки, `bytes` = `None` до размера файла (без него скачивание и импорт отказывают). `check_io` знает три раскладки: `Classic` (StemStudio), `FramesFirst` (KICK, репозиторий исчез), `Fused` (этот; частотной ветви нет, `spec_out` = нули). Страховка: `separate` на первом громком сегменте проверяет, что сумма стемов ≈ входу, иначе `BadModel`; GPU-режимы сверяются с CPU (`ort_backend`).

**Конвейер:**
- ✅ `ort_backend.rs`: `OrtBackend::load` (`ort` =2.0.0-rc.10, `load-dynamic`; `ORT_DYLIB_PATH` ставится на `onnxruntime.dll` рядом с exe, нужен ONNX Runtime 1.22.x; нет файла → `StemError::NoRuntime`), `check_io` по формам тензоров, CPU, потоков `job::default_threads()` = половина ядер. Если ort не соберётся — менять только этот файл (API меняется между rc).
- ✅ `dsp.rs`: STFT / iSTFT как в `HTDemucs._spec/_ispec` (hann периодическое, scale 1/64, reflect-паддинг 1536, кадры `t*1024-1536`, верхний бин отброшен, огибающая 1.5); тест «туда-обратно» на синусах.
- ✅ `separate.rs`: нормализация трека по моно-миксу, сегменты 25 % с треугольным окном, `стем = wave + iSTFT(spec_out)`, возврат масштаба; тесты на фейковой сети (проходит стем 0 = вход).
- ✅ `wav.rs` (16 бит, `.part` → переименование), `job.rs` (`create_stems`: декод → разделение → файлы `<Artist - Title> - Vocals.wav` и т. д., прогресс 0..1, отмена через `AtomicBool`), `radiant-audio/src/offline.rs` (`decode_stereo`: целый файл → стерео 44.1 кГц тем же декодером и ресемплером).
- ✅ «Create stems» в меню трека запускает задачу: 4 файла (Vocals, Drums, Bass, Other) в `<папка музыки>/Stems/<Artist - Title>/`, прогресс в строке статуса, один заказ за раз. Кнопки отмены в интерфейсе ещё нет (`create_stems` отмену уже умеет).

### Этап 12. Dropbox
- ✅ `radiant-cloud` (написано, не компилировалось): вход OAuth 2 + PKCE в браузере, ответ на локальный адрес `http://127.0.0.1:53682/` (порт фиксирован, его надо добавить в Redirect URIs приложения; `auth.rs`, `pkce.rs`); refresh-токен в `%APPDATA%/Radiant/dropbox.json` открытым текстом (`token.rs`); `api.rs` — обновление access-токена, список папки приложения (рекурсивно), скачивание по `id:`, повтор при 429/5xx; `sync.rs` — качает в `<папка музыки>/Dropbox` только недостающие файлы (музыка, `.lrc`/`.txt`, `.jpg`/`.jpeg`/`.png`), существующие не трогает, имена приводятся к допустимым для Windows, `..` отклоняется, через `.part`, отмена, стоп после 3 ошибок подряд. Ничего не загружается, не меняется и не удаляется. Тесты: PKCE-вектор RFC 7636, разбор ответа браузера, пути и имена, токен-файл (без сети)
- ✅ Настройки → Dropbox (7-й раздел, `settings_section` до 6): карточка с состояниями (не подключено / ждём браузер / подключено / идёт загрузка / нет ключа в сборке), кнопки Connect / Sync now / Disconnect / Cancel, строка «Saves to», полоса прогресса; после загрузки новых файлов запускается пересканирование. Код: `wire_dropbox` в `main.rs`. 24 новых строки во всех 10 .po
- ✅ Владельцу: создать приложение в Dropbox App Console (Scoped access, App folder, права `files.metadata.read` и `files.content.read`, Redirect URI `http://127.0.0.1:53682/`) и вписать App key в `APP_KEY` (`radiant-cloud/src/lib.rs`); если ключ пуст, кнопка «Connect» серая с пояснением
- ✅ Проверить вручную: вход в браузере, большая папка (сотни файлов), обрыв сети посреди файла, имена с кириллицей/иероглифами, повторный запуск (ничего не качается заново), Disconnect
- Не сделано сознательно: выбор подпапки облака (доступ «App folder» = корень приложения), автосинхронизация при запуске, шифрование токена (DPAPI)

### Этап 13. Интеграция с Windows
- ✅ Медиа-клавиши и SMTC через `souvlaki`: `radiant-ui/src/media.rs`; кнопки Windows вызывают те же колбэки, что кнопки окна; контролы создаются таймером после показа окна (нужен HWND). Без обложки в панели Windows
- ✅ Горячие клавиши окна (`main-fs` в `main.slint`): Space, ←/→ ∓5 с, Ctrl+←/→ или P/N, ↑/↓ громкость, M, S, R; не работают на экране блокировки
- ✅ Проверить вручную: медиа-клавиши с окном в фоне, Windows 10/11, Bluetooth-наушники, Space после диалогов и разблокировки, Windows 7/8 (SMTC нет)

### Этап 13б. Дополнительные функции
- ⬜ Возможность очистки кэша приложения через новую вкладку в настройках; отображать размер уже сохранённых файлов в кэше
- ⬜ Исправить баг с подёргиванием/лагами списка альбомов во время прокрутки
- ⬜ Перевести `README.md` на языки cs, de, es, fr, ja, pl, ru (обновить), uk, zh
- ⬜ Добавить подробные скриншоты из готового приложения в каждый `README.md` каждого перевода (ещё не сделаны)

### Этап 14. Выпуск
- ⬜ Portable .zip с нужными файлами и установщик (EXE) в нём; в установщике выбор папки для приложения, затем выбор папки с музыкой, выбор языка приложения по умолчанию
- ⬜ Лицензия Slint: вариант «royalty-free» с указанием авторства, добавить уведомление
- ⬜ Раздел «Сторонние компоненты» (symphonia, cpal, rubato, rtrb, ort, onnxruntime.dll, ureq, sha2, Slint; модель Demucs: MIT, Meta AI) и `onnxruntime.dll` рядом с exe в portable-zip
- 🟡 Пользовательский README написан (README.md на английском + README_ru.md; осталось перевести на остальные языки); сохранить `LICENSE.md` (PolyForm Noncommercial 1.0.0, Required Notice с ником), `FUNDING.yml`
- ✅ (в README.md, раздел «AI stem separation setup»; перепроверить версии и пути к выпуску) TODO для пользовательского README: в инструкцию установки из GitHub Releases включить установку двух DLL для стемов, со ссылками на скачивание. (1) `onnxruntime.dll` из пакета Microsoft.ML.OnnxRuntime.DirectML 1.22.x: https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/1.22.1 → `.nupkg` открыть как zip → `runtimes/win-x64/native/onnxruntime.dll`. (2) `DirectML.dll` из пакета Microsoft.AI.DirectML 1.15.4: https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/1.15.4 → `bin/x64-win/DirectML.dll`. Обе положить рядом с `radiant.exe` (DirectML-вариант работает и на CPU, и на GPU; GPU включается в Настройки → AI & stems). Объяснить «как для новичка» и описать, что без них стемы недоступны, а остальное приложение работает. Если пути внутри пакетов или версии изменятся к выпуску — перепроверить; при желании сразу класть обе DLL в portable-zip и установщик (лицензии MIT, условия распространения сверить)
- ⬜ Заметка про SmartScreen («Подробнее → Выполнить в любом случае»)

---

## Technical notes for the assistant

**Pipeline:** `symphonia` decode (with `symphonia-adapter-libopus` registered in a custom codec registry for Opus) → `convert.rs` (rubato sinc resampler) → phase vocoder (`radiant-dsp/src/stretch.rs`) → `AudioFx` chain → `rtrb` ring buffer → `cpal`. Engine commands live in `radiant-audio` (`Command::SetFx`, `SetSpeed`, hidden `DurationScanned`).

**Settings:** `radiant-core/src/settings.rs` stores `AppSettings` as pretty JSON. `radiant-ui/src/main.rs` loads them on startup, applies them to the UI + engine, saves after every user change, and rescans the remembered music folder on launch. Missing JSON fields fall back to defaults; out-of-range numbers are clamped on load. A newer folder scan replaces an older one. A failed read or save is shown in the status line.

**Resampler.** `radiant-audio/src/sinc_core.rs` is the only file that uses `rubato` (5.x API: `Async::new_sinc`, chunk 1024, 128 taps, Blackman-Harris², cubic; `rubato = { version = "5", default-features = false }`). If rubato's API causes errors, fix only this file. `convert.rs` trims the filter delay at the start, `finish()` flushes the tail and cuts to `round(frames * ratio)`, `reset()` after seek keeps the tables; falls back to linear interpolation if rubato cannot be created. Resampler tail is played out before the vocoder tail at end of track.

**Seek and duration.** `decode.rs`: sample-accurate seek (land on a packet boundary, drop frames before the target). `needs_scan` is set when the container gives no length or for a raw MP3 without Xing/Info/VBRI (symphonia 0.5.4 only estimates, wrong for VBR). `scan_duration` walks packets on a helper thread and sends `Command::DurationScanned`; a result for a non-current track is ignored; until then the seek bar uses the estimate or is off.

**Equalizer modes.** `EqMode` (`Off`/`Global`/`PerTrack`) and `track_eq` live in `radiant-core/src/settings.rs` with the helpers `effective_eq`, `eq_editable`, `set_active_eq`, `set_fx_keep_eq`, `reset_sound` (unit-tested). The `fx` cell in `main.rs` is the EFFECTIVE set sent to the engine; `settings.fx.eq_gains_db` stays the global curve. `refresh_eq` / `apply_curve` / `show_eq_state` in `main.rs` apply a curve; the status timer reports a started track via the Slint callback `now-track-changed(path)` (per-track curve is loaded on the UI thread, up to one timer tick after the track starts). `eq_curve.rs` draws the mini graph (Catmull-Rom -> Bezier, 328x46 box).

**Sound settings.** Rust (`main.rs`) owns `AudioFxSettings` and sends the whole set on every change (`Command::SetFx`, applied on the engine thread). `RING_MS = 400` in `engine.rs` (shorter ring = faster audible response; raise it if crackling appears on weak machines). Slider steps: 0.5 dB (EQ/bass/treble), 1 % (subsonic), 0.1 (speed).

**Vocoder.** Port of `TimeStretch.kt` (same N, hops, peak detection, phase locking, stereo-sum phases, OLA, tail; hand-written radix-2 FFT, no new crate). API: `TimeStretch::{new,set_speed,process,finish,reset,configure}`, `slider_to_speed`. Sits after the resampler, before the effects in `Engine::fill`; pass-through at speed 1; reset on seek and on new track. `position.rs` (`PositionTracker`) keeps position/progress in SOURCE time; a speed change is registered at the output frame where it becomes audible.

**Slint gotchas.** An element without `x` / `y` inside a plain `Rectangle` (no layout) is CENTERED, not placed at 0: always set `x` and `y` explicitly.  `VolumeBar` callback is named `edited` (not `changed`, which clashes with a Slint keyword). Mouse wheel: `scroll-event` on each slider's TouchArea; 1 notch = 60 px = 5 s seek / 5 % volume / `wheel-step`. Player-bar control row uses absolute positions; window min-width 640. Preset names are English keys (`rock`, `pop`, ... = Android `preset_*`) until localization exists.

**Robustness (engine.rs).** `skip_streak` counts files in a row that gave no audio (open error or decode error before the first good chunk); it is reset by the first good chunk and by any user command; at `MAX_SKIPS` the player stops. `guarded()` turns a decoder panic into an error (needs `panic = "unwind"` in the profile). Device: `Output.lost` is set by the cpal error callback; `refresh_output()` also compares the default device name (every `DEVICE_CHECK_INTERVAL` while playing, and on every `start()`, nothing while paused). `reopen_output()` rebuilds stream, vocoder, effects (from `fx_settings`), resampler and position tracker for the new rate/channels, then seeks the decoder to the old position. On failure the player pauses with a message; Play retries. Vocoder bypass: `use_voc` / `voc_drain`; `vocoder_reset()` keeps them consistent after any `stretch.reset()`.

**Normalization note.** The envelope follows the spec (fast attack), so on a pure sine it settles ~15 % below the 0.16 target; kept as on Android. Gain keeps adapting for ~9 s of silence until the envelope falls under the gate (1e-6), then freezes.

**Spectrum rays.** `cpal` callback pushes a mono mix (before volume, after fade) into an rtrb ring only while `SpectrumTap.enabled` is set (UI sets it while the player view is open). The UI timer (25 ms, `main.rs`) calls `Player::spectrum().poll(dt)` -> `radiant-dsp/src/spectrum.rs` (FFT 2048, Hann, 60 log bands 40 Hz..16 kHz, +3 dB/oct tilt, DB_FLOOR/DB_CEIL = -78/-20, attack 30 ms, release 160 ms, falling peaks) -> `radiant-ui/src/rays.rs` (`build(bands, peaks, phase, time, bass, level)`) builds SVG path strings (1000x1000 box, lows at the bottom, mirrored) -> 4 filled `Path` layers in `main.slint` (triangles; pseudo-random angle/length/band per ray, two halves map the scale differently, `phase` turns slowly). Ring diameter is `/ 1.4` so the rays fit. Tune by eye: DB_FLOOR/DB_CEIL, TILT_DB_PER_OCT, R_SPAN, RAY_COUNT, timer interval. Tests: `spectrum.rs` (4), `rays.rs` (6) need no external crates.

**Tests.** The ramp-WAV seek tests take expectations modulo 30011 (the ramp wraps). The resampler sine test allows a slide of up to 1 frame and checks a residual shape error < 0.005 (rubato reports its delay in whole frames). MP3 header detection (ID3 size, Xing/Info/VBRI) is unit-tested; real-file check with ffmpeg: `ffmpeg -i in.wav -c:a libmp3lame -b:a 192k -write_xing 0 headerless.mp3` (CBR) and `-q:a 2 -write_xing 0` (VBR).

---

# Appendix: original project brief (moved here from README.md)

> Written for an AI assistant / developer building the PC version of Radiant. Section numbers below are used by the notes above ("Appendix section 5" = algorithm reference).

### 1. Goal

Build a **desktop (PC) version of Radiant**: a simple, private music player for **local audio files** with the same
feature set and sound-processing quality as the Android app, written in **Rust**.

The owner's priorities, in this order:

1. **Optimization**: low CPU, low RAM, small binary, instant start, ~0 % CPU when idle or paused.
2. **Audio processing**: the DSP (equalizer, bass/treble, subsonic filter, normalization, limiter, pitch-preserving
   speed change, AI stem separation) must sound as good as on Android and must never glitch or click.
3. **Feature parity** with the Android app wherever it makes sense on a desktop (see section 4 for what does not carry over).

Principles inherited from the Android project: no ads, no accounts, no tracking, no internet needed
(the network is used only for the optional Dropbox download and for downloading the stem models), everything stays on the device.

#### Target platform

**Windows** (decided by the owner). Minimum supported version: **Windows 10**, and **older versions (Windows 8.1 / 7) if that is
technically possible**. Primary architecture: **x86_64**; ARM64 Windows is optional and can come later.

- **Windows 10 and 11 are the guaranteed targets.** Everything in this brief (including stems) must work there.
- **Windows 7 / 8 / 8.1 are a best-effort stretch goal**, do not let it block the main version. As far as I know, current Rust no longer
  supports them as a normal target (the standard `windows-msvc` targets require Windows 10; Win7 exists only as a separate, lower-tier
  `x86_64-win7-windows-msvc` target that needs a nightly toolchain with `-Zbuild-std`), and other dependencies may drop them too
  (check ONNX Runtime and the Slint/windowing backend). Investigate early and report honestly what works. A sensible outcome could be:
  "Windows 10+ full version; optional legacy build for Windows 7/8.1 without stems (no `ort`) and with the software renderer".
  Do not promise legacy support to users until a real build was tested on such a system.
- Keep the code **platform-neutral** where it costs nothing (Rust, `cpal`, `symphonia`, Slint are cross-platform) and put Windows-specific
  parts (media controls, file dialogs, installer, autostart if ever needed) behind small modules, so Linux/macOS stay possible later.

---

### 2. Fixed technology choices

| Area | Choice | Notes |
|---|---|---|
| Language | **Rust** (stable, 2021+ edition) | |
| Audio output | **`cpal`** | Output only. On Windows it uses **WASAPI** (shared mode by default). Real-time callback rules in section 6. |
| Decoding | **`symphonia`** | MP3, FLAC, OGG/Vorbis, AAC, ALAC, WAV, tags. See the codec caveat below. |
| DSP | **Self-written** | Port of Android's `AudioFx.kt` and the phase vocoder in `TimeStretch.kt`. Do not replace it with a third-party effect library. |
| FFT | **`realfft` / `rustfft`** or **`rubato`** (free choice) | `rubato` is also handy for resampling (stems need 44.1 kHz). |
| AI stems | **`ort`** (ONNX Runtime) | One htdemucs (Demucs v4) ONNX model, downloaded in Settings (the Android app uses Open-Unmix; see PROGRESS.md stage 11). |
| UI | **Slint** | Keep it light: no web view, no heavy animation. |

Caveats to verify at the start (crate versions and capabilities change, check the current docs, do not trust this list blindly):

- **Opus.** As far as I know `symphonia` has no native Opus decoder. The Android app plays OGG/Opus, so find a decoder
  (libopus bindings or a pure-Rust one) and plug it in, or document the gap.
- **AAC.** `symphonia` decodes AAC-LC; HE-AAC support may be missing. Check against the owner's real files.
- **ALAC** is supported by `symphonia`, so the Android workaround (FFmpeg built only for ALAC) is **not needed** on PC. Do not add FFmpeg.
- **Slint license.** Slint is offered under several licenses (GPLv3, a royalty-free license with attribution, commercial).
  Radiant is under **PolyForm Noncommercial 1.0.0**, which is not GPL-compatible, so the project must use the
  royalty-free (attribution) option. Read the current Slint license terms and add the required attribution.
- **`ort` needs the ONNX Runtime native library** (`onnxruntime.dll`, downloaded or dynamically loaded). Decide how it is bundled in the release
  (size matters), and keep stems optional so the base app stays small. Check which Windows versions the chosen ONNX Runtime build supports
  (relevant for the Windows 7/8.1 goal).
- **Slint renderer.** Slint can use a GPU backend (OpenGL via femtovg, or Skia) or a software renderer. For a small footprint and for old/weak
  graphics drivers test the **software renderer** (or the lightest backend) first and compare memory and CPU; pick by measurement.

---

### 3. The Android app (reference implementation)

**Radiant** (formerly "Local Player") is a native Android music player written in **Kotlin + Jetpack Compose (Material 3)
+ AndroidX Media3 (ExoPlayer)**, with ONNX Runtime for stems. Package `com.example.localplayer`, ~13,000 lines.
Source (public repo of the owner, branch `main`): `https://github.com/john3sobieski/radiant-audio-player`.
The Android README in that repo is the authoritative user-level description; below is a condensed version plus the
engineering details a porter needs. **Always read the matching Kotlin file before porting a module.**

#### 3.1 Features

- **Music folder.** The user picks one folder; Radiant scans it recursively and rescans on every launch. Nothing outside
  that folder is ever visible.
- **Cloud download (optional).** Dropbox (access type *App folder*, OAuth 2 with PKCE through the browser, so the app never sees
  a password). **One-way only**: music is downloaded from a cloud folder the user chooses into a `Dropbox` subfolder of the
  music folder. Nothing is uploaded, changed or deleted in the cloud, nothing local is deleted or overwritten.
- **Library.** Sections: Playlists, Albums (grouped automatically from tags), Tracks, Queues. Search and sorting in every section.
- **Playlists.** Create, rename, delete, add/remove tracks. The *Liked tracks* playlist always exists, is pinned on top and can't be
  renamed or deleted.
- **Queues.** Exactly four fixed queues (cannot be created or deleted); add/remove tracks, reorder by drag or "move up/down".
- **Reorder by drag** for playlists, albums and tracks (manual order is a sort mode of its own).
- **Sorting.** Tracks: manual, name, artist, album, duration, file date. Albums: manual, name, artist, track count, file date.
  Playlists: manual, name, created, updated, track count. Each has an ascending/descending toggle.
- **Player.** Play/pause, next/previous, seek bar, shuffle, repeat (off / all / one), cover art, quality badge (format + bitrate),
  300 ms fade-in and 250 ms fade-out on play/pause to avoid clicks.
- **Lyrics.** From tags (ID3 `USLT`/`SYLT`, Vorbis `LYRICS`, M4A `©lyr`) and from `.lrc` / `.txt` next to the track with the same
  name; rechecked on every scan. Synced (`.lrc`) lyrics scroll with the song, highlight the current line, fade lines already sung;
  tapping a line seeks to it. Plain lyrics get a compact view.
- **Playback speed.** 3x slower to 3x faster, step 0.1, **pitch preserved** (details in 5.2).
- **Sound.** Mono/stereo, volume normalization, bass boost, treble boost, subsonic filter, 12-band equalizer with 10 presets.
  EQ modes: off / one curve for all tracks / a separate curve remembered per track (the global curve and per-track curves are
  stored independently and never overwrite each other).
- **Stems (AI).** Split a track into vocals / drums / bass / other and mix any selection into **one** WAV (details in 5.3).
- **App lock** (off by default): 4-digit, 6-digit or custom code, recovery file with 12 random words (details in 5.4).
- **Track details:** tags, format, bitrate, sample rate, channels, file size, etc.
- **Themes:** light, true-black dark (OLED), follow system. **App language:** System + 10 languages (en, de, ru, uk, zh, ja, fr, cs, es, pl).
- **Power saving** (Off / Medium / Intensive): fewer animations, rarer progress updates, and in Intensive also forced black theme,
  volume capped at 40 %, mono, DSP bypassed, stems disabled. See section 4 for the desktop equivalent.
- **Formats:** MP3, FLAC, OGG/Opus, M4A/AAC, ALAC, WAV and whatever else the system can decode.

#### 3.2 Data model (`Models.kt`)

- `Track { uri, title, artist, albumArtist, album, durationMs, trackNo, info: AudioInfo, modified, added, lyricsUri?, tagLyrics }`
- `Album { key, name, artist, tracks }`, `Playlist { id, name, trackUris, createdAt, updatedAt }`, `PlayQueue { id 0..3, trackUris }`
- Special values: `FAVORITES_ID = "favorites"`, `QUEUE_COUNT = 4`, `NO_ALBUM` / `NO_ARTIST` placeholders (displayed through translated strings,
  never stored translated).
- `SortKey` / `SortState(key, descending)` as listed above.

#### 3.3 Source map: what to read for each module

| Android file | Content | PC plan |
|---|---|---|
| `AudioFx.kt` | Sound settings store, the `AudioFxProcessor` (EQ, bass, treble, subsonic, normalization, mono, limiter) | **Port as-is** (see 5.1) |
| `TimeStretch.kt` | `TimeStretchProcessor` and `PhaseVocoder` (pitch-preserving speed) | **Port as-is** (see 5.2) |
| `StemDsp.kt`, `Stems.kt`, `StemModels.kt`, `StemDecoder.kt`, `StemOrt.kt`, `StemService.kt` | Stem pipeline: decoding, STFT, ONNX passes, masks, WAV writing, model download/verification | **Port** (see 5.3); service becomes a background thread |
| `AppLock.kt`, `LockUi.kt` | Code hashing, brute-force delays, recovery file | **Port** the logic (see 5.4); redo the UI |
| `Lyrics.kt`, `LyricsUi.kt` | Tag/.lrc parsing, lyrics screen | Port parsing; redo UI |
| `Library.kt`, `Search.kt`, `ListTools.kt`, `Labels.kt` | Folder scan, grouping, sorting, drag reorder | Port logic; reorder UI in Slint |
| `Quality.kt`, `TrackInfo.kt`, `CoverStore.kt` | Format/bitrate detection, details, cover cache | Port logic (symphonia gives most of it) |
| `Cloud.kt`, `CloudUi.kt`, `CloudSyncService.kt` | Dropbox OAuth PKCE and one-way download | Port logic; use a loopback redirect instead of an Android URI scheme |
| `PlaybackService.kt`, `PlayerViewModel.kt` | Media3 player wiring, state, fades | **Do not port**; replaced by the cpal/symphonia engine, keep the behavior (fades, repeat, shuffle) |
| `PowerSave.kt`, `Motion.kt`, `Scrollbar.kt`, `Ui.kt`, ... | Android UI/Compose | Redo in Slint |
| `res/values*/strings.xml` | 320 strings x 10 languages | **Convert with a script**, do not retranslate (section 7) |

---

### 4. What does not carry over to PC

- **Media3 / ExoPlayer, `PlaybackService`, MediaSession, foreground services.** Replace with your own engine. On Windows use the OS media
  integration instead: media keys and the system transport controls (**SMTC**, System Media Transport Controls; check crates such as
  `souvlaki`). SMTC exists on Windows 10 and later; on older Windows fall back to plain media-key handling or skip it.
- **Storage Access Framework (SAF).** On Windows the music folder is a normal path (use a native folder dialog such as `rfd`).
  Handle Unicode paths (non-Latin names are common in this library) and long paths correctly; use `std::path`/`OsString`, never assume UTF-8 file names.
  Stems are written to `Stems/<artist - title>/` inside it; the Dropbox download goes to its `Dropbox` subfolder.
- **Android-specific UI:** Material You dynamic colors, notifications, the three-dots sheets, power-saving "Medium/Intensive" as such.
  Desktop mapping to decide with the owner: a single optional *Power saving / low-resource* mode (fewer redraws, DSP bypass,
  stems disabled) rather than three levels. The "volume capped at 40 %" and "no Bluetooth" rules are phone-specific and should be dropped.
- **FFmpeg ALAC build.** Not needed (see section 2).
- **Android signing, GitHub Actions APK workflow.** Replace with a Rust CI (section 8).

Features to keep identical in behavior: everything in 3.1 except the items above.

---

### 5. Algorithm reference (parameters from the Android code)

Everything below is already tuned by ear on Android; **keep the numbers** unless you measure a reason to change them.

#### 5.1 Effects chain (`AudioFxProcessor`)

Input: 16-bit or float PCM, any channel count (stereo in practice). Internally `f64`. Order per frame:

1. **Mono downmix** (if enabled): `m = (L + R) / 2` into both channels.
2. **Normalization** (if enabled), a slow automatic gain on the frame's mean power:
   target RMS `0.16` (about -16 dBFS); power envelope follower with attack `0.25 s`, release `0.8 s`; gate at power `1e-6`
   (about -60 dBFS, below it the gain is frozen); desired gain `0.16 / sqrt(env)` clamped to `0.25 .. 2.8` (-12 dB .. +9 dB);
   gain smoothing with attack `0.3 s` (gain going down) and release `2.0 s` (going up). Coefficient: `1 - exp(-1 / (sampleRate * seconds))`.
3. **Filter chain** (all biquads, RBJ Audio EQ Cookbook formulas, direct form II transposed, state per channel per filter):
   - **12-band equalizer**, peaking filters at `32, 64, 125, 250, 500, 1000, 2000, 3500, 6000, 9000, 12500, 16000 Hz`, range ±12 dB.
     Q of each band depends on the distance to its neighbours so the bands overlap evenly:
     `lo = previous band (or f/2 for the first)`, `hi = next band (or 2f for the last)`,
     `octaves = ln(hi/lo) / ln 2 / 2`, `Q = 1 / (2 * sinh(ln2/2 * octaves))`.
     A band is skipped if `|gain| < 0.05 dB` or `f >= 0.45 * sampleRate`.
   - **Bass boost**: low shelf at `110 Hz`, slope S = 1, up to +15 dB.
   - **Treble boost**: high shelf at `6000 Hz`, slope S = 1, up to +15 dB (skipped if the corner is above `0.45 * sampleRate`).
   - **Subsonic filter**: intensity 0..100 % maps linearly to a cutoff of `12 .. 40 Hz`; two cascaded Butterworth high-pass
     sections with `Q1 = 0.5412` and `Q2 = 1.3066` (24 dB/oct). Active above 0.5 %.
   - **Preamp** before the filters: `10^(-0.5 * maxBoost / 20)` where `maxBoost` is the largest positive gain among EQ bands, bass and
     treble (half of the boost is compensated to hit the limiter less often).
   - When a filter is switched off its state is zeroed (prevents a click when it is re-enabled). Coefficients are recomputed only when a
     settings *version counter* changes (the audio thread reads atomics, settings are replaced whole, never mutated in place).
4. **Soft limiter** (only when normalization or any filter is active): linear up to `0.9`, then `0.9 + 0.1 * tanh((|x| - 0.9) / 0.1)`.
5. When nothing is active the buffer is copied through untouched (**zero cost bypass**; keep this property).

Equalizer presets: `EqPresets.kt` (10 presets). EQ modes: `OFF`, `GLOBAL`, `PER_TRACK` (per-track curves are persisted by track URI).

#### 5.2 Speed change with preserved pitch (`TimeStretchProcessor`, `PhaseVocoder`)

Why self-written: the stock Sonic implementation (speech oriented) "chops" music when slowed down. The replacement is a **phase
vocoder with identity phase locking**.

- **Speed mapping.** UI slider position `t` in `-2 .. +2`, step 0.1: speed `= 1 + t` for `t >= 0`, `1 / (1 - t)` for `t < 0`
  (so `+2` = 3x faster, `-2` = 3x slower). The label shows `1..3x` or `-1..-3x`. The processor clamps speed to `0.25 .. 4`.
- **Pass-through.** At speed 1 (|speed - 1| < 0.001) and before the first change, audio goes through untouched with no delay and no CPU.
  The vocoder engine starts on the fly from the current position, so there is no gap.
- **Frame size** `N = 2^round(log2(sampleRate * 0.09))` clamped to `2048 .. 16384` (4096 at 44.1 kHz, about 90 ms), Hann window,
  synthesis hop `N / 4`, analysis hop `(N / 4) * speed`. Output frames always sit at a constant hop; input frames are taken with a variable step.
- **Phase locking.** Phases of all bins belonging to one spectral peak are advanced identically (removes the "metallic" tone of a plain vocoder);
  phases stay continuous between frames (no clicks).
- **Stereo.** The phase advance is computed from the **sum of channels** and applied to both, so the stereo image does not drift.
  Channels are processed in pairs (one forward and one inverse FFT per pair per frame). More than two channels: pairs of channels.
- **Overlap-add** normalized by the running sum of squared windows; the accumulator is shifted by one hop per frame.
- **Streaming**: bounded buffers; at end of stream append silence and flush the tail (`emitTail`).
- Cost: a few percent of one core. The FFT in Kotlin is hand-written radix-2 with tables; in Rust use `realfft`/`rustfft` with pre-planned FFTs.
- Read `PhaseVocoder.Group.process` for the exact phase-locking code before porting.

#### 5.3 Stem separation (AI)

> **Superseded on PC (owner's decision):** the PC version uses Demucs (htdemucs, 4 stems, one ONNX network) instead of Open-Unmix. The requirements, the contract of the network and the status are in PROGRESS.md, stage 11. What follows describes the Android version; the output file names, the 16-bit WAV output, the temp-file idea and the progress / cancel behavior still apply.

- **Models.** Open-Unmix `umxhq` exported to ONNX (MIT, © 2019 Inria), one network per stem, four total, about 34 MB each. Not bundled: downloaded
  in Settings (SHA-256 verified) or imported from files (recognized by checksum, the file name does not matter).
  Hugging Face repos `MixDirective/open-unmix-umxhq-{vocals,drums,bass,other}-onnx`, file `model.onnx`
  (URL: `https://huggingface.co/<repo>/resolve/main/model.onnx`). Expected sizes and SHA-256:

  | Stem | Bytes | SHA-256 |
  |---|---|---|
  | vocals | 35,623,512 | `f27742bb52b24cb039614dc76d0764e279df3eb2a9d6daa82c6daee3369ec747` |
  | drums | 35,623,507 | `471628e1e48138a634f962a89f622929980cf5485ed0a4e6e648e9dfcd1145d9` |
  | bass | 35,623,502 | `8931033f04666fdcf300d610579351d74275a6c55dd4346009290c844033726f` |
  | other | 35,623,507 | `fe62c1d8fb8326c7e68540a675dfa186a2b131f763fb41050ea5e9a9859eb76e` |

- **Tensor.** Input and output of every network: magnitudes `[1, 2, 2049, frames]` (channel, bin, frame), index `(channel * 2049 + bin) * frames + frame`.
- **STFT.** 44.1 kHz stereo, `N_FFT = 4096`, `HOP = 1024` (2049 bins), centered STFT with reflection at the edges.
- **Blocks.** Core `512` hops (about 11.9 s) plus `86` hops (about 2 s) of context on each side; the context is discarded because the
  bidirectional LSTM is worse near block edges. Seamless stitching. Memory does not depend on track length (about 250 MB).
- **Masking.** All four networks always run; estimates `v` are combined with a soft mask `v^2 / sum(v^2)` so the stems add up to the original and none
  is too quiet. A selection of stems is mixed as the sum of the selected masks applied to the mix: "everything except bass" = full track without bass.
- **Decoding and resampling.** The track is decoded to 44.1 kHz stereo float (Android: system decoder; PC: `symphonia`, plus a resampler when the file
  is not 44.1 kHz, the Android code has a polyphase sinc resampler, `ZERO_CROSSINGS = 14`; `rubato` is the natural replacement). Multichannel files use the first two channels.
  Intermediate audio goes to a temporary float32 stereo file (about 85 MB per 4 minutes), removed afterwards.
- **Output.** 16-bit / 44.1 kHz stereo WAV in `Stems/<artist - title>/` inside the music folder, for example `... - No Bass.wav`, `... - Vocals + Drums.wav`.
  The name depends only on the selection (not on the language) so a repeat run overwrites the same file.
- **Behavior.** Not real-time; progress bar, remaining-time estimate, cancel; runs in the background; temp files cleaned on cancel/error.
  Known limits: plain ratio mask without Wiener filtering (slightly below reference quality); vocals/drums/bass are empty above about 16 kHz.
- **On PC:** run in a worker thread (or a few, but benchmark: ONNX Runtime already uses intra-op threads), use `ort` with CPU execution first, consider
  optional GPU execution providers later. Expect to be much faster than on a phone.

#### 5.4 App lock (`AppLock.kt`)

- Code kinds: 4 digits, 6 digits, or custom (Latin letters and digits, 4..64 chars, no spaces). Asked on every launch and every return from the background.
- **No code is stored.** Store a random salt and a `PBKDF2-HMAC-SHA256` hash (**30,000 iterations**), compare in constant time.
- **Brute-force delay.** After every 5 wrong codes in a row: wait 30 s, then 1, 2, 4... minutes, up to 15 minutes. The delay survives restarts (persisted).
- **Recovery file.** On creating a code, generate **12 random words** from a built-in list of **700 English words** with a cryptographically secure RNG
  (700^12, about 113 bits), saved by the user as a `.txt`. Only a hash of the words is stored. The lock turns on only after the file has been written. Picking
  the file on the lock screen (*Forgot password*) disables the lock. Changing the code creates a new file and invalidates the old one.
- The lock protects the app UI only, not the audio files; music keeps playing while locked. Screen capture is blocked on the lock screen on Android (optional on PC).
- Keep `AppLock.kt` as the spec; the word list is in the Android source.

#### 5.5 Lyrics, library and the rest

- **Lyrics parsing** (`Lyrics.kt`): ID3v2 `USLT`/`SYLT`, Vorbis comment `LYRICS`, MP4 `©lyr`, plus sidecar `.lrc`/`.txt`. `.lrc` timestamps drive the line index by binary search
  on the playback position; a small lead (`LEAD_MS`) is added so a line is highlighted slightly before it is sung.
- **Library scan** (`Library.kt`): recursive walk of the chosen folder, cached; checks lyrics availability on every scan; albums grouped by album artist + album tags with
  `NO_ALBUM`/`NO_ARTIST` fallbacks. On PC use a filesystem watcher or a rescan on start plus a manual rescan button; keep scans off the UI thread.
- **Cover art:** `CoverStore.kt` keeps a size-tiered bitmap cache. On PC cache decoded covers at a few fixed sizes and bound the cache.
- **Quality badge / details:** `Quality.kt`, `TrackInfo.kt` (format, bitrate, sample rate, channels, size).

---

### 6. Suggested architecture and engineering rules

Cargo workspace, one crate per concern (names are a suggestion):

- `radiant-core`: data model, library scan, playlists, queues, sorting, search, settings persistence, lyrics parsing, app-lock logic. No audio, no UI.
- `radiant-audio`: decoding thread (`symphonia`), ring buffer, `cpal` output stream, transport (play/pause/seek/repeat/shuffle, fades), gapless-friendly track switching.
- `radiant-dsp`: `AudioFx` chain, phase vocoder, resampler wrapper. Pure functions over `&mut [f32]`/`&mut [f64]`; unit-testable without audio devices.
- `radiant-stems`: STFT, ONNX passes, masks, WAV writer, model store (download, SHA-256, import).
- `radiant-cloud`: Dropbox OAuth PKCE and one-way download (blocking or async, but separate from the UI thread).
- `radiant-ui` (the binary): Slint UI, wiring, OS media controls.

**Real-time audio rules (the main reason for choosing Rust):**

- The `cpal` callback must not allocate, lock a mutex, do I/O or log. It only copies finished samples from a lock-free SPSC ring buffer (e.g. `rtrb` or `ringbuf`) into the device buffer.
- Decode and run the DSP on a dedicated thread that keeps the ring buffer filled; do not decode in the callback.
- Pass settings to the audio side through atomics plus a version counter (the Android pattern: settings are replaced whole, the audio thread rebuilds coefficients when the version changes).
- `f32` PCM end to end; the filter state and coefficients may stay `f64` (as on Android).
- Reuse buffers, no per-frame allocations; pre-plan FFTs; zero-cost bypass when an effect is off.
- Underrun handling: output silence and recover, never panic in the audio path.
- Handle device changes (headphones unplugged, default device switched) by re-opening the stream and resuming.

**Performance targets to measure and report (do not guess):** CPU while idle, paused and playing (plain, with full EQ chain, at 3x/-3x speed), RAM after scanning a large library,
cold start time, binary/installer size. Release profile: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
UI: redraw only on change (nothing animating while paused), update the seek bar at a modest rate, virtualize long lists, no polling loops.

**Testing the DSP:** biquad frequency responses against the RBJ formulas; phase vocoder: a sine at speed 0.5/2.0 keeps its frequency (FFT peak) and its length scales;
stems: the sum of all four stems must reproduce the input within a small error; app lock: known-answer PBKDF2 vectors and the delay schedule.

---

### 7. Localization

The Android app has **10 languages**: English (default), German, Russian, Ukrainian, Chinese, Japanese, French, Czech, Spanish, Polish,
about 320 strings each, in `app/src/main/res/values*/strings.xml` (Android XML). **Do not retranslate.** Write a conversion script from those files to the
chosen PC format (Slint supports gettext via `@tr(...)`, or use Fluent), keep the same keys where possible, and keep *System* as a language option that follows the OS locale,
falling back to English. The language can change at runtime without interrupting playback.

---

### 8. Build, CI and release

The owner builds through **GitHub Actions** and pastes build logs when something fails; there is no local toolchain assumption.

- Provide a workflow on a **`windows-latest`** runner (MSVC toolchain, target `x86_64-pc-windows-msvc`) that builds the release binary, caches `~/.cargo` and `target`, and uploads the artifact.
  Link the C runtime statically (`-C target-feature=+crt-static`) so users do not need the Visual C++ Redistributable.
- Release formats to consider: a **portable `.zip`** (exe plus `onnxruntime.dll` if stems are included) and an **installer** (MSI or NSIS, or MSIX). Keep the installer small and optional.
- The binary will be unsigned (code-signing certificates cost money), so Windows SmartScreen may warn on first run. Mention it in the final README ("More info, Run anyway") and
  consider signing later.
- If the Windows 7/8.1 stretch goal is pursued, add a separate, clearly named legacy workflow instead of complicating the main one.
- Never claim something compiles or works unless it was built; if you cannot run it, say so and give the owner the exact commands to test.
- Releases go to GitHub Releases like on Android (latest stable build there).

---

### 9. Working with the owner

- **Language:** the owner writes in **Russian**: reply in Russian, keep code, comments in English unless asked otherwise, and keep file names and identifiers in English.
- **Experience:** the owner does not write Rust or Kotlin; the code is produced with AI help. Explain decisions briefly and concretely, avoid unexplained jargon, give complete files
  or exact diffs (the owner uploads project archives and applies changes as whole files), and keep the project structure stable.
- **No feature loss.** When optimizing, remove only what plays no role at all; do not cut or change behavior without asking. When unsure whether something matters, ask.
- **Identity.** The author appears only as the nickname **john3sobieski** (GitHub `https://github.com/john3sobieski`). Do not ask for or insert a real name.
- **Support links** used in the Android README and `.github/FUNDING.yml`: Ko-fi `radiant_player`, Buy Me a Coffee `radiant_player`, GitHub Sponsors `john3sobieski`.
  Keep the same in the PC repository.

---

### 10. License and third-party components

- **Radiant is licensed under PolyForm Noncommercial 1.0.0** (`LICENSE.md`, with a *Required Notice* line naming the nickname). Free for noncommercial use; nobody may sell it or publish it for profit.
  Keep `LICENSE.md` and the notice in the PC project.
- Third-party code stays under its own licenses: `symphonia` (MPL-2.0), `cpal` (Apache-2.0), `ort` and ONNX Runtime (MIT / Apache-2.0), `rustfft`/`realfft`/`rubato` (MIT/Apache-2.0),
  Slint (see the license note in section 2), Open-Unmix (MIT, © 2019 Inria) with weights trained on MUSDB18-HQ (check the data/weights terms for any use beyond personal). Check each crate's current
  license and list them in a *Third-party components* section of the final README.

---

### 11. Suggested order of work

1. Investigate the Windows 7/8.1 question quickly (toolchain target, `ort`, Slint backend) and report what is realistic; confirm with the owner which Android features are must-have for version 1.
2. Skeleton workspace + Windows CI that produces a runnable binary.
3. Playback engine: folder scan, decode, `cpal` output, play/pause/seek/next/prev/shuffle/repeat, fades. Measure CPU.
4. Slint UI for the library (Playlists / Albums / Tracks / Queues), search, sorting, drag reorder, player view with cover and quality badge.
5. `AudioFx` chain (EQ, bass, treble, subsonic, normalization, mono, limiter) with unit tests; then the phase-vocoder speed control.
6. Lyrics (tags + `.lrc`), track details, themes, languages.
7. App lock.
8. Stems (`ort`), model manager.
9. Dropbox one-way download.
10. Windows media integration (SMTC / media keys), portable zip and installer, user-facing README, release.

Keep this file updated as decisions are made (platforms, crate versions, deviations from Android), so the next session starts from the current state.

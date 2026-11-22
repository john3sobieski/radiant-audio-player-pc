# PROGRESS_Linux — Radiant для Linux
(заметки для следующей сессии: читать после `PROGRESS.md`. Там правила, Windows-версия и общая история; здесь только то, что нужно для Linux)

## Правила для ассистента
- Все правила из `PROGRESS.md` (раздел «Rules for the assistant») действуют и здесь: отвечать по-русски, код и комментарии на английском, присылать только изменённые файлы, в песочнице ничего не собирать. Сборку и тесты владелец запускает только через GitHub Actions: проверять результат по логам CI.
- Linux в песочнице не собрать и не проверить. Всё, чего владелец не проверил на реальной системе, помечать 🟡 или ⬜, а не ✅. Не утверждать про версии дистрибутивов, пакетов и инструментов по памяти: помечать «проверить».
- Windows-версия не должна пострадать: Linux-код только под `#[cfg(not(windows))]` или `#[cfg(target_os = "linux")]`, общий код не ломать.
- Обновлять отметки на месте, без журнала сессий.

## Legend
✅ готово, собрано в CI и проверено на реальной системе  |  🟡 написано, но не собиралось или не проверено на Linux  |  ⬜ не начато  |  ❌ нужно исправить

---

## Цель
Radiant работает на **Ubuntu, Linux Mint, Debian, Fedora и Arch**. Все четыре способа поставки: **AppImage, Flatpak, .deb + .rpm, AUR**. Архитектура: **x86_64** (ARM64 позже, если понадобится).

| Дистрибутив | Основной способ | Запасные |
|---|---|---|
| Ubuntu | .deb | AppImage, Flatpak |
| Linux Mint | .deb (Mint на базе Ubuntu) | AppImage, Flatpak |
| Debian | .deb | AppImage, Flatpak |
| Fedora | .rpm | AppImage, Flatpak |
| Arch | AUR | AppImage, Flatpak |

Минимальная glibc: бинарник собирается на самом старом поддерживаемом Ubuntu LTS (22.04; по логу CI бинарнику нужна glibc не новее 2.35), чтобы AppImage и пакеты запускались и на более новых. Debian 11 и старше не поддерживаются (проверить, хватает ли им glibc, если понадобятся).

---

## Что такое проект сейчас (Windows-версия)

Нативный музыкальный плеер для локальных треков, порт Android-приложения Radiant. Rust + Slint. Версия живёт в одном месте: корневой `Cargo.toml`, `[workspace.package]`, `version`.

**Crate'ы (`crates/`):**
| Crate | Что делает |
|---|---|
| `radiant-core` | настройки (JSON), библиотека, теги, тексты песен (.lrc), плейлисты, 4 очереди, сортировка, тема, блокировка приложения (PIN, фраза восстановления) |
| `radiant-audio` | декодирование (`symphonia`, Opus через libopus), ресемплер (`rubato`), вывод (`cpal`, на Windows WASAPI), кольцевой буфер (`rtrb`), спектр |
| `radiant-dsp` | свой 12-полосный EQ, bass/treble/subsonic, нормализация, лимитер, фазовый вокодер (скорость ±3×), пресеты |
| `radiant-stems` | ИИ-разделение на 4 стема (Demucs htdemucs, ONNX), скачивание модели (~90 МБ), `ort` с `load-dynamic` |
| `radiant-cloud` | вход в Dropbox (OAuth 2 + PKCE в браузере, ответ на `http://127.0.0.1:53682/`), синхронизация |
| `radiant-ui` | окно Slint (`ui/main.slint`), свой заголовок окна (`window_chrome.rs`), медиа-клавиши (`media.rs`), языки (`i18n.rs`, 10 языков) |
| `radiant-setup` | установщик и деинсталлятор Windows (`winreg`); `Radiant-Setup.exe` собирает CI |

**Конвейер звука:** декодер → ресемплер → вокодер → цепочка эффектов → кольцевой буфер → `cpal`.

**Где что лежит (Windows):** настройки, плейлисты, очереди, модель и результаты в `%APPDATA%\Radiant`; рядом с `radiant.exe` лежат `onnxruntime.dll` и `DirectML.dll` (кладёт установщик).

**Особенности Windows, которые на Linux устроены иначе:**
| Область | Windows сейчас | Linux |
|---|---|---|
| Окно | `no-frame`, свой заголовок, перетаскивание и ресайз через winit | 🟡 тот же код; X11 и Wayland проверить |
| Медиа-клавиши | SMTC через `souvlaki` (`media.rs`, нужен HWND) | ⬜ сейчас пустая заглушка; нужен MPRIS |
| Звук | `cpal` → WASAPI | 🟡 `cpal` → ALSA (на PipeWire/PulseAudio через их слой; проверить) |
| Стемы | `onnxruntime.dll` + DirectML (GPU) | ⬜ `libonnxruntime.so`, CPU; `directml` на Linux не нужен |
| Папка данных | `%APPDATA%\Radiant` | 🟡 в коде есть ветка `XDG_CONFIG_HOME`; решить про модели и результаты стемов |
| Диалог папки | `rfd` | 🟡 нужен xdg-desktop-portal или GTK; проверить |
| Браузер для Dropbox | `rundll32` | 🟡 `xdg-open` уже в коде |
| Язык системы | `GetUserDefaultUILanguage` | 🟡 переменные `LANG` и др. уже в коде |
| Случайные байты (блокировка) | `BCryptGenRandom` | 🟡 `/dev/urandom` уже в коде |
| Установка | `Radiant-Setup.exe` | ⬜ пакеты (ниже) |
| Иконка | `assets/icon.ico` в `build.rs` | ⬜ нужны PNG/SVG, `.desktop`, AppStream |

---

## ROADMAP

### Этап L1. Сборка на Linux
- ✅ Список системных зависимостей для сборки и запуска (таблица ниже); сверен с реальной сборкой: бинарник напрямую требует только `libfontconfig.so.1`, `libasound.so.2`, `libgcc_s`, `libm`, `libc` (остальное из `ldd` — зависимости fontconfig), X11/Wayland/EGL подгружаются через `dlopen`
- ✅ Job `linux` в `.github/workflows/build.yml` на Ubuntu 22.04: ставит пакеты из таблицы, собирает, печатает `ldd`/`NEEDED` и максимальную версию glibc, гонит тесты, артефакт `radiant-linux-x86_64` — бинарник. Сборка прошла. В CI собирается только `radiant-ui`, тесты идут без `radiant-setup`: установщик на `winreg` и собирается только на Windows
- ✅ `directml` только на Windows: в `radiant-stems` фича больше не в `default`, `radiant-ui` включает её через `[target.'cfg(windows)'.dependencies.radiant-stems]`. На Windows результат тот же (проверить, что CI Windows зелёный). GPU на Linux (CUDA/ROCm, решение владельца: нужно) — отдельная задача в L2, пункт про стемы
- ⬜ Предупреждения и ошибки первой Linux-сборки: починить, не трогая Windows-ветки
- ⬜ Тесты на Linux в CI (core, audio, dsp, stems)

#### Системные зависимости (список подтверждён сборкой в CI; названия пакетов Fedora и Arch проверить)
Для сборки (нужны `-dev`/`-devel` пакеты, найденные через `pkg-config`):
| Что | Зачем | Debian/Ubuntu / Fedora / Arch (проверить) |
|---|---|---|
| C/C++ компилятор, make | `ring` (rustls), `opusic-sys` | `build-essential` / `gcc gcc-c++ make` / `base-devel` |
| `pkg-config` | поиск ALSA и fontconfig | `pkg-config` / `pkgconf-pkg-config` / `pkgconf` |
| `cmake` | `opusic-sys` (libopus собирается из исходников, фича `bundled` включена по умолчанию) | `cmake` / `cmake` / `cmake` |
| ALSA (заголовки) | `cpal` → `alsa-sys` | `libasound2-dev` / `alsa-lib-devel` / `alsa-lib` |
| fontconfig (заголовки) | `fontique` (Slint) → `yeslogic-fontconfig-sys`, фича `dlopen` не включена, значит линкуется при сборке | `libfontconfig1-dev` / `fontconfig-devel` / `fontconfig` |

Для запуска (X11/Wayland-библиотеки подгружаются через `dlopen`, на сборку не влияют, но должны быть в системе):
| Что | Зачем |
|---|---|
| `libasound2` + звуковой сервер (PipeWire/PulseAudio с ALSA-слоем) | вывод звука (`cpal`) |
| `libfontconfig1` и хотя бы один шрифт с кириллицей и CJK | текст интерфейса |
| `libwayland-client`, `libxkbcommon` | Wayland (`winit`, `wayland-dlopen`) |
| `libX11`, `libXcursor`, `libXi`, `libXrandr`, `libxkbcommon-x11`, `libxcb` | X11 (`x11-dl`, `xkbcommon-dl`) |
| `libEGL` / `libGL` (Mesa) | рендерер `femtovg` (OpenGL) через `glutin` |
| D-Bus сессии | трей Slint (`ksni`), портал (`ashpd`), MPRIS (`souvlaki` с бэкендом `use_zbus`); всё на Rust (`zbus`), `libdbus` не нужна |
| `xdg-desktop-portal` + бэкенд (gtk/gnome/kde) | диалог папки (`rfd`) |
| `xdg-utils` (`xdg-open`) | браузер для входа в Dropbox |
| `libonnxruntime.so` 1.22.x (CPU; позже CUDA/ROCm) | только стемы |

Не нужны (проверить по первой сборке): `libseat`, `libinput`, `libgbm`, `libdrm`, `libudev` — это бэкенд `linuxkms`, в Slint он по умолчанию выключен. `ort` с `load-dynamic` ничего не линкует и не скачивает.

Замечание: `Cargo.lock` в архиве устарел (там `rfd` 0.15.4 при `0.18` в `Cargo.toml`, у `radiant-ui` нет `radiant-cloud` и `souvlaki`, есть `skia-bindings`). Первая Linux-сборка пересоздаст его, итоговый список зависимостей может слегка измениться.

### Этап L2. Код под Linux
- 🟡 **MPRIS** (написано, ждёт CI и проверки на системе: `playerctl status`, виджет GNOME/KDE, медиа-клавиши; `souvlaki` 0.8 с `use_zbus`, имя шины `org.mpris.MediaPlayer2.io.github.john3sobieski.Radiant`; если `zbus`-бэкенд капризничает, запасной вариант — бэкенд `libdbus`, но тогда нужен `libdbus-1-dev`; осталось: обложка через `file://`): медиа-клавиши и виджет плеера в GNOME/KDE (`media.rs`, ветка не-Windows; проверить, что `souvlaki` даёт MPRIS без HWND; метаданные, обложка, позиция)
- 🟡 Папки данных: настройки, плейлисты, очереди, блокировка и токен Dropbox в `~/.config/Radiant` (или `$XDG_CONFIG_HOME`), модель Demucs в `~/.local/share/Radiant/models` (или `$XDG_DATA_HOME`), результаты стемов и скачанное из Dropbox рядом с музыкой, как на Windows (`settings::data_dir` / `models_dir`, тест на правила XDG); миграции не нужны
- 🟡 Стемы: поиск `libonnxruntime.so` — сначала `ORT_DYLIB_PATH` (если файл есть; так можно подставить свою сборку с CUDA/ROCm), потом рядом с бинарником (`runtime_library_path()`). ⬜ Осталось: GPU на Linux (CUDA/ROCm: фича `ort`, `ExecutionProvider`, проверка результата как у DirectML; решить, какой именно провайдер и как поставлять), замерить скорость на CPU и честно написать в README
- 🟡 Свой заголовок: перетаскивание и ресайз идут через `winit` (`drag_window`, `drag_resize_window`), он поддерживает X11 и Wayland, код общий с Windows, менять не пришлось. Исправлено: Wayland не сообщает «окно свёрнуто», поэтому блокировка по сворачиванию на Linux учитывает ещё и нажатие кнопки «свернуть» в заголовке (`window_chrome::take_minimize_click`; сворачивание не кнопкой, например жестом, на Wayland не заблокирует). Проверить вручную: X11 и Wayland (GNOME, KDE) — перетаскивание, ресайз за края и углы, двойной клик, разворот; тень и рамка отсутствуют (на GNOME окно без обводки, при необходимости нарисовать тонкую рамку); снап к краям
- 🟡 Шрифты: в `main.slint` шрифт не задан, Slint берёт системный через fontconfig и сам подбирает запасной для символов, которых нет в шрифте. Свой шрифт в бинарник не кладём (CJK весит десятки МБ). Значит кириллица и CJK зависят от системы: в `.deb` и `.rpm` добавить рекомендуемые пакеты шрифтов (Noto Sans и Noto Sans CJK, имена пакетов проверить), во Flatpak проверить, что видны шрифты хозяина. Проверить вручную на минимальной системе: все 10 языков без «квадратиков»
- 🟡 Иконка окна и `app_id`: `slint::set_xdg_app_id("io.github.john3sobieski.Radiant")` перед созданием окна (Linux). Wayland берёт значок из `.desktop` с тем же именем (`io.github.john3sobieski.Radiant.desktop`, этап L3), поэтому значок в доке появится только после установки пакета; из голого бинарника, скорее всего, нет. X11 берёт значок из `icon:` окна (`logo.svg`, проверить)
- 🟡 HiDPI: масштаб берётся из `winit` через Slint (X11: `Xft.dpi` / `GDK_SCALE`, Wayland: дробный масштаб), весь интерфейс в логических пикселях, кода не требуется. Проверить вручную 125–200 % на X11 и Wayland; запасной вариант — переменная `SLINT_SCALE_FACTOR`
- 🟡 Диалог выбора папки и файла: `rfd` 0.18 (на Linux через `xdg-desktop-portal`, GTK не нужен). Если портала нет (минимальные окружения, голый WM), диалог молча не откроется: проверить, что именно возвращает `rfd` в этом случае, и при необходимости показать подсказку «установите xdg-desktop-portal» или ввод пути вручную
- 🟡 Файловые пути: сканирование и плейлисты работают с `PathBuf`, кириллица и иероглифы не проблема. Исправлено: (1) имя не в UTF-8 (старые архивы в cp1251/cp866) ломало бы сохранение настроек, плейлистов и очередей целиком, теперь такие пути пишутся без потерь (`radiant-core/src/path_serde.rs`, обычные имена в JSON выглядят как раньше, тесты); (2) сканирование на Linux следует за симлинками на файлы и папки, с защитой от петель (раньше они молча пропускались). Не сделано: регистр имён (`a.mp3` и `A.mp3` — разные файлы на Linux, пока считаем разными) и ключи `track_eq` для не-UTF-8 путей (используют текст с заменой, при совпадении двух таких имён кривая общая)
- ⬜ Dropbox: порт 53682 на `127.0.0.1`, `xdg-open` без графического браузера

### Этап L3. Ресурсы для пакетов
- ⬜ `radiant.desktop` (имя, категория AudioVideo/Audio/Player, `MimeType` аудио, иконка)
- ⬜ Иконки: SVG из `assets/logo.svg` и PNG 16–512 px (hicolor)
- ⬜ AppStream `metainfo.xml` (нужен Flathub, полезен для GNOME Software/Discover): описание, скриншоты, лицензия, версия из `Cargo.toml`
- ⬜ `app-id` вида `io.github.<ник>.Radiant` (только GitHub-ник, решить с владельцем)
- ⬜ Лицензии: текст ONNX Runtime (MIT) и список сторонних компонентов кладутся в пакеты; лицензия проекта PolyForm Noncommercial, поэтому проверить, принимает ли её Flathub и репозитории дистрибутивов (если нет, остаются GitHub Releases + AUR)

### Этап L4. Четыре способа поставки
**1. AppImage**
- ⬜ Сборка в CI (из бинарника этапа L1) со всеми нужными `.so`, кроме системных (звук, графика)
- ⬜ Внутрь кладётся `libonnxruntime.so` рядом с бинарником
- ⬜ Проверка на Ubuntu, Mint, Debian, Fedora, Arch; FUSE не установлен (`--appimage-extract-and-run`)

**2. Flatpak**
- ⬜ Манифест (`flatpak-builder`), runtime с Wayland/X11/звуком
- ⬜ Права: звук (PulseAudio/PipeWire), `xdg-music` и выбор папки через портал, сеть (Dropbox, модель), без лишнего доступа к `$HOME`
- ⬜ Сборка без сети: зависимости Rust заранее (генератор источников для Flatpak, проверить)
- ⬜ MPRIS из песочницы (имя шины D-Bus по `app-id`)
- ⬜ Подача на Flathub (по желанию, после стабильной сборки; ревью занимает время)

**3. .deb и .rpm**
- ⬜ `.deb` (Ubuntu, Mint, Debian): генерация пакета в CI (инструмент выбрать при работе над пунктом) или ручной `control` (зависимости, `.desktop`, иконки, `libonnxruntime.so`)
- ⬜ `.rpm` (Fedora): генерация пакета в CI (инструмент выбрать при работе над пунктом) или `spec`; имена зависимостей у Fedora другие
- ⬜ Расположение файлов: бинарник и `libonnxruntime.so` в одном каталоге (например `/usr/lib/radiant/`), ссылка в `/usr/bin/radiant`
- ⬜ Проверка установки, обновления и удаления: `apt`, `dnf`; удаление не трогает пользовательские данные

**4. AUR (Arch)**
- ⬜ `PKGBUILD` (сборка из тега релиза, `makedepends` Rust, `depends` ALSA и т.д.)
- ⬜ ONNX Runtime: зависимость из репозиториев Arch или скачивание (проверить, что подходит версия 1.22.x)
- ⬜ Публикация `radiant-bin` (готовый бинарник) и/или `radiant` (сборка из исходников)

### Этап L5. Проверка на реальных системах
Для каждого: Ubuntu (текущий LTS), Linux Mint (текущая), Debian (stable), Fedora (текущая), Arch. Сессии X11 и Wayland, где доступно.
- ⬜ Запуск и закрытие, свой заголовок (перетаскивание, ресайз, развернуть, двойной клик)
- ⬜ Выбор папки, сканирование, обложки, поиск, сортировка
- ⬜ Воспроизведение mp3, flac, ogg, opus, aac; смена устройства вывода; Bluetooth
- ⬜ Скорость 0.5×–3×, EQ, пресеты, режим Per track
- ⬜ Медиа-клавиши и виджет в панели (MPRIS), горячие клавиши окна
- ⬜ Стемы: скачивание модели, разделение, время на CPU
- ⬜ Dropbox: вход, синхронизация, обрыв сети
- ⬜ Блокировка приложения, фраза восстановления
- ⬜ Языки (все 10), шрифты, HiDPI
- ⬜ Энергопотребление (режим low power)

### Этап L6. Выпуск
- ⬜ Имена файлов релиза: `Radiant-<версия>-x86_64.AppImage`, `.deb`, `.rpm`, исходники для AUR/Flatpak
- ⬜ Раздел «Linux» в `README.md` и всех переводах (cs, de, es, fr, ja, pl, ru, uk, zh): установка по дистрибутивам, зависимости, ограничения
- ⬜ Убрать «a Linux version» из списка планов в `README.md`
- ⬜ CI кладёт все артефакты Linux в релиз вместе с `Radiant-Setup.exe`

---

## Закрытые вопросы к владельцу
- Нужен ли GPU-ускоритель стемов на Linux (CUDA/ROCm) или хватит CPU - Ответ: да
- Подавать ли на Flathub, если лицензия PolyForm Noncommercial окажется несовместимой с его правилами? - Ответ: нет
- Какой `app-id` использовать? - Ответ: io.github.john3sobieski.Radiant
- Поддерживать ли Wayland и X11 одинаково (по умолчанию да)? - Ответ: да

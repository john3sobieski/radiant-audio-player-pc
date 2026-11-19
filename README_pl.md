<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant na PC</h1>

<p align="center">
  Szybki i prywatny odtwarzacz muzyki dla Twoich własnych plików audio, z korektorem na studyjnym poziomie i rozdzielaniem ścieżek (stemów) przez AI bez internetu.<br>
  Bez reklam. Bez kont. Bez telemetrii.
</p>

<p align="center">
  <a href="../../releases/latest"><b>Pobierz najnowszą wersję</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Wersja na Androida</a>
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README_de.md">Deutsch</a> ·
  <a href="README_ru.md">Русский</a> ·
  <a href="README_uk.md">Українська</a> ·
  <a href="README_zh.md">中文</a> ·
  <a href="README_ja.md">日本語</a> ·
  <a href="README_fr.md">Français</a> ·
  <a href="README_cs.md">Čeština</a> ·
  <a href="README_es.md">Español</a> ·
  <b>Polski</b>
</p>

<p align="center">
  <a href="https://ko-fi.com/I5I829ZD02"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support on Ko-fi"></a>
  <a href="https://www.buymeacoffee.com/radiant_player"><img src="https://img.shields.io/badge/Support%20the%20project-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy Me a Coffee"></a>
</p>

---

## Dlaczego Radiant

Większość odtwarzaczy chce, żebyś się zalogował, korzystał ze streamingu albo oglądał reklamy. Radiant jest odwrotnością: odtwarza **Twoje** pliki z **Twojego** folderu i nie robi nic za Twoimi plecami.

- **Prywatność z założenia.** Bez reklam, kont, analityki i telemetrii. Wszystko zostaje na Twoim komputerze. Sieć jest używana tylko do pobrania modelu AI i tylko wtedy, gdy naciśniesz przycisk.
- **Rozdzielanie stemów przez AI, offline.** Podziel dowolny utwór na wokal, perkusję, bas i resztę, a następnie zapisz podkład, wersję samego wokalu lub dowolny miks jako jeden plik WAV. Wszystko liczy się na Twoim komputerze (procesor lub karta graficzna), nic nie jest wysyłane.
- **Blokada aplikacji.** Zabezpiecz program 4-cyfrowym, 6-cyfrowym lub własnym kodem. Sam kod nigdy nie jest przechowywany, tylko skrót z solą. Błędne próby są spowalniane, a plik odzyskiwania z 12 słowami przywróci dostęp, jeśli zapomnisz kodu.
- **Prawdziwa regulacja dźwięku.** 12-pasmowy korektor z 10 presetami, osobna krzywa zapamiętywana dla każdego utworu (jeśli chcesz), bas i soprany, filtr subsoniczny, normalizacja głośności, tryb mono i łagodny limiter. Gdy wszystko jest wyłączone, dźwięk przechodzi bez zmian.
- **Prędkość bez „chipmunk”.** Od 3× wolniej do 3× szybciej, wysokość dźwięku pozostaje bez zmian.
- **Oszczędny w zasobach.** Napisany w Rust. Mały program, bez silnika webowego, podczas pauzy nic się nie animuje. Opcjonalny tryb niskiego zużycia wyłącza animacje i efekty dla laptopów i słabszych komputerów.
- **Wygląda tak, jak chcesz.** Jasny, ciemny, czarny (OLED) lub zgodny z Windows; 10 kolorów akcentu dla każdego motywu albo własny kolor.
- **Twój język.** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski.
- **Za darmo do użytku osobistego.** Zobacz [Licencja](#licencja).

## Funkcje

- **Formaty:** MP3, FLAC, OGG/Vorbis, Opus, M4A/AAC, ALAC, WAV.
- **Biblioteka:** Utwory, Albumy, Playlisty i Kolejki. Wyszukiwanie i sortowanie wszędzie, kolejność zmienia się przeciąganiem. Albumy są grupowane automatycznie na podstawie tagów. Playlisty można umieszczać w folderach; „Ulubione utwory” są zawsze dostępne. Cztery stałe kolejki do szybkich list odsłuchu.
- **Folder z muzyką:** wybierz jeden folder w Ustawieniach; Radiant skanuje go przy każdym uruchomieniu (i po naciśnięciu przycisku odświeżania). Nic poza tym folderem nie jest nigdy ruszane.
- **Odtwarzacz:** okładka, plakietka jakości (Hi-Res / Lossless, format, głębia bitowa, częstotliwość próbkowania, przepływność), odtwarzanie losowe, powtarzanie, płynne wyciszanie przy odtwarzaniu i pauzie, dokładne przewijanie.
- **Teksty piosenek:** odczytywane z tagów lub z plików `.lrc` / `.txt` obok utworu. Teksty synchronizowane przewijają się razem z piosenką; kliknięcie wiersza przenosi do niego.
- **Wizualizator:** spektrum w kształcie „słońca” wokół okładki, poruszające się w rytm muzyki.
- **Szczegóły utworu:** wszystkie tagi i dane techniczne pliku.

## Pobieranie i instalacja

Wymagania: **Windows 10 lub 11**, 64-bitowy.

1. Otwórz **[najnowszą wersję](../../releases/latest)** i pobierz instalator (`Radiant-Setup.exe`).
2. Uruchom go i postępuj zgodnie z krokami: folder instalacji, folder z muzyką (opcjonalnie), język aplikacji i dodatkowe opcje (skróty, natychmiastowe pobranie modelu do stemów AI). Radiant instaluje się tylko dla bieżącego użytkownika Windows, więc uprawnienia administratora nie są potrzebne; środowisko uruchomieniowe Visual C++ jest wbudowane. Instalator jest na razie tylko po angielsku.
3. Na ostatniej stronie kliknij **Launch Radiant** albo uruchom program później z wybranego skrótu.
4. Jeśli w instalatorze folder z muzyką został pusty, otwórz **Ustawienia → Biblioteka** i wybierz go.

Windows może wyświetlić komunikat *„System Windows ochronił Twój komputer”* (SmartScreen), ponieważ program nie jest podpisany płatnym certyfikatem. Kliknij **Więcej informacji → Uruchom mimo to**. Kod źródłowy jest publiczny, a każda kompilacja jest z niego budowana przez GitHub Actions.

Aby zaktualizować, uruchom instalator nowszej wersji (najpierw zamknij Radiant): podmieni pliki programu w istniejącym folderze, a Twoje playlisty i modele pozostaną na miejscu.

Ustawienia, playlisty i modele są przechowywane w `%APPDATA%\Radiant`. Aby usunąć Radiant, otwórz **Ustawienia systemu Windows → Aplikacje**, znajdź Radiant i kliknij **Odinstaluj** (albo uruchom `uninstall.exe` z folderu programu). Zaznacz **Also delete my settings and data**, aby usunąć także `%APPDATA%\Radiant`. Twoje pliki muzyczne nigdy nie są ruszane.

### Konfiguracja stemów AI (opcjonalnie)

Dwa pliki potrzebne do stemów (`onnxruntime.dll` i `DirectML.dll`) są instalowane razem z Radiantem, więc nie trzeba niczego kopiować ręcznie. Osobno pobierany jest tylko model Demucs, aby program pozostał mały. Pobieranie możesz zaznaczyć już w instalatorze albo zrobić to później. Reszta aplikacji działa bez modelu.

1. W Radiancie otwórz **Ustawienia → AI i stemy** i naciśnij **Pobierz** przy **Model Demucs** (jednorazowo, plik jest sprawdzany po pobraniu). Plik modelu można też zaimportować ręcznie.
2. Opcjonalnie włącz tam **Karta graficzna (GPU)**, aby przetwarzanie było znacznie szybsze. Jeśli Twoja karta sobie z tym nie poradzi, Radiant po cichu użyje procesora.

Następnie kliknij utwór prawym przyciskiem myszy → **Utwórz stemy** lub użyj karty **Stemy** w odtwarzaczu. Wyniki są zapisywane w `Stems/<Wykonawca - Tytuł>/` wewnątrz folderu z muzyką i dodawane do playlisty w folderze „Stemy”.

## Wersja mobilna

Radiant jest dostępny również na Androida, z lokalną muzyką, odtwarzaniem offline, korektorem, rozdzielaniem stemów przez AI i blokadą aplikacji: **[Radiant na Androida](https://github.com/john3sobieski/radiant-audio-player)**.

## Co dalej

Radiant jest aktywnie rozwijany. Plany, bez określonej kolejności:

- więcej ustawień i personalizacji;
- dalsza optymalizacja (procesor, pamięć, czas uruchamiania);
- usprawnienia interfejsu;
- więcej języków interfejsu;
- klawisze multimedialne Windows i systemowe sterowanie odtwarzaniem;
- opcjonalne jednokierunkowe pobieranie muzyki z magazynu w chmurze (Dropbox);
- być może pobieranie utworów bezpośrednio z innych serwisów;
- obsługa klucza NFC do odblokowywania aplikacji;
- wersja na Linuksa;
- instalator i więcej.

Pomysły i zgłoszenia błędów są mile widziane w [Issues](../../issues).

## Licencja

Radiant jest udostępniany na licencji **[PolyForm Noncommercial License 1.0.0](LICENSE.md)**: bezpłatnie do użytku osobistego i innego niekomercyjnego. Sprzedawanie go ani publikowanie w celu osiągnięcia zysku jest zabronione.

### Komponenty zewnętrzne

[Slint](https://slint.dev) (interfejs), [symphonia](https://github.com/pdeljanov/Symphonia) (dekodowanie, MPL-2.0), libopus (Opus), [cpal](https://github.com/RustAudio/cpal) (wyjście audio), [rubato](https://github.com/HEnquist/rubato) (resampling), [rtrb](https://github.com/mgeier/rtrb), [ort](https://github.com/pykeio/ort) z ONNX Runtime i DirectML (Microsoft, MIT), [rfd](https://github.com/PolyMeilex/rfd), [ureq](https://github.com/algesten/ureq), [image](https://github.com/image-rs/image) oraz model **Demucs (htdemucs)** firmy Meta AI (MIT). Każdy komponent podlega własnej licencji.

<a href="https://slint.dev"><img src="git_images/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="140"></a>

*Radiant tworzy john3sobieski.*

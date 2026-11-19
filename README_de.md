<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant für PC</h1>

<p align="center">
  Ein schneller, privater Musikplayer für deine eigenen Audiodateien, mit Studio-Equalizer und KI-Stem-Trennung ohne Internet.<br>
  Keine Werbung. Keine Konten. Keine Telemetrie.
</p>

<p align="center">
  <a href="../../releases/latest"><b>Neueste Version herunterladen</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Android-Version</a>
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <b>Deutsch</b> ·
  <a href="README_ru.md">Русский</a> ·
  <a href="README_uk.md">Українська</a> ·
  <a href="README_zh.md">中文</a> ·
  <a href="README_ja.md">日本語</a> ·
  <a href="README_fr.md">Français</a> ·
  <a href="README_cs.md">Čeština</a> ·
  <a href="README_es.md">Español</a> ·
  <a href="README_pl.md">Polski</a>
</p>

<p align="center">
  <a href="https://ko-fi.com/I5I829ZD02"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support on Ko-fi"></a>
  <a href="https://www.buymeacoffee.com/radiant_player"><img src="https://img.shields.io/badge/Support%20the%20project-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy Me a Coffee"></a>
</p>

---

## Warum Radiant

Die meisten Player wollen, dass du dich anmeldest, streamst oder Werbung ansiehst. Radiant ist das Gegenteil: Er spielt **deine** Dateien aus **deinem** Ordner und tut sonst nichts hinter deinem Rücken.

- **Privat von Grund auf.** Keine Werbung, keine Konten, keine Analyse, keine Telemetrie. Alles bleibt auf deinem Computer. Das Netzwerk wird nur genutzt, um das KI-Modell herunterzuladen, und nur, wenn du auf die Schaltfläche drückst.
- **KI-Stem-Trennung, offline.** Zerlege jeden Titel in Gesang, Schlagzeug, Bass und Rest und speichere ein Instrumental, eine reine Gesangsversion oder jede beliebige Mischung als eine WAV-Datei. Alles läuft auf deinem PC (Prozessor oder Grafikkarte), nichts wird hochgeladen.
- **App-Sperre.** Schütze die App mit einem 4-stelligen, 6-stelligen oder eigenen Code. Der Code selbst wird nie gespeichert, nur ein gesalzener Hash. Falsche Eingaben werden ausgebremst, und eine Wiederherstellungsdatei mit 12 Wörtern bringt dich wieder hinein, falls du den Code vergisst.
- **Echte Klangregelung.** 12-Band-Equalizer mit 10 Voreinstellungen, auf Wunsch eine eigene Kurve pro Titel, Bass und Höhen, Subsonic-Filter, Lautstärke-Normalisierung, Mono-Modus und ein weicher Limiter. Ist alles aus, wird das Audio unverändert durchgereicht.
- **Tempo ohne Mickymaus-Stimme.** Von 3× langsamer bis 3× schneller, die Tonhöhe bleibt gleich.
- **Schonend für Ressourcen.** In Rust geschrieben. Kleines Programm, keine Web-Engine, in der Pause bewegt sich nichts. Ein optionaler Energiesparmodus schaltet Animationen und Effekte für Laptops und schwache Rechner ab.
- **Sieht aus, wie du willst.** Hell, Dunkel, reines Schwarz (OLED) oder wie Windows; 10 Akzentfarben pro Design oder eine eigene Farbe.
- **Deine Sprache.** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski.
- **Kostenlos für den privaten Gebrauch.** Siehe [Lizenz](#lizenz).

## Funktionen

- **Formate:** MP3, FLAC, OGG/Vorbis, Opus, M4A/AAC, ALAC, WAV.
- **Bibliothek:** Titel, Alben, Playlists und Warteschlangen. Suche und Sortierung überall, Reihenfolge per Drag-and-drop ändern. Alben werden automatisch aus den Tags gebildet. Playlists lassen sich in Ordnern ablegen; „Gelikte Titel“ gibt es immer. Vier feste Warteschlangen für schnelle Hörlisten.
- **Musikordner:** Wähle in den Einstellungen einen Ordner; Radiant liest ihn bei jedem Start ein (und mit der Aktualisieren-Schaltfläche). Nichts außerhalb dieses Ordners wird angefasst.
- **Player:** Cover, Qualitätsabzeichen (Hi-Res / Lossless, Format, Bittiefe, Abtastrate, Bitrate), Zufallswiedergabe, Wiederholen, weiches Ein- und Ausblenden bei Wiedergabe und Pause, präzises Spulen.
- **Songtexte:** aus Tags oder aus `.lrc`- / `.txt`-Dateien neben dem Titel. Synchronisierte Texte laufen mit dem Song mit; ein Klick auf eine Zeile springt dorthin.
- **Visualisierer:** eine Spektrum-„Sonne“ um das Cover, die sich im Takt der Musik bewegt.
- **Titeldetails:** alle Tags und technischen Daten einer Datei.

## Download und Installation

Voraussetzungen: **Windows 10 oder 11**, 64 Bit.

1. Öffne die **[neueste Version](../../releases/latest)** und lade das Installationsprogramm herunter (`Radiant-Setup.exe`).
2. Starte es und folge den Schritten: Installationsordner, dein Musikordner (optional), Sprache der App und zusätzliche Optionen (Verknüpfungen, das KI-Stem-Modell gleich herunterladen). Radiant wird nur für den aktuellen Windows-Benutzer installiert, daher sind keine Administratorrechte nötig; die Visual-C++-Laufzeit ist eingebaut. Das Installationsprogramm gibt es vorerst nur auf Englisch.
3. Klicke auf der letzten Seite auf **Launch Radiant** oder starte Radiant später über die gewählte Verknüpfung.
4. Falls du den Musikordner im Installationsprogramm leer gelassen hast, öffne **Einstellungen → Bibliothek** und wähle ihn aus.

Windows zeigt eventuell die Meldung *„Der Computer wurde durch Windows geschützt“* (SmartScreen), weil das Programm nicht mit einem kostenpflichtigen Zertifikat signiert ist. Klicke auf **Weitere Informationen → Trotzdem ausführen**. Der Quellcode ist öffentlich, und jeder Build wird daraus von GitHub Actions erstellt.

Zum Aktualisieren startest du das Installationsprogramm der neuen Version (schließe Radiant vorher): Es ersetzt die Programmdateien im vorhandenen Ordner, deine Playlists und Modelle bleiben erhalten.

Einstellungen, Playlists und Modelle liegen in `%APPDATA%\Radiant`. Um Radiant zu entfernen, öffne die Windows-**Einstellungen → Apps**, suche Radiant und klicke auf **Deinstallieren** (oder starte `uninstall.exe` im Programmordner). Setze den Haken bei **Also delete my settings and data**, um auch `%APPDATA%\Radiant` zu löschen. Deine Musikdateien werden nie angetastet.

### Einrichtung der KI-Stems (optional)

Die beiden Dateien, die Stems brauchen (`onnxruntime.dll` und `DirectML.dll`), werden zusammen mit Radiant installiert, du musst also nichts von Hand kopieren. Nur das Demucs-Modell wird separat heruntergeladen, damit das Programm klein bleibt. Den Download kannst du schon im Installationsprogramm anhaken oder später nachholen. Ohne das Modell funktioniert der Rest der App.

1. Öffne in Radiant **Einstellungen → KI & Stems** und drücke bei **Demucs-Modell** auf **Herunterladen** (einmalig, die Datei wird nach dem Download geprüft). Du kannst die Modelldatei auch von Hand importieren.
2. Optional: Schalte dort **Grafikkarte (GPU)** ein, für deutlich schnellere Verarbeitung. Wenn deine Karte das nicht kann, nutzt Radiant ohne Meldung den Prozessor.

Danach: Rechtsklick auf einen Titel → **Stems erstellen**, oder nutze den Reiter **Stems** im Player. Die Ergebnisse werden in `Stems/<Interpret - Titel>/` in deinem Musikordner gespeichert und einer Playlist im Ordner „Stems“ hinzugefügt.

## Mobile Version

Radiant gibt es auch für Android, mit lokaler Musik, Offline-Wiedergabe, Equalizer, KI-Stem-Trennung und App-Sperre: **[Radiant für Android](https://github.com/john3sobieski/radiant-audio-player)**.

## Wie geht es weiter

Radiant wird aktiv weiterentwickelt. Pläne, in keiner bestimmten Reihenfolge:

- mehr Einstellungen und Anpassungsmöglichkeiten;
- weitere Optimierung (Prozessor, Speicher, Startzeit);
- Verbesserungen an der Oberfläche;
- mehr Oberflächensprachen;
- Windows-Medientasten und System-Wiedergabesteuerung;
- optionaler Einweg-Download deiner Musik aus dem Cloud-Speicher (Dropbox);
- möglicherweise das direkte Herunterladen von Titeln von anderen Diensten;
- Unterstützung eines NFC-Schlüssels zum Entsperren der App;
- eine Linux-Version;
- ein Installationsprogramm und mehr.

Ideen und Fehlermeldungen sind in den [Issues](../../issues) willkommen.

## Lizenz

Radiant steht unter der **[PolyForm Noncommercial License 1.0.0](LICENSE.md)**: kostenlos für private und andere nichtkommerzielle Nutzung. Der Verkauf oder die Veröffentlichung mit Gewinnabsicht ist nicht erlaubt.

### Komponenten von Drittanbietern

[Slint](https://slint.dev) (Oberfläche), [symphonia](https://github.com/pdeljanov/Symphonia) (Dekodierung, MPL-2.0), libopus (Opus), [cpal](https://github.com/RustAudio/cpal) (Audioausgabe), [rubato](https://github.com/HEnquist/rubato) (Resampling), [rtrb](https://github.com/mgeier/rtrb), [ort](https://github.com/pykeio/ort) mit ONNX Runtime und DirectML (Microsoft, MIT), [rfd](https://github.com/PolyMeilex/rfd), [ureq](https://github.com/algesten/ureq), [image](https://github.com/image-rs/image) sowie das Modell **Demucs (htdemucs)** von Meta AI (MIT). Jede Komponente steht unter ihrer eigenen Lizenz.

<a href="https://slint.dev"><img src="git_images/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="140"></a>

*Radiant wird von john3sobieski entwickelt.*

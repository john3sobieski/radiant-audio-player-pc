<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant pro PC</h1>

<p align="center">
  Rychlý a soukromý přehrávač hudby pro vaše vlastní audiosoubory, s ekvalizérem ve studiové kvalitě a AI oddělováním stop bez připojení k internetu.<br>
  Žádné reklamy. Žádné účty. Žádná telemetrie.
</p>

<p align="center">
  <a href="../../releases/latest"><b>Stáhnout nejnovější verzi</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Verze pro Android</a>
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README_de.md">Deutsch</a> ·
  <a href="README_ru.md">Русский</a> ·
  <a href="README_uk.md">Українська</a> ·
  <a href="README_zh.md">中文</a> ·
  <a href="README_ja.md">日本語</a> ·
  <a href="README_fr.md">Français</a> ·
  <b>Čeština</b> ·
  <a href="README_es.md">Español</a> ·
  <a href="README_pl.md">Polski</a>
</p>

<p align="center">
  <a href="https://ko-fi.com/I5I829ZD02"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support on Ko-fi"></a>
  <a href="https://www.buymeacoffee.com/radiant_player"><img src="https://img.shields.io/badge/Support%20the%20project-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy Me a Coffee"></a>
</p>

---

## Proč Radiant

Většina přehrávačů chce, abyste se přihlásili, streamovali nebo se dívali na reklamy. Radiant je opak: přehrává **vaše** soubory z **vaší** složky a nic dalšího za vašimi zády nedělá.

- **Soukromí od základu.** Žádné reklamy, účty, analytika ani telemetrie. Všechno zůstává ve vašem počítači. Síť se používá jen ke stažení AI modelu, a to pouze po stisknutí tlačítka.
- **AI oddělování stop bez internetu.** Rozdělte libovolnou skladbu na zpěv, bicí, basu a zbytek a uložte instrumentální verzi, verzi jen se zpěvem nebo libovolný mix jako jeden soubor WAV. Všechno se počítá na vašem PC (procesor nebo grafická karta), nic se nenahrává.
- **Zámek aplikace.** Chraňte aplikaci 4místným, 6místným nebo vlastním kódem. Samotný kód se nikdy neukládá, jen hash se solí. Chybné pokusy se zpomalují a obnovovací soubor s 12 slovy vás pustí zpět, pokud kód zapomenete.
- **Skutečné ovládání zvuku.** 12pásmový ekvalizér s 10 předvolbami, na přání samostatná křivka zapamatovaná pro každou skladbu, basy a výšky, subsonický filtr, normalizace hlasitosti, režim mono a měkký limiter. Když je vše vypnuté, zvuk prochází beze změny.
- **Rychlost bez hlasu veverky.** Od 3× pomaleji do 3× rychleji, výška tónu zůstává stejná.
- **Šetrné k prostředkům.** Napsáno v Rustu. Malý program, žádný webový engine, při pauze se nic neanimuje. Volitelný úsporný režim vypne animace a efekty pro notebooky a slabší počítače.
- **Vypadá, jak chcete.** Světlý, tmavý, čistě černý (OLED) nebo podle Windows; 10 barev zvýraznění pro každý motiv, případně vlastní barva.
- **Váš jazyk.** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski.
- **Zdarma pro osobní použití.** Viz [Licence](#licence).

## Funkce

- **Formáty:** MP3, FLAC, OGG/Vorbis, Opus, M4A/AAC, ALAC, WAV.
- **Knihovna:** Skladby, Alba, Playlisty a Fronty. Hledání a řazení všude, pořadí se mění přetažením. Alba se automaticky seskupují podle tagů. Playlisty lze ukládat do složek; „Oblíbené skladby“ jsou vždy k dispozici. Čtyři pevné fronty pro rychlé seznamy k poslechu.
- **Hudební složka:** vyberte jednu složku v Nastavení; Radiant ji prohledá při každém spuštění (a tlačítkem obnovení). Nic mimo tuto složku se nikdy nemění.
- **Přehrávač:** obal alba, odznak kvality (Hi-Res / Lossless, formát, bitová hloubka, vzorkovací frekvence, datový tok), náhodné pořadí, opakování, plynulé zeslabení při přehrávání a pauze, přesné převíjení.
- **Texty písní:** načítají se z tagů nebo ze souborů `.lrc` / `.txt` vedle skladby. Synchronizované texty se posouvají spolu s písní; kliknutím na řádek se na něj skočí.
- **Vizualizér:** spektrální „slunce“ kolem obalu alba, které se pohybuje s hudbou.
- **Podrobnosti o skladbě:** všechny tagy a technické údaje souboru.

## Stažení a instalace

Požadavky: **Windows 10 nebo 11**, 64bitový.

1. Otevřete **[nejnovější verzi](../../releases/latest)** a stáhněte instalátor (`Radiant-Setup.exe`).
2. Spusťte ho a postupujte podle kroků: složka instalace, hudební složka (volitelně), jazyk aplikace a další volby (zástupci, okamžité stažení modelu pro AI stopy). Radiant se instaluje jen pro aktuálního uživatele Windows, takže nejsou potřeba práva správce; běhové prostředí Visual C++ je vestavěné. Instalátor je zatím jen v angličtině.
3. Na poslední stránce klikněte na **Launch Radiant** nebo program později spusťte zvoleným zástupcem.
4. Pokud jste v instalátoru nechali hudební složku prázdnou, otevřete **Nastavení → Knihovna** a vyberte ji.

Windows může zobrazit zprávu *„Windows ochránil váš počítač“* (SmartScreen), protože program není podepsán placeným certifikátem. Klikněte na **Další informace → Přesto spustit**. Zdrojový kód je veřejný a každé sestavení z něj vytváří GitHub Actions.

Chcete-li aktualizovat, spusťte instalátor novější verze (nejprve zavřete Radiant): nahradí soubory programu ve stávající složce a vaše playlisty a modely zůstanou na místě.

Nastavení, playlisty a modely jsou uloženy v `%APPDATA%\Radiant`. Chcete-li Radiant odstranit, otevřete **Nastavení Windows → Aplikace**, najděte Radiant a klikněte na **Odinstalovat** (nebo spusťte `uninstall.exe` ze složky programu). Zaškrtněte **Also delete my settings and data**, aby se smazala i složka `%APPDATA%\Radiant`. Vaše hudební soubory zůstanou nedotčeny.

### Nastavení AI stop (volitelné)

Oba soubory, které stopy potřebují (`onnxruntime.dll` a `DirectML.dll`), se instalují spolu s Radiantem, takže není třeba nic kopírovat ručně. Samostatně se stahuje jen model Demucs, aby program zůstal malý. Stažení můžete zaškrtnout už v instalátoru, nebo ho provést později. Zbytek aplikace funguje i bez modelu.

1. V Radiantu otevřete **Nastavení → AI a stopy** a u **Model Demucs** stiskněte **Stáhnout** (jednou, soubor se po stažení zkontroluje). Soubor modelu můžete také importovat ručně.
2. Volitelně tam zapněte **Grafická karta (GPU)** pro mnohem rychlejší zpracování. Pokud to vaše karta nezvládne, Radiant tiše použije procesor.

Potom klikněte pravým tlačítkem na skladbu → **Vytvořit stopy** nebo použijte kartu **Stopy** v přehrávači. Výsledky se ukládají do `Stems/<Interpret - Název>/` uvnitř vaší hudební složky a přidávají se do playlistu ve složce „Stopy“.

## Mobilní verze

Radiant je k dispozici i pro Android, s místní hudbou, přehráváním offline, ekvalizérem, AI oddělováním stop a zámkem aplikace: **[Radiant pro Android](https://github.com/john3sobieski/radiant-audio-player)**.

## Co dál

Radiant se aktivně vyvíjí. Plány, v libovolném pořadí:

- více nastavení a přizpůsobení;
- další optimalizace (procesor, paměť, doba spuštění);
- vylepšení rozhraní;
- více jazyků rozhraní;
- multimediální klávesy Windows a systémové ovládání přehrávání;
- případně stahování skladeb přímo z jiných služeb;
- podpora NFC klíče pro odemykání aplikace;
- verze pro Linux a další.

Nápady a hlášení chyb uvítáme v [Issues](../../issues).

## Licence

Radiant je šířen pod licencí **[PolyForm Noncommercial License 1.0.0](LICENSE.md)**: zdarma pro osobní a jiné nekomerční použití. Prodávat ho ani zveřejňovat za účelem zisku není dovoleno.

### Komponenty třetích stran

[Slint](https://slint.dev) (rozhraní), [symphonia](https://github.com/pdeljanov/Symphonia) (dekódování, MPL-2.0), libopus (Opus), [cpal](https://github.com/RustAudio/cpal) (zvukový výstup), [rubato](https://github.com/HEnquist/rubato) (převzorkování), [rtrb](https://github.com/mgeier/rtrb), [ort](https://github.com/pykeio/ort) s ONNX Runtime a DirectML (Microsoft, MIT), [rfd](https://github.com/PolyMeilex/rfd), [ureq](https://github.com/algesten/ureq), [image](https://github.com/image-rs/image) a model **Demucs (htdemucs)** od Meta AI (MIT). Každá komponenta podléhá vlastní licenci.

<a href="https://slint.dev"><img src="git_images/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="140"></a>

*Radiant vytváří john3sobieski.*

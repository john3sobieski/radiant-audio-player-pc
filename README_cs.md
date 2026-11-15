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

1. Otevřete **[nejnovější verzi](../../releases/latest)** a stáhněte sestavení pro Windows.
2. Rozbalte `.zip` do libovolné složky (například `C:\Apps\Radiant`).
3. Spusťte `radiant.exe`. Žádný instalátor ani nic k nastavování; běhové prostředí Visual C++ je vestavěné.
4. Otevřete **Nastavení → Knihovna** a vyberte svou hudební složku.

Windows může zobrazit zprávu *„Windows ochránil váš počítač“* (SmartScreen), protože program není podepsán placeným certifikátem. Klikněte na **Další informace → Přesto spustit**. Zdrojový kód je veřejný a každé sestavení z něj vytváří GitHub Actions.

Nastavení, playlisty a modely jsou uloženy v `%APPDATA%\Radiant`. Chcete-li Radiant odstranit, smažte složku s programem a (volitelně) tuto složku.

### Nastavení AI stop (volitelné)

Pro stopy jsou potřeba dva další soubory, které nejsou součástí balíčku, aby program zůstal malý. Zbytek aplikace funguje i bez nich.

*(ve výchozím stavu jsou už zahrnuty v zipu vydání.)*

1. **`onnxruntime.dll`**
   - Stáhněte <https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/1.22.1> (soubor `.nupkg`).
   - `.nupkg` je zip: přejmenujte ho na `.zip` (nebo otevřete v 7-Zip) a vezměte `runtimes/win-x64/native/onnxruntime.dll`.
2. **`DirectML.dll`**
   - Stáhněte <https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/1.15.4>.
   - Otevřete stejným způsobem a vezměte `bin/x64-win/DirectML.dll`.
3. Oba soubory umístěte **vedle `radiant.exe`**.
4. V Radiantu otevřete **Nastavení → AI a stopy** a u **Model Demucs** stiskněte **Stáhnout** (jednou, soubor se po stažení zkontroluje). Soubor modelu můžete také importovat ručně.
5. Volitelně tam zapněte **Grafická karta (GPU)** pro mnohem rychlejší zpracování. Pokud to vaše karta nezvládne, Radiant tiše použije procesor.

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
- volitelné jednosměrné stahování vaší hudby z cloudového úložiště (Dropbox);
- případně stahování skladeb přímo z jiných služeb;
- podpora NFC klíče pro odemykání aplikace;
- verze pro Linux;
- instalátor a další.

Nápady a hlášení chyb uvítáme v [Issues](../../issues).

## Licence

Radiant je šířen pod licencí **[PolyForm Noncommercial License 1.0.0](LICENSE.md)**: zdarma pro osobní a jiné nekomerční použití. Prodávat ho ani zveřejňovat za účelem zisku není dovoleno.

### Komponenty třetích stran

[Slint](https://slint.dev) (rozhraní), [symphonia](https://github.com/pdeljanov/Symphonia) (dekódování, MPL-2.0), libopus (Opus), [cpal](https://github.com/RustAudio/cpal) (zvukový výstup), [rubato](https://github.com/HEnquist/rubato) (převzorkování), [rtrb](https://github.com/mgeier/rtrb), [ort](https://github.com/pykeio/ort) s ONNX Runtime a DirectML (Microsoft, MIT), [rfd](https://github.com/PolyMeilex/rfd), [ureq](https://github.com/algesten/ureq), [image](https://github.com/image-rs/image) a model **Demucs (htdemucs)** od Meta AI (MIT). Každá komponenta podléhá vlastní licenci.

*Radiant vytváří john3sobieski.*

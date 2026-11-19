<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant pour PC</h1>

<p align="center">
  Un lecteur de musique rapide et privé pour vos propres fichiers audio, avec un égaliseur de niveau studio et la séparation de stems par IA, hors ligne.<br>
  Sans publicité. Sans compte. Sans télémétrie.
</p>

<p align="center">
  <a href="../../releases/latest"><b>Télécharger la dernière version</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Version Android</a>
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README_de.md">Deutsch</a> ·
  <a href="README_ru.md">Русский</a> ·
  <a href="README_uk.md">Українська</a> ·
  <a href="README_zh.md">中文</a> ·
  <a href="README_ja.md">日本語</a> ·
  <b>Français</b> ·
  <a href="README_cs.md">Čeština</a> ·
  <a href="README_es.md">Español</a> ·
  <a href="README_pl.md">Polski</a>
</p>

<p align="center">
  <a href="https://ko-fi.com/I5I829ZD02"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support on Ko-fi"></a>
  <a href="https://www.buymeacoffee.com/radiant_player"><img src="https://img.shields.io/badge/Support%20the%20project-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy Me a Coffee"></a>
</p>

---

## Pourquoi Radiant

La plupart des lecteurs veulent que vous vous connectiez, fassiez du streaming ou regardiez des publicités. Radiant fait l'inverse : il lit **vos** fichiers depuis **votre** dossier et ne fait rien d'autre dans votre dos.

- **Privé par conception.** Pas de publicité, pas de compte, pas d'analyse, pas de télémétrie. Tout reste sur votre ordinateur. Le réseau n'est utilisé que pour télécharger le modèle d'IA, et seulement quand vous appuyez sur le bouton.
- **Séparation de stems par IA, hors ligne.** Divisez n'importe quel morceau en voix, batterie, basse et reste, puis enregistrez un instrumental, une version voix seule ou n'importe quel mélange dans un seul fichier WAV. Tout se calcule sur votre PC (processeur ou carte graphique), rien n'est envoyé.
- **Verrouillage de l'application.** Protégez le programme avec un code à 4 chiffres, à 6 chiffres ou personnalisé. Le code lui-même n'est jamais stocké, seulement une empreinte salée. Les essais ratés sont ralentis, et un fichier de récupération de 12 mots vous permet de rentrer si vous oubliez le code.
- **De vrais réglages sonores.** Égaliseur 12 bandes avec 10 préréglages, courbe distincte mémorisée par morceau si vous le souhaitez, basses et aigus, filtre subsonique, normalisation du volume, mode mono et limiteur doux. Quand tout est désactivé, l'audio passe sans aucun traitement.
- **Vitesse sans voix de chipmunk.** De 3 fois plus lent à 3 fois plus rapide, la hauteur du son reste la même.
- **Léger en ressources.** Écrit en Rust. Petit programme, pas de moteur web, rien ne s'anime en pause. Un mode économie d'énergie facultatif coupe les animations et les effets pour les portables et les machines modestes.
- **L'apparence que vous voulez.** Clair, sombre, noir pur (OLED) ou celui de Windows ; 10 couleurs d'accent par thème, ou la vôtre.
- **Votre langue.** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski.
- **Gratuit pour un usage personnel.** Voir la [Licence](#licence).

## Fonctionnalités

- **Formats :** MP3, FLAC, OGG/Vorbis, Opus, M4A/AAC, ALAC, WAV.
- **Bibliothèque :** Pistes, Albums, Playlists et Files d'attente. Recherche et tri partout, réorganisation par glisser-déposer. Les albums sont regroupés automatiquement d'après les tags. Les playlists peuvent être rangées dans des dossiers ; « Pistes aimées » est toujours là. Quatre files d'attente fixes pour des listes d'écoute rapides.
- **Dossier de musique :** choisissez un dossier dans les Paramètres ; Radiant l'analyse à chaque démarrage (et avec le bouton d'actualisation). Rien en dehors de ce dossier n'est jamais touché.
- **Lecteur :** pochette, badge de qualité (Hi-Res / Lossless, format, profondeur de bits, fréquence d'échantillonnage, débit), lecture aléatoire, répétition, fondu doux à la lecture et à la pause, recherche précise dans le morceau.
- **Paroles :** lues depuis les tags ou depuis des fichiers `.lrc` / `.txt` placés à côté du morceau. Les paroles synchronisées défilent avec la chanson ; un clic sur une ligne y saute.
- **Visualiseur :** un « soleil » de spectre autour de la pochette, qui bouge avec la musique.
- **Détails du morceau :** tous les tags et les données techniques d'un fichier.

## Téléchargement et installation

Configuration requise : **Windows 10 ou 11**, 64 bits.

1. Ouvrez la **[dernière version](../../releases/latest)** et téléchargez l'installateur (`Radiant-Setup.exe`).
2. Lancez-le et suivez les étapes : dossier d'installation, dossier de musique (facultatif), langue de l'application et options supplémentaires (raccourcis, téléchargement immédiat du modèle pour les stems par IA). Radiant s'installe uniquement pour l'utilisateur Windows actuel, aucun droit d'administrateur n'est donc nécessaire ; le runtime Visual C++ est intégré. L'installateur n'existe pour l'instant qu'en anglais.
3. Sur la dernière page, cliquez sur **Launch Radiant**, ou lancez le programme plus tard avec le raccourci choisi.
4. Si vous avez laissé le dossier de musique vide dans l'installateur, ouvrez **Paramètres → Bibliothèque** et choisissez-le.

Windows peut afficher un message *« Windows a protégé votre ordinateur »* (SmartScreen), car le programme n'est pas signé avec un certificat payant. Cliquez sur **Informations complémentaires → Exécuter quand même**. Le code source est public, et chaque version est construite à partir de celui-ci par GitHub Actions.

Pour mettre à jour, lancez l'installateur de la nouvelle version (fermez d'abord Radiant) : il remplace les fichiers du programme dans le dossier existant, et vos playlists et modèles restent en place.

Les paramètres, les playlists et les modèles sont stockés dans `%APPDATA%\Radiant`. Pour supprimer Radiant, ouvrez **Paramètres Windows → Applications**, trouvez Radiant et cliquez sur **Désinstaller** (ou lancez `uninstall.exe` depuis le dossier du programme). Cochez **Also delete my settings and data** pour supprimer aussi `%APPDATA%\Radiant`. Vos fichiers de musique ne sont jamais touchés.

### Configuration des stems par IA (facultatif)

Les deux fichiers nécessaires aux stems (`onnxruntime.dll` et `DirectML.dll`) sont installés avec Radiant, il n'y a donc rien à copier à la main. Seul le modèle Demucs est téléchargé séparément, afin de garder le programme léger. Vous pouvez cocher son téléchargement dans l'installateur ou le faire plus tard. Le reste de l'application fonctionne sans le modèle.

1. Dans Radiant, ouvrez **Paramètres → IA et stems** et appuyez sur **Télécharger** pour le **Modèle Demucs** (une seule fois, le fichier est vérifié après le téléchargement). Vous pouvez aussi importer le fichier du modèle à la main.
2. Facultatif : activez-y **Carte graphique (GPU)** pour un traitement bien plus rapide. Si votre carte n'en est pas capable, Radiant utilise discrètement le processeur.

Ensuite, clic droit sur un morceau → **Créer les stems**, ou utilisez l'onglet **Stems** du lecteur. Les résultats sont enregistrés dans `Stems/<Artiste - Titre>/` à l'intérieur de votre dossier de musique et ajoutés à une playlist du dossier « Stems ».

## Version mobile

Radiant existe aussi pour Android, avec musique locale, lecture hors ligne, égaliseur, séparation de stems par IA et verrouillage de l'application : **[Radiant pour Android](https://github.com/john3sobieski/radiant-audio-player)**.

## Et ensuite

Radiant est activement développé. Projets, sans ordre particulier :

- plus de paramètres et de personnalisation ;
- optimisation poussée (processeur, mémoire, temps de démarrage) ;
- améliorations de l'interface ;
- plus de langues d'interface ;
- touches multimédia de Windows et contrôles de lecture du système ;
- téléchargement unidirectionnel facultatif de votre musique depuis un stockage cloud (Dropbox) ;
- peut-être le téléchargement direct de morceaux depuis d'autres services ;
- prise en charge d'une clé NFC pour déverrouiller l'application ;
- une version Linux ;
- un installateur et plus encore.

Les idées et les rapports de bugs sont les bienvenus dans les [Issues](../../issues).

## Licence

Radiant est distribué sous la **[PolyForm Noncommercial License 1.0.0](LICENSE.md)** : gratuit pour un usage personnel et tout autre usage non commercial. Il est interdit de le vendre ou de le publier dans un but lucratif.

### Composants tiers

[Slint](https://slint.dev) (interface), [symphonia](https://github.com/pdeljanov/Symphonia) (décodage, MPL-2.0), libopus (Opus), [cpal](https://github.com/RustAudio/cpal) (sortie audio), [rubato](https://github.com/HEnquist/rubato) (rééchantillonnage), [rtrb](https://github.com/mgeier/rtrb), [ort](https://github.com/pykeio/ort) avec ONNX Runtime et DirectML (Microsoft, MIT), [rfd](https://github.com/PolyMeilex/rfd), [ureq](https://github.com/algesten/ureq), [image](https://github.com/image-rs/image), ainsi que le modèle **Demucs (htdemucs)** de Meta AI (MIT). Chacun est soumis à sa propre licence.

<a href="https://slint.dev"><img src="git_images/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="140"></a>

*Radiant est développé par john3sobieski.*

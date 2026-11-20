<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant for PC</h1>

<p align="center">
  あなた自身の音声ファイルのための、高速でプライバシーを守るミュージックプレーヤー。スタジオ品質のイコライザーと、オフラインで使える AI ステム分離を搭載。<br>
  広告なし。アカウント不要。テレメトリなし。
</p>

<p align="center">
  <a href="../../releases/latest"><b>最新リリースをダウンロード</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Android 版</a>
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README_de.md">Deutsch</a> ·
  <a href="README_ru.md">Русский</a> ·
  <a href="README_uk.md">Українська</a> ·
  <a href="README_zh.md">中文</a> ·
  <b>日本語</b> ·
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

## Radiant を選ぶ理由

多くのプレーヤーは、サインインやストリーミング、広告の表示を求めます。Radiant はその逆です。**あなたの**フォルダにある**あなたの**ファイルを再生するだけで、知らないところで何かをすることはありません。

- **設計から守られるプライバシー。** 広告、アカウント、分析、テレメトリはありません。すべてがあなたのコンピューターの中に留まります。ネットワークを使うのは AI モデルのダウンロード時だけで、ボタンを押したときに限られます。
- **オフラインの AI ステム分離。** どんな曲でもボーカル、ドラム、ベース、その他に分け、インストゥルメンタル、ボーカルのみ、または好きなミックスを 1 つの WAV ファイルとして保存できます。処理はお使いの PC（CPU またはグラフィックスカード）上で行われ、何もアップロードされません。
- **アプリロック。** 4 桁、6 桁、または自分で決めたコードでアプリを保護します。コード自体は保存されず、ソルト付きハッシュだけが保存されます。間違えるたびに待ち時間が延び、コードを忘れても 12 個の単語を記した復元ファイルで再び入れます。
- **本格的なサウンド調整。** 10 種類のプリセットを備えた 12 バンドイコライザー、必要に応じて曲ごとに記憶できる専用カーブ、ベースとトレブル、サブソニックフィルター、音量ノーマライゼーション、モノラルモード、ソフトリミッター。すべてオフにすると、音声は手を加えずにそのまま出力されます。
- **声が変にならない速度変更。** 3 倍遅くから 3 倍速まで、音の高さは変わりません。
- **軽量。** Rust で書かれています。小さなプログラムで、Web ビューは使わず、一時停止中は何もアニメーションしません。オプションの低消費電力モードでは、ノート PC や低スペックの PC 向けにアニメーションとエフェクトをオフにできます。
- **好みの見た目に。** ライト、ダーク、純黒（OLED）、または Windows に合わせる設定。テーマごとに 10 色のアクセントカラーがあり、自分で色を選ぶこともできます。
- **あなたの言語で。** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski。
- **個人利用は無料。** [ライセンス](#ライセンス)をご覧ください。

## 機能

- **対応フォーマット:** MP3、FLAC、OGG/Vorbis、Opus、M4A/AAC、ALAC、WAV。
- **ライブラリ:** 曲、アルバム、プレイリスト、キュー。どこでも検索と並べ替えができ、ドラッグ＆ドロップで順序を変更できます。アルバムはタグから自動的にまとめられます。プレイリストはフォルダに整理でき、「お気に入りの曲」は常に用意されています。手軽なリスニングリスト用に、固定のキューが 4 つあります。
- **音楽フォルダ:** 設定で 1 つのフォルダを選びます。Radiant は起動のたびに（および更新ボタンで）そのフォルダをスキャンします。そのフォルダの外には一切触れません。
- **プレーヤー:** カバーアート、音質バッジ（Hi-Res / Lossless、フォーマット、ビット深度、サンプリング周波数、ビットレート）、シャッフル、リピート、再生・一時停止時のなめらかなフェード、正確なシーク。
- **歌詞:** タグ、または曲と同じ場所にある `.lrc` / `.txt` ファイルから読み込みます。同期歌詞は曲に合わせてスクロールし、行をクリックするとその位置にジャンプします。
- **ビジュアライザー:** カバーアートの周りに広がるスペクトラムの「太陽」が、音楽に合わせて動きます。
- **曲の詳細:** ファイルのすべてのタグと技術データ。

## ダウンロードとインストール

動作環境: **Windows 10 または 11**、64 ビット。

1. **[最新リリース](../../releases/latest)** を開き、インストーラー（`Radiant-Setup.exe`）をダウンロードします。
2. 実行して、画面の手順に従います。インストール先フォルダ、音楽フォルダ（任意）、アプリの言語、追加オプション（ショートカット、AI ステム用モデルをすぐにダウンロードするか）を選びます。Radiant は現在の Windows ユーザー用にのみインストールされるため、管理者権限は不要です。Visual C++ ランタイムは組み込み済みです。インストーラーは今のところ英語のみです。
3. 最後のページで **Launch Radiant** を押すか、選択したショートカットから後で起動します。
4. インストーラーで音楽フォルダを空のままにした場合は、**設定 → ライブラリ** を開いてフォルダを選択します。

有料の証明書で署名されていないため、Windows に *「Windows によって PC が保護されました」*（SmartScreen）というメッセージが表示されることがあります。**詳細情報 → 実行** をクリックしてください。ソースコードは公開されており、すべてのビルドは GitHub Actions がそこから作成します。

更新するには、新しいバージョンのインストーラーを実行します（先に Radiant を終了してください）。既存のフォルダ内のプログラムファイルが置き換えられ、プレイリストとモデルはそのまま残ります。

設定、プレイリスト、モデルは `%APPDATA%\Radiant` に保存されます。Radiant を削除するには、Windows の **設定 → アプリ** で Radiant を探して **アンインストール** をクリックします（またはプログラムフォルダ内の `uninstall.exe` を実行します）。**Also delete my settings and data** にチェックを入れると、`%APPDATA%\Radiant` も一緒に削除されます。音楽ファイルには一切触れません。

### AI ステムのセットアップ（任意）

ステムに必要な 2 つのファイル（`onnxruntime.dll` と `DirectML.dll`）は Radiant と一緒にインストールされるため、手動でコピーする必要はありません。プログラムを小さく保つため、別途ダウンロードされるのは Demucs モデルだけです。ダウンロードはインストーラーでチェックしておくことも、後から行うこともできます。モデルがなくても、アプリのその他の機能は動作します。

1. Radiant で **設定 → AI とステム** を開き、**Demucs モデル**の**ダウンロード**を押します（1 回だけ。ダウンロード後にファイルが検証されます）。モデルファイルを手動で読み込むこともできます。
2. 任意: そこで**グラフィックスカード（GPU）**をオンにすると、処理がはるかに速くなります。お使いのカードが対応していない場合は、Radiant が自動的に CPU を使います。

そのあと、曲を右クリック → **ステムを作成**、またはプレーヤーの**ステム**タブを使います。結果は音楽フォルダ内の `Stems/<アーティスト - タイトル>/` に保存され、「ステム」フォルダ内のプレイリストに追加されます。

## モバイル版

Radiant は Android でも利用できます。ローカル音楽、オフライン再生、イコライザー、AI ステム分離、アプリロックに対応しています: **[Radiant for Android](https://github.com/john3sobieski/radiant-audio-player)**。

## 今後の予定

Radiant は活発に開発中です。計画は次のとおりです（順不同）:

- 設定とカスタマイズの拡充;
- さらなる最適化（CPU、メモリ、起動時間）;
- インターフェースの改善;
- 対応言語の追加;
- Windows のメディアキーとシステムの再生コントロール;
- 他のサービスから曲を直接ダウンロードする機能（検討中）;
- NFC キーによるアプリのロック解除;
- Linux 版.

ご意見やバグ報告は [Issues](../../issues) までお寄せください。

## ライセンス

Radiant は **[PolyForm Noncommercial License 1.0.0](LICENSE.md)** のもとで提供されています。個人利用およびその他の非商用利用は無料です。販売や、営利目的での公開は認められていません。

### サードパーティコンポーネント

[Slint](https://slint.dev)（インターフェース）、[symphonia](https://github.com/pdeljanov/Symphonia)（デコード、MPL-2.0）、libopus（Opus）、[cpal](https://github.com/RustAudio/cpal)（オーディオ出力）、[rubato](https://github.com/HEnquist/rubato)（リサンプリング）、[rtrb](https://github.com/mgeier/rtrb)、ONNX Runtime と DirectML を使う [ort](https://github.com/pykeio/ort)（Microsoft、MIT）、[rfd](https://github.com/PolyMeilex/rfd)、[ureq](https://github.com/algesten/ureq)、[image](https://github.com/image-rs/image)、そして Meta AI の **Demucs (htdemucs)** モデル（MIT）。それぞれ各自のライセンスに従います。

<a href="https://slint.dev"><img src="git_images/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="140"></a>

*Radiant は john3sobieski が制作しています。*

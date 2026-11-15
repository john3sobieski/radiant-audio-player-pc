<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant PC 版</h1>

<p align="center">
  一款快速、注重隐私的本地音乐播放器，配备录音室级均衡器和无需联网的 AI 分轨功能。<br>
  无广告。无账号。无遥测。
</p>

<p align="center">
  <a href="../../releases/latest"><b>下载最新版本</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Android 版</a>
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README_de.md">Deutsch</a> ·
  <a href="README_ru.md">Русский</a> ·
  <a href="README_uk.md">Українська</a> ·
  <b>中文</b> ·
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

## 为什么选择 Radiant

大多数播放器要你登录、在线播放或者看广告。Radiant 恰恰相反：它只播放**你的**文件夹里**你的**文件，不会在背后做任何别的事。

- **天生注重隐私。** 无广告、无账号、无数据分析、无遥测。一切都留在你的电脑上。仅在你点击按钮时才会联网下载 AI 模型。
- **离线 AI 分轨。** 把任意曲目拆分为人声、鼓、贝斯和其他部分，并将伴奏、纯人声或任意组合保存为一个 WAV 文件。全部在你的电脑上运行（处理器或显卡），不会上传任何内容。
- **应用锁。** 用 4 位、6 位或自定义密码保护应用。密码本身不会被保存，只保存加盐哈希。输错会被限速，忘记密码时，包含 12 个单词的恢复文件可以让你重新进入。
- **真正的音效控制。** 12 段均衡器和 10 个预设，可选的每首曲目独立曲线，低音和高音、次声滤波、音量标准化、单声道模式和柔和限幅器。全部关闭时，音频原样通过。
- **变速不变调。** 从慢 3 倍到快 3 倍，音高保持不变。
- **节省资源。** 使用 Rust 编写。程序小巧，没有网页引擎，暂停时没有任何动画。可选的低功耗模式会为笔记本电脑和配置较低的电脑关闭动画和特效。
- **外观由你决定。** 浅色、深色、纯黑（OLED）或跟随 Windows；每种主题提供 10 种强调色，也可以自选颜色。
- **你的语言。** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski。
- **个人使用免费。** 参见[许可证](#许可证)。

## 功能

- **格式：** MP3、FLAC、OGG/Vorbis、Opus、M4A/AAC、ALAC、WAV。
- **音乐库：** 曲目、专辑、播放列表和队列。各处均可搜索和排序，拖放即可调整顺序。专辑会根据标签自动归类。播放列表可以放进文件夹；“喜欢的曲目”始终存在。四个固定队列，用于快速收听列表。
- **音乐文件夹：** 在设置中选择一个文件夹；Radiant 每次启动时都会扫描它（也可点击刷新按钮）。该文件夹之外的内容绝不会被触碰。
- **播放器：** 封面、音质徽章（Hi-Res / Lossless、格式、位深度、采样率、比特率）、随机播放、循环、播放和暂停时的平滑淡入淡出、精确跳转。
- **歌词：** 从标签或曲目旁的 `.lrc` / `.txt` 文件读取。同步歌词随歌曲滚动；点击某一行即可跳转到该处。
- **可视化：** 封面周围的频谱“太阳”，随音乐律动。
- **曲目详情：** 文件的所有标签和技术数据。

## 下载与安装

系统要求：**Windows 10 或 11**，64 位。

1. 打开**[最新版本](../../releases/latest)**，下载 Windows 版本。
2. 将 `.zip` 解压到任意文件夹（例如 `C:\Apps\Radiant`）。
3. 运行 `radiant.exe`。无需安装程序，也无需任何设置；Visual C++ 运行库已内置。
4. 打开**设置 → 音乐库**，选择你的音乐文件夹。

Windows 可能会显示*“Windows 已保护你的电脑”*（SmartScreen）的提示，因为本程序没有使用付费证书签名。点击**更多信息 → 仍要运行**。源代码是公开的，每个版本都由 GitHub Actions 根据它构建。

设置、播放列表和模型保存在 `%APPDATA%\Radiant`。要卸载 Radiant，请删除程序文件夹，以及（可选）该文件夹。

### AI 分轨设置（可选）

分轨需要两个额外文件，为了让程序保持小巧，它们没有随程序打包。没有它们，应用的其余功能仍可正常使用。

*（默认情况下，它们已包含在发布版的 zip 中。）*

1. **`onnxruntime.dll`**
   - 下载 <https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/1.22.1>（一个 `.nupkg` 文件）。
   - `.nupkg` 本质上是 zip：将其重命名为 `.zip`（或用 7-Zip 打开），取出 `runtimes/win-x64/native/onnxruntime.dll`。
2. **`DirectML.dll`**
   - 下载 <https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/1.15.4>。
   - 用同样的方法打开，取出 `bin/x64-win/DirectML.dll`。
3. 将这两个文件放在 **`radiant.exe` 旁边**。
4. 在 Radiant 中打开**设置 → AI 与分轨**，点击 **Demucs 模型**旁的**下载**（仅需一次，下载后会校验文件）。你也可以手动导入模型文件。
5. 可选：在那里开启**显卡（GPU）**，处理速度会快得多。如果你的显卡不支持，Radiant 会悄悄改用处理器。

然后右键点击曲目 → **创建分轨**，或使用播放器中的**分轨**标签页。结果会保存到音乐文件夹内的 `Stems/<艺术家 - 标题>/`，并添加到“分轨”文件夹中的一个播放列表。

## 移动版

Radiant 也有 Android 版，支持本地音乐、离线播放、均衡器、AI 分轨和应用锁：**[Radiant for Android](https://github.com/john3sobieski/radiant-audio-player)**。

## 后续计划

Radiant 正在积极开发中。计划如下，不分先后：

- 更多设置和个性化选项；
- 进一步优化（处理器、内存、启动时间）；
- 界面改进；
- 更多界面语言；
- Windows 媒体键和系统播放控制；
- 可选的从云存储（Dropbox）单向下载音乐；
- 可能支持直接从其他服务下载曲目；
- 支持用 NFC 密钥解锁应用；
- Linux 版本；
- 安装程序等更多内容。

欢迎在 [Issues](../../issues) 中提出想法和报告错误。

## 许可证

Radiant 采用 **[PolyForm Noncommercial License 1.0.0](LICENSE.md)** 许可：个人及其他非商业用途免费。不得出售或以营利为目的发布。

### 第三方组件

[Slint](https://slint.dev)（界面）、[symphonia](https://github.com/pdeljanov/Symphonia)（解码，MPL-2.0）、libopus（Opus）、[cpal](https://github.com/RustAudio/cpal)（音频输出）、[rubato](https://github.com/HEnquist/rubato)（重采样）、[rtrb](https://github.com/mgeier/rtrb)、带有 ONNX Runtime 和 DirectML 的 [ort](https://github.com/pykeio/ort)（Microsoft，MIT）、[rfd](https://github.com/PolyMeilex/rfd)、[ureq](https://github.com/algesten/ureq)、[image](https://github.com/image-rs/image)，以及 Meta AI 的 **Demucs (htdemucs)** 模型（MIT）。每个组件均遵循其各自的许可证。

*Radiant 由 john3sobieski 制作。*

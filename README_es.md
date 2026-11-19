<p align="center">
  <img src="/git_images/Radiant.png" width="800" alt="Radiant">
</p>

<h1 align="center">Radiant para PC</h1>

<p align="center">
  Un reproductor de música rápido y privado para tus propios archivos de audio, con un ecualizador de nivel de estudio y separación de stems con IA sin conexión.<br>
  Sin anuncios. Sin cuentas. Sin telemetría.
</p>

<p align="center">
  <a href="../../releases/latest"><b>Descargar la última versión</b></a> ·
  <a href="https://github.com/john3sobieski/radiant-audio-player">Versión para Android</a>
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
  <b>Español</b> ·
  <a href="README_pl.md">Polski</a>
</p>

<p align="center">
  <a href="https://ko-fi.com/I5I829ZD02"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support on Ko-fi"></a>
  <a href="https://www.buymeacoffee.com/radiant_player"><img src="https://img.shields.io/badge/Support%20the%20project-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy Me a Coffee"></a>
</p>

---

## Por qué Radiant

La mayoría de los reproductores quieren que inicies sesión, hagas streaming o veas anuncios. Radiant es lo contrario: reproduce **tus** archivos de **tu** carpeta y no hace nada más a tus espaldas.

- **Privado por diseño.** Sin anuncios, sin cuentas, sin analítica, sin telemetría. Todo se queda en tu ordenador. La red solo se usa para descargar el modelo de IA, y solo cuando pulsas el botón.
- **Separación de stems con IA, sin conexión.** Divide cualquier pista en voz, batería, bajo y resto, y guarda un instrumental, una versión solo de voz o cualquier mezcla que quieras en un único archivo WAV. Todo se calcula en tu PC (procesador o tarjeta gráfica), no se sube nada.
- **Bloqueo de la app.** Protege la aplicación con un código de 4 dígitos, de 6 dígitos o uno personalizado. El código en sí nunca se guarda, solo un hash con sal. Los intentos fallidos se ralentizan, y un archivo de recuperación de 12 palabras te devuelve el acceso si olvidas el código.
- **Controles de sonido de verdad.** Ecualizador de 12 bandas con 10 preajustes, una curva distinta recordada por pista si quieres, graves y agudos, filtro subsónico, normalización de volumen, modo mono y un limitador suave. Con todo desactivado, el audio pasa sin tocar.
- **Velocidad sin voz de ardilla.** De 3 veces más lento a 3 veces más rápido, con el mismo tono.
- **Ligero en recursos.** Escrito en Rust. Programa pequeño, sin motor web, nada se anima cuando pausas. Un modo de bajo consumo opcional desactiva las animaciones y los efectos para portátiles y equipos modestos.
- **Con el aspecto que quieras.** Claro, oscuro, negro puro (OLED) o el de Windows; 10 colores de acento por tema, o elige el tuyo.
- **Tu idioma.** English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski.
- **Gratis para uso personal.** Consulta la [Licencia](#licencia).

## Características

- **Formatos:** MP3, FLAC, OGG/Vorbis, Opus, M4A/AAC, ALAC, WAV.
- **Biblioteca:** Pistas, Álbumes, Listas y Colas. Búsqueda y ordenación en todas partes, arrastra y suelta para reordenar. Los álbumes se agrupan automáticamente a partir de las etiquetas. Las listas pueden guardarse en carpetas; «Pistas favoritas» siempre está ahí. Cuatro colas fijas para listas de escucha rápidas.
- **Carpeta de música:** elige una carpeta en Ajustes; Radiant la escanea en cada inicio (y con el botón de actualizar). Nunca se toca nada fuera de esa carpeta.
- **Reproductor:** carátula, insignia de calidad (Hi-Res / Lossless, formato, profundidad de bits, frecuencia de muestreo, tasa de bits), aleatorio, repetición, fundido suave al reproducir y pausar, búsqueda precisa.
- **Letras:** se leen de las etiquetas o de archivos `.lrc` / `.txt` junto a la pista. Las letras sincronizadas se desplazan con la canción; al hacer clic en una línea se salta a ella.
- **Visualizador:** un «sol» de espectro alrededor de la carátula que se mueve con la música.
- **Detalles de la pista:** todas las etiquetas y los datos técnicos de un archivo.

## Descarga e instalación

Requisitos: **Windows 10 u 11**, 64 bits.

1. Abre la **[última versión](../../releases/latest)** y descarga el instalador (`Radiant-Setup.exe`).
2. Ejecútalo y sigue los pasos: carpeta de instalación, tu carpeta de música (opcional), idioma de la aplicación y opciones adicionales (accesos directos, descargar ya el modelo de stems con IA). Radiant se instala solo para el usuario actual de Windows, así que no hacen falta permisos de administrador; el runtime de Visual C++ está incluido. Por ahora el instalador solo está en inglés.
3. En la última página pulsa **Launch Radiant**, o inicia el programa más tarde con el acceso directo que elegiste.
4. Si dejaste vacía la carpeta de música en el instalador, abre **Ajustes → Biblioteca** y elígela.

Windows puede mostrar el mensaje *«Windows protegió su PC»* (SmartScreen), porque el programa no está firmado con un certificado de pago. Haz clic en **Más información → Ejecutar de todas formas**. El código fuente es público, y cada compilación la genera GitHub Actions a partir de él.

Para actualizar, ejecuta el instalador de la nueva versión (cierra Radiant antes): sustituye los archivos del programa en la carpeta existente, y tus listas y modelos se quedan donde están.

Los ajustes, las listas y los modelos se guardan en `%APPDATA%\Radiant`. Para desinstalar Radiant, abre **Configuración de Windows → Aplicaciones**, busca Radiant y pulsa **Desinstalar** (o ejecuta `uninstall.exe` desde la carpeta del programa). Marca **Also delete my settings and data** para borrar también `%APPDATA%\Radiant`. Tus archivos de música nunca se tocan.

### Configuración de los stems con IA (opcional)

Los dos archivos que necesitan los stems (`onnxruntime.dll` y `DirectML.dll`) se instalan junto con Radiant, así que no hay que copiar nada a mano. Solo el modelo Demucs se descarga aparte, para mantener el programa pequeño. Puedes marcar su descarga en el instalador o hacerlo más tarde. El resto de la aplicación funciona sin el modelo.

1. En Radiant abre **Ajustes → IA y stems** y pulsa **Descargar** en el **Modelo Demucs** (una sola vez, el archivo se comprueba tras la descarga). También puedes importar el archivo del modelo a mano.
2. Opcional: activa allí **Tarjeta gráfica (GPU)** para un procesamiento mucho más rápido. Si tu tarjeta no puede, Radiant usa el procesador sin avisar.

Después, clic derecho en una pista → **Crear stems**, o usa la pestaña **Stems** del reproductor. Los resultados se guardan en `Stems/<Artista - Título>/` dentro de tu carpeta de música y se añaden a una lista de la carpeta «Stems».

## Versión móvil

Radiant también está disponible para Android, con música local, reproducción sin conexión, ecualizador, separación de stems con IA y bloqueo de la app: **[Radiant para Android](https://github.com/john3sobieski/radiant-audio-player)**.

## Lo que viene

Radiant se desarrolla activamente. Planes, sin ningún orden concreto:

- más ajustes y personalización;
- más optimización (procesador, memoria, tiempo de inicio);
- mejoras en la interfaz;
- más idiomas de interfaz;
- teclas multimedia de Windows y controles de reproducción del sistema;
- descarga opcional y unidireccional de tu música desde el almacenamiento en la nube (Dropbox);
- quizá descargar pistas directamente desde otros servicios;
- compatibilidad con una llave NFC para desbloquear la app;
- una versión para Linux;
- un instalador y más.

Las ideas y los informes de errores son bienvenidos en [Issues](../../issues).

## Licencia

Radiant se distribuye bajo la **[PolyForm Noncommercial License 1.0.0](LICENSE.md)**: gratis para uso personal y otros usos no comerciales. No está permitido venderlo ni publicarlo con ánimo de lucro.

### Componentes de terceros

[Slint](https://slint.dev) (interfaz), [symphonia](https://github.com/pdeljanov/Symphonia) (decodificación, MPL-2.0), libopus (Opus), [cpal](https://github.com/RustAudio/cpal) (salida de audio), [rubato](https://github.com/HEnquist/rubato) (remuestreo), [rtrb](https://github.com/mgeier/rtrb), [ort](https://github.com/pykeio/ort) con ONNX Runtime y DirectML (Microsoft, MIT), [rfd](https://github.com/PolyMeilex/rfd), [ureq](https://github.com/algesten/ureq), [image](https://github.com/image-rs/image) y el modelo **Demucs (htdemucs)** de Meta AI (MIT). Cada uno tiene su propia licencia.

<a href="https://slint.dev"><img src="git_images/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="140"></a>

*Radiant está hecho por john3sobieski.*

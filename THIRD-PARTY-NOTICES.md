# Third-party notices

BuzzPrint's own code, art (Boberto, icon) and documentation are MIT licensed (see `LICENSE`).
The following components keep their own licenses:

| Component | Where | License |
|---|---|---|
| Inter font | `src/BuzzPrint/Assets/Fonts/Inter-*.ttf` | SIL Open Font License 1.1 (`Inter-LICENSE.txt`) |
| JetBrains Mono font | `src/BuzzPrint/Assets/Fonts/JetBrainsMono-Regular.ttf` | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) |
| Microsoft.Web.WebView2 | NuGet package, restored at build time (not in this repository) | Microsoft license (BSD-style), see the package |
| FFmpeg | Not in this repository. `build.ps1` copies the locally installed `ffmpeg.exe` next to the app; the Microsoft Store package (`-Msix`) bundles the unmodified gyan.dev *essentials* build in `tools\` with `FFmpeg-LICENSE.txt` and `FFmpeg-SOURCE.txt` (where to get the source) | GPL v3 (gyan.dev builds), see ffmpeg.org |

# Palmo Translate

A Rust desktop GUI for an existing PLaMo 3 Translate server. Uses GTK4 and libadwaita through gtk-rs, like [GNOME Podcasts](https://gitlab.gnome.org/World/podcasts), plus Tokio and reqwest for asynchronous networking. No model is bundled, downloaded, or run locally.

## Use

Run `cargo run --release`. Open **Settings** using the gear icon in the window header to change **API server URL** or **Maximum output tokens**. The default server is `http://localhost:9931`; enter `http://fedora.local:9931` for the LAN server (a root URL or a URL ending in `/v1` is accepted). Choose the source language or automatic detection, choose a target, enter text, then click **Translate** or press **Ctrl+Enter** (**Command+Enter** on macOS).

- All 53 model-supported languages, including Persian and both Chinese variants.
- Searchable language selectors; swap explicit source/target languages and reuse the translation.
- Selectable output and copy-to-clipboard button.
- Live streaming translation, cancellation that retains partial output, visible network/server errors, and an explicit incomplete-output warning when the output token limit is reached. Partial output remains selectable and copyable during streaming or after cancellation/failure.
- Equal-width language selectors with a centered swap button. Side-by-side editors on wide windows; vertically stacked on narrow ones.
- Server and output-token settings are saved when Settings closes; languages are also saved when a translation starts. Settings use GLib's platform-specific user configuration directory under `palmo-translate/settings.json`. Source and translation text are not saved.

The server receives text only when Translate is activated. HTTP is unencrypted: use it on a trusted LAN, or configure HTTPS with a trusted certificate. There is no cloud service or telemetry. Cancel drops the client request; the server may continue an already-running generation. This client targets a single-model llama.cpp server without API-key authentication.

## Release downloads

Publishing a GitHub Release (including a prerelease) triggers `.github/workflows/release.yml`. It checks out the release tag, tests and compiles natively, then attaches these archives and `SHA256SUMS` after **all** builds succeed:

| Archive | Platform |
| --- | --- |
| `palmo-translate-linux-x86_64.tar.gz` | Linux x86-64, built on Ubuntu 26.04 |
| `palmo-translate-windows-x86_64.zip` | Windows x86-64, MSYS2 UCRT64 / GNU |
| `palmo-translate-macos-aarch64.tar.gz` | macOS 15+, Apple Silicon |
| `palmo-translate-macos-x86_64.tar.gz` | macOS 15+, Intel |

These are **dynamically linked binaries, not standalone installers**. Each archive includes this README; Linux also includes the desktop launcher. Install the native runtime first, extract the matching archive, and run `./palmo-translate` from its directory:

- **Linux:** targets Ubuntu 26.04 or a distribution with compatible glibc, GTK, and libadwaita runtimes. Older Ubuntu releases are not a supported binary target; build from source there instead. On Ubuntu 26.04: `sudo apt install libgtk-4-1 libadwaita-1-0`. On a sufficiently recent Fedora: `sudo dnf install gtk4 libadwaita`.
- **macOS:** `brew install gtk4 libadwaita`, using native Homebrew for your CPU architecture. Binaries are not Developer ID signed or notarized; macOS security controls may block downloaded executables.
- **Windows:** install MSYS2, then run `pacman -S --needed mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-libadwaita` in **UCRT64**. Run `./palmo-translate.exe` from that terminal so its runtime DLLs and resources are available.

Rust is not needed to run release binaries. On Linux, verify a download with `sha256sum --ignore-missing -c SHA256SUMS`; on macOS use `shasum -a 256 --ignore-missing -c SHA256SUMS`. On Windows use PowerShell `Get-FileHash <archive.zip> -Algorithm SHA256` and compare with the matching entry.

Push the workflow and packaging script **before creating the release tag**, then publish a release for that tag. Draft releases and tag pushes alone do not trigger builds. The workflow uses the built-in `GITHUB_TOKEN`; no extra repository secrets are required. Failed builds leave the published release without new assets; after fixing the cause, rerun the failed workflow as appropriate. Rerunning successful builds replaces assets with the same names.

## Build prerequisites

Rust **1.92+** (edition 2024), a C linker, pkg-config, GTK **4.12+**, and libadwaita **1.5+**. `Cargo.lock` is committed for reproducible dependency resolution. Uses gtk4 **0.11.5**, libadwaita **0.9.2**, and reqwest **0.13.5** with Rustls and platform certificate verification. Stable gtk-rs releases are used rather than Podcasts' development alpha versions; media playback dependencies are not needed.

### Linux

Fedora:

```sh
sudo dnf install cargo rust gcc pkgconf-pkg-config gtk4-devel libadwaita-devel
cargo build --release --locked
./target/release/palmo-translate
```

Ubuntu 24.04 or newer (install current Rust using rustup):

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
cargo build --release --locked
```

Optional per-user desktop installation:

```sh
install -Dm755 target/release/palmo-translate "$HOME/.local/bin/palmo-translate"
install -Dm644 data/io.github.amiralimollaei.PalmoTranslate.desktop "$HOME/.local/share/applications/io.github.amiralimollaei.PalmoTranslate.desktop"
```

Ensure `~/.local/bin` is in your desktop session's PATH.

### macOS

Install Rust and [Homebrew](https://brew.sh), then:

```sh
brew install pkg-config gtk4 libadwaita
cargo build --release --locked
./target/release/palmo-translate
```

If your shell cannot find Homebrew's pkg-config metadata, add `$(brew --prefix)/lib/pkgconfig` and `$(brew --prefix)/share/pkgconfig` to `PKG_CONFIG_PATH`.

### Windows

Use an [MSYS2](https://www.msys2.org) **UCRT64** terminal and a matching GNU Rust toolchain, not MSVC:

```sh
pacman -S --needed mingw-w64-ucrt-x86_64-toolchain mingw-w64-ucrt-x86_64-rust mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-libadwaita mingw-w64-ucrt-x86_64-nasm
cargo build --release --locked
./target/release/palmo-translate.exe
```

Run inside UCRT64 so GTK DLLs and resources are available. Standalone distribution would need to bundle the GTK/libadwaita DLLs, transitive dependencies, schemas, icons, and resources; copying the executable alone is insufficient. Likewise, a standalone macOS app would need a bundle containing native dependencies. Release archives intentionally use system-installed runtimes instead. The native UI is GNOME-styled on all platforms.

The Rustls AWS-LC cryptography provider requires NASM for the Windows source build; the release workflow installs it too. NASM is not required to run the compiled application.

Linux is the exercised development platform; macOS and Windows require platform verification. `.local` host discovery also depends on the OS/network; use the server's IP address if necessary.

## API and verification

The client follows the model author's [production recommendation](https://huggingface.co/pfnet/plamo-3-translate-8b-GGUF#pfml3-prompt-format): construct PFML3 and POST to `/v1/completions` with `stream: true`, temperature zero, an explicit output limit, and `<|plamo:tag|>` / `<|plamo:bos|>` stop sequences. SSE text deltas are appended immediately; a broken stream is reported instead of treating partial text as a completed translation. It omits `model`, using the already-loaded model. No chat-template configuration is required. Literal PLaMo control tokens in source text are rejected rather than interpreted as prompt structure.

The token limit is for generated output, not input characters. Input and output together must fit the server's configured context window; server context-limit errors are shown rather than silently splitting or truncating text. Requests have a 10-second connection timeout and 180-second overall timeout.

```sh
cargo test --locked
cargo run --locked --example translate -- '会議は来週の月曜日に変更されました。' en http://fedora.local:9931
```

The example contacts the real server. Model licensing and server operation remain separate from this client; consult the model's PLaMo Community License for your use case.

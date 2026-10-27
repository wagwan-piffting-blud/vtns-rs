# VoiceText/NeoSpeech in Rust

## What is this?

A native Rust reimplementation of the VoiceText (NeoSpeech) text-to-speech engine. It turns text into speech with the original voice banks, and its output is bit-identical to what the original engine DLLs produce from the same text.

Supported voices:

| Voice | Language | Engine builds reproduced |
|---|---|---|
| Paul, Kate, Julie, James | English (US) | VoiceText 3.11 (2014), 32-bit `vt_eng.dll` and the 64-bit voice DLLs |
| Violeta | Spanish | 2013 build |
| Paul, Julie (older installs) | English (US) | 2009 builds |
| Paul (oldest install) | English (US) | 2004 build |
| Violeta (older install) | Spanish | 2009 build |

The engine build is picked from the voice data, so an older install works with the same command line.

No voice data or engine code is included. You need your own VoiceText voice install.

## How to use

See the built-in help:

```sh
./vtpaul --help
```

Some examples:

```sh
./vtpaul "Hello world." -o hello.wav
./vtpaul --voice julie --rate 120 -f story.txt -o story.wav
./vtpaul --voice violeta --data "/path/to/Violeta" "Hola a todos."
```

Note the binary is **called** `vtpaul`, but it can be used to generate speech for any of the supported voices. The voice is selected using the `--voice` option (`paul`, `kate`, `julie`, `james` or `violeta`; default `paul`).

The voice data is looked up in this order: the `--data` folder, the `VTPAUL_DATA` environment variable, then the first of these that holds the voice: the folder holding the binary, and the platform's default folders (`vtpaul --help` lists them for your machine):

| Platform | Default folders |
|---|---|
| Windows | `%ProgramFiles(x86)%\NWSPaulNumber2`, `%ProgramFiles%\NWSPaulNumber2` |
| Linux | `$XDG_DATA_HOME/vtpaul` (else `~/.local/share/vtpaul`), `/usr/local/share/vtpaul`, `/usr/share/vtpaul` |
| macOS | `~/Library/Application Support/vtpaul`, `/Library/Application Support/vtpaul` |

The folder must contain `data-<voice>` and `data-common`, as a VoiceText install does; on Linux and macOS, copy those two folders from a Windows install into one of the default folders, or point `--data` at them.

Without `-o` the audio is played. `--format` writes WAV (default), 8-bit WAV, A-law/µ-law WAV, AU, raw PCM, A-law, µ-law or Dialogic ADPCM. Pitch, rate, volume, pauses and emphasis have the same ranges and defaults as the original engine. VTML tags in the text are supported.

## The GUI

`gui/` is a desktop app, VTNS (Tauri, with the same engine inside; its output is byte-identical to the CLI's):

- a text box, Speak / Pause / Stop, and Save to File in any of the CLI's formats;
- a voice picker covering every install it finds, and sliders for rate, pitch, volume, sentence pause, comma pause and emphasis;
- menus that wrap the selection in VTML tags (pauses, breaks, rate/pitch/volume, say-as, alias, part of speech). For the 2004 voice they insert its older `<vt_…>` tags;
- a **Phoneme Builder** for `<vtml_phoneme alphabet="x-cmu">`: click CMU phones, see a respelling, preview, insert. It warns before speaking about any phoneme tag the engine would ignore.

No voices are bundled. The app looks in a folder you pick with **Voices…** (remembered; an install folder holding `data-common`, or a folder of installs up to two levels down), then in `VTPAUL_DATA`, the app's own folder and the platform's default folders above (each may also hold several installs).

| Platform | Installers |
|---|---|
| Windows 10/11 x64 | NSIS setup `.exe`, `.msi` |
| macOS 11+ (universal) | `.dmg`, signed and notarized |
| Linux x64, glibc 2.35+ (Ubuntu 22.04+, Debian 12+) | `.deb`, `.AppImage` |
| Linux arm64 (Raspberry Pi 4/5, 64-bit OS) | `.deb`, `.AppImage` |

## Builds

Prebuilt binaries for Windows (amd64, x86), Linux (amd64, x86, arm64, armv6, armv7) and macOS (one universal binary for Apple silicon and Intel, signed and notarized) are on the releases page. To build from source:

```sh
cargo build --release
```

`build_targets.ps1` builds every target from Windows into `dist/`: Windows and Linux locally (static CRT, static musl via `rust-lld`, no cross toolchain), macOS by running `build_macos.sh` over ssh on the Mac. `-NoMac` skips macOS, `-Only <target>` builds one. `-Check` renders every voice, every engine generation (2014, 2009, 2004), `--x64`, prosody and the encoders through every binary (Linux under WSL and `qemu-user`, both macOS slices on the Mac) and demands each output's SHA256 equal `windows-amd64`'s; it reads the voices from `../all_voices`. `-NoBuild -Check` checks what is already in `dist/`.

`gui/build_targets.ps1` builds every GUI installer into `gui/dist/`: Windows locally, Linux in Docker (x64 on Docker Desktop, arm64 on a Pi over ssh), macOS by running `gui/build_macos.sh` on the Mac over ssh. `-Only <target>` builds one. The GUI cannot be cross-built: each platform links its own webview. For a development build, `cd gui && cargo build`; a debug build also looks for voices in `../all_voices`.

## How it works

[`reveng/README.md`](reveng/README.md) describes the engine stage by stage (text front end, linguistic analysis, prosody, unit selection, waveform generation) and how each stage is verified against the original DLLs.

## License

GPL-3.0-only. See [LICENSE](LICENSE).

## GenAI Disclosure Notice: Portions of this repository have been generated using Generative AI tools (Claude, Claude Code).

# VoiceText/NeoSpeech in Rust

## What is this?

A native Rust reimplementation of the VoiceText (NeoSpeech) text-to-speech engine. It turns text into speech with the original voice banks, and its output is bit-identical to what the original engine DLLs produce from the same text.

Supported voices:

| Voice | Language | Engine builds reproduced |
|---|---|---|
| Paul, Kate, Julie, James | English (US) | VoiceText 3.11 (2014), 32-bit `vt_eng.dll` and the 64-bit voice DLLs |
| Violeta | Spanish | 2013 build |
| Paul, Julie (older installs) | English (US) | 2009 builds |
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

The voice data is looked up in this order: the `--data` folder, the `VTPAUL_DATA` environment variable, the folder holding the binary, then `C:\Program Files (x86)\NWSPaulNumber2`. The folder must contain `data-<voice>` and `data-common`, as a VoiceText install does.

Without `-o` the audio is played. `--format` writes WAV (default), 8-bit WAV, A-law/µ-law WAV, AU, raw PCM, A-law, µ-law or Dialogic ADPCM. Pitch, rate, volume, pauses and emphasis have the same ranges and defaults as the original engine. VTML tags in the text are supported.

## Builds

Prebuilt binaries for Windows, Linux (amd64, arm64, armv6, armv7) and macOS (signed and notarized) are on the releases page. To build from source:

```sh
cargo build --release
```

## How it works

[`reveng/README.md`](reveng/README.md) describes the engine stage by stage (text front end, linguistic analysis, prosody, unit selection, waveform generation) and how each stage is verified against the original DLLs.

## License

GPL-3.0-only. See [LICENSE](LICENSE).

## GenAI Disclosure Notice: Portions of this repository have been generated using Generative AI tools (Claude, Claude Code).

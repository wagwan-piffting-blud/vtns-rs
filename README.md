# VoiceText/NeoSpeech in Rust

## What is this?

A Rust library for interacting with VoiceText/NeoSpeech text-to-speech voice banks. Supported voices are Paul, James, Kate, and Julie. This library allows you to convert text into speech using these voices. **NOTE: Violeta is not currently supported, but support may be added in the future.**

## How to use

See the built in help:

```sh
./vtpaul --help
```

Note the binary is **called** `vtpaul`, but it can be used to generate speech for any of the supported voices. The voice is selected using the `--voice` option. It defaults to paul, and looks in the path specified by `--data` first, then next to the binary, then `C:\Program Files (x86)\NWSPaulNumber2` if either of those first two locations are not found. The `--data` option can be used to specify a custom path to the voice bank data.

## GenAI Disclosure Notice: Portions of this repository have been generated using Generative AI tools (Claude, Claude Code).

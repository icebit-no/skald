# Silero VAD

`silero-vad-v6.2-F32.gguf` is Silero VAD v6.2, the speech detection behind the silence gate
([design.md](../../docs/design.md#silence)), as GGUF for transcribe.cpp. The `skald` binary holds it, and writes
it into the models folder when the file there is missing or not this one, so it is never downloaded
([design.md](../../docs/design.md#first-run), D-23).

| | |
|---|---|
| Model | [snakers4/silero-vad](https://github.com/snakers4/silero-vad), by the Silero Team, ported from commit 5cd7945676eb32225748052e2e6a0580e4686a08 |
| File | [handy-computer/silero-vad-v6.2-gguf](https://huggingface.co/handy-computer/silero-vad-v6.2-gguf) at revision `a9b2d61c20ae4ddf3644edb34969e8185a485448`, unchanged |
| URL | `https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/a9b2d61c20ae4ddf3644edb34969e8185a485448/silero-vad-v6.2-F32.gguf` |
| Size | 1 241 056 bytes |
| SHA-256 | `0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6` |
| Licence | MIT, Copyright (c) 2020-present Silero Team: [`LICENSE-silero-vad`](LICENSE-silero-vad), the upstream `LICENSE` at the commit above. The GGUF repository is MIT as well. |

It is the file [engine-check.md](../../docs/research/engine-check.md) measured. A test in `crates/skald` checks
the file the binary holds against the size and SHA-256 above.

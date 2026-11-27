# Research

What was found before building, checked on 2026-11-27. Each section ends with what it means for Skald.

## Models

| Model | What it is | Word errors per 100 words | In Skald |
|---|---|---|---|
| [NB-Whisper Large](https://huggingface.co/NbAiLab/nb-whisper-large) | OpenAI's Whisper large-v3 trained further by the National Library on Norwegian speech (NRK, Stortinget, NST, audiobooks), 2024. 1.5 B parameters; ggml 3.1 GB, q5_0 1.1 GB | 2.2 on NST, 6.6 on FLEURS | Default |
| [NB-Whisper Medium](https://huggingface.co/NbAiLab/nb-whisper-medium) / [Small](https://huggingface.co/NbAiLab/nb-whisper-small) / [Base](https://huggingface.co/NbAiLab/nb-whisper-base) | Smaller sizes of the same | 7.2 / 8.3 / 11.5 on FLEURS | For machines without a GPU |
| [NB-ASR Bokfink02](https://huggingface.co/NbAiLab/nb-asr-qwen06b-bokfink02-beta) | The National Library's newer model, October 2026: Qwen3-ASR 0.6 B with its own tokenizer. Bokmål (readable or verbatim), nynorsk, English. Keyword context. Trained with noise (phone, radio, Teams) and with noise-only audio that has empty targets | Not measured on the same benchmark. An earlier checkpoint: 2.6 on NST studio, 6.3 on NST from a distance | Choice, beta, after M1a |
| [Nordlyd](https://huggingface.co/NbAiLab/nordlyd-asr-qwen06B-nov26preview) | Next in the same family, 18 November 2026. Bokmål, nynorsk, English; tagged code-switching. Gated with manual approval; the model card is not public | Unknown | When open |
| [Whisper large-v3](https://huggingface.co/openai/whisper-large-v3) | OpenAI's multilingual original | 6.8 on NST, 10.4 on FLEURS | No |
| [Whisper small](https://huggingface.co/openai/whisper-small) | OpenAI | 29.6 on FLEURS | No |
| [BuzzASR norwegian](https://huggingface.co/BuzzASR/norwegian) | Another Whisper large-v3 fine-tune, MIT, 2026 | 8.4 on FLEURS (its own measurement) | No |
| Apple dictation | Built into macOS and iOS. `nb-NO` runs on the device; there is no nynorsk recogniser; no user vocabulary. Apple's SpeechTranscriber (2025) lists 30 locales and no Norwegian (queried on macOS 26) | Not published | No |
| Omarchy (Voxtype) | Omarchy's dictation; downloads a small English Whisper model (about 150 MB) by default and can point at another ggml file | Weak on Norwegian | No |

The NB-Whisper and OpenAI figures on FLEURS and NST come from the National Library's paper
([arXiv 2402.01917](https://arxiv.org/abs/2402.01917)), measured the same way for every model. Bokfink's
figures come from its model card, on its own test sets. NVIDIA's Parakeet and Canary, Alibaba's plain
Qwen3-ASR, Cohere Transcribe and Voxtral do not support Norwegian.

Bokfink is published for Python: `qwen-asr`, Transformers or vLLM, with CUDA, which means an NVIDIA GPU and
several gigabytes of tools, and nothing for the Mac's GPU. Skald converts it once to GGUF and runs it in
transcribe.cpp like NB-Whisper; Python is needed only for the conversion and for checking it against the
National Library's script (M1a).

**For Skald:** NB-Whisper Large by default; Bokfink after M1a, marked beta; the engine behind one interface so
Nordlyd can follow.

## Licences

- Every current NB-Whisper model, Bokfink and Nordlyd are Apache 2.0. The licence puts no condition on the
  licence of a program that uses the model, so Skald is MIT. The model cards ask for attribution; Norwegian
  copyright law requires it for downloads made in Norway.
- Downloading at first run from Hugging Face means Skald does not redistribute the model. Credit it in the
  About tab and on the site. A model we convert and host ourselves (Bokfink as GGUF) carries the licence text,
  the attribution and a note that it was converted.
- whisper.cpp, OpenAI Whisper and transcribe.cpp are MIT; transcribe-rs and Handy are MIT; whisper-rs is
  Unlicense. Their notices go in `THIRD-PARTY-NOTICES`.
- VoiceInk and FluidVoice (GPL-3) and Whispering, Voquill and VoiceTypr (AGPL-3) can be read but not copied
  from.

## Existing tools

| App | Platforms | Licence, price | Runs NB-Whisper | Vocabulary, corrections | Right ⌘ alone |
|---|---|---|---|---|---|
| [Handy](https://github.com/cjpais/Handy) | macOS, Windows, Linux | MIT, free | Yes, a custom model file | Words as a prompt; exact replacement only through a hook script ([PR 1533](https://github.com/cjpais/Handy/pull/1533) open since June) | No, either ⌘ |
| [OpenWhispr](https://github.com/OpenWhispr/openwhispr) | macOS, Windows, Linux | MIT, paid cloud tier | No, fixed list | Yes | Yes |
| [Voxtype](https://github.com/peteonrails/voxtype) | Linux, macOS | MIT, free | Yes, a model path | Yes | Any key |
| [FluidVoice](https://github.com/altic-dev/FluidVoice) | macOS (others in beta) | GPL-3 with a closed runtime | No | Yes | Yes |
| [VoiceInk](https://github.com/Beingpax/VoiceInk) | macOS | GPL-3, paid binary | Yes | Yes | Unknown |
| [VoiceTypr](https://github.com/ideaplexa/voicetypr) | macOS, Windows | AGPL-3, paid after 3 days | No | Yes | Yes |
| Spokenly, superwhisper, MacWhisper, Wispr Flow | various | Closed or subscription | No | Partly | Partly |

No app met every requirement (free, macOS and Windows, NB-Whisper, exact corrections, right ⌘ alone). Handy
came closest; a user reported in [Handy issue 2014](https://github.com/cjpais/Handy/issues/2014) that NB-Whisper
stopped it from reading Norwegian as Danish or Swedish.

**For Skald:** Handy's issue tracker is a list of problems to avoid: a paste racing the clipboard
([502](https://github.com/cjpais/Handy/issues/502)), the modifier still held when the paste is sent
([2051](https://github.com/cjpais/Handy/issues/2051)), Metal hanging on Intel Macs with AMD GPUs
([2340](https://github.com/cjpais/Handy/issues/2340)), input freezing when Accessibility is revoked while the
app runs ([1281](https://github.com/cjpais/Handy/issues/1281)).

## Stack

| Part | Choice | Notes |
|---|---|---|
| Window and overlay | GPUI with gpui-kit, as Ferric | PopUp window: non-activating panel on macOS, topmost without activation on Windows, layer shell on Wayland. Click-through is set on the native window; windows without focus are capped near 30 fps until `inactive_frame_interval` is lifted |
| Engine | [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) | Whisper as GGUF, Qwen3-ASR, Silero VAD v6.2; Metal, Vulkan, CUDA; MIT |
| Binding | [transcribe-rs](https://github.com/cjpais/transcribe-rs) or our own | Decided in M1a |
| Hotkey | [handy-keys](https://github.com/handy-computer/handy-keys) | Modifier-only hotkeys, left and right apart, press and release. global-hotkey cannot do a lone modifier; rdev is unmaintained |
| Audio | cpal and rubato | WASAPI, CoreAudio, ALSA/PipeWire; 16 kHz mono |
| Clipboard and paste | arboard, enigo as fallback | Paste after release; elevated windows on Windows refuse synthetic input |
| Tray | macOS status item through objc2 (as Ferric); notification area on Windows; StatusNotifierItem on Linux | |
| macOS permissions | Accessibility (event tap and paste), Microphone | The grant follows the signature: unsigned builds lose it on every update |
| Packaging | Developer ID and notarization; SignPath; tap, bucket, winget, AUR | Homebrew 6 disables casks that fail Gatekeeper in its own repository and asks users to trust third-party taps |

**Risks:** macOS permissions and signing; Vulkan loaded at start on Windows (a machine without a Vulkan driver
must still start and say why); insertion into apps that refuse synthetic input; Bokfink's custom tokenizer and
prompt in transcribe.cpp (M1a).

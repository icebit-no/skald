# Build plan

How Skald behaves is in [design.md](design.md). This file says in which order it is built and what each
milestone contains. How a run works is in [working-method.md](working-method.md).

## M0 Repository and plan

- [x] Public repository `icebit-no/skald`, MIT.
- [x] Design, plan, decisions, research and working method as text in `docs/`.

## M1 Engine check and the core on macOS

Design: [Recording](design.md#recording), [Silence](design.md#silence), [Insertion](design.md#insertion),
[Vocabulary and corrections](design.md#vocabulary-and-corrections). Research: [Models](research.md#models),
[Stack](research.md#stack).

### M1a Engine check

Two pull requests, each with a report in `docs/research/`; they can run side by side. The engine and silence
items in M1b wait until both are merged, because they decide the binding and which models Skald can offer; the
workspace, the core, audio and input do not depend on them. Test clips are fetched
by a script from a pinned source with SHA-256 checks, not committed.

**NB-Whisper and VAD** (`docs/research/engine-check.md`):

- [x] Build transcribe.cpp with Metal. Convert NB-Whisper Large to GGUF with transcribe.cpp's converter and
      check on 10 Norwegian clips that its text matches whisper.cpp with NbAiLab's `ggml-model.bin`.
- [x] Check that the vocabulary prompt works with NB-Whisper in transcribe.cpp: as initial prompt, and carried
      into each 30-second window for a long take.
- [x] Run Silero VAD from transcribe.cpp on the same clips, including silent takes.
- [x] Choose the Rust binding: transcribe-rs, or our own over transcribe.cpp's C API. Say why.
- [x] Measure the time to transcribe 9 s of speech with NB-Whisper on Apple Silicon.

**Bokfink** (`docs/research/bokfink-gguf.md`):

- [x] Convert NB-ASR Bokfink02 (Qwen3-ASR 0.6B with NbAiLab's own tokenizer) to GGUF. Check that its text
      matches NbAiLab's `inference.py` on 20 clips. Find out whether transcribe.cpp takes the custom tokenizer,
      the language prefixes (`language Norwegian<asr_text>` and `language Norwegian Nynorsk<asr_text>`) and
      keyword context, and what is missing if not.
- [x] Measure the time to transcribe 9 s of speech with Bokfink on Apple Silicon.

### M1b The core

- [x] Cargo workspace: `skald-core` (configuration, vocabulary, corrections, history; no I/O), `skald-engine`
      (transcribe.cpp), `skald-audio` (capture, resampling, levels), `skald-input` (hotkey and paste), and the
      `skald` binary.
- [x] Right ⌘ hold with handy-keys: arm after 200 ms, cancel when another key joins, a watchdog for a missed
      release.
- [x] Capture with cpal, resampled to 16 kHz mono with rubato, level readings for the meter; the microphone
      chosen by the priority list.
- [x] The silence gate: level, VAD (at least 250 ms of speech), known phrases.
- [x] Transcribe with NB-Whisper Large on Metal, loaded once.
- [ ] Beam search 5 (D-16). transcribe.cpp decodes Whisper greedily only; waiting on Thomas
      ([engine-check.md](research/engine-check.md#beam-search-three-ways-and-the-one-recommended)).
- [x] Insert by clipboard and ⌘V after the key is released; put the previous clipboard back unless the user
      chose to copy the text.
- [x] Vocabulary prompt and corrections from a TOML configuration; history of the five latest texts.
- [x] Runs from a terminal and can replace diktafon for daily use.

## M2 The app on macOS

Design: [Overlay](design.md#overlay), [Menu](design.md#menu), [Settings](design.md#settings),
[Command line](design.md#command-line), [Identity](design.md#identity).

One binary, `skald` (D-25). New crates, one area each: `skald-overlay` (the GPUI overlay and its themes),
`skald-tray` (the menu bar item and its menu), `skald-control` (the local socket, its protocol and the command
line's commands), `skald-models` (downloading a model), and `skald-settings` (the settings window, after the
overlay has set GPUI up). The binary joins them.

- [ ] Overlay on GPUI: a PopUp window that takes no focus, lets clicks through, is transparent and draws at
      60 fps; the states recording, carving, inserted, heard nothing, microphone changed and hands-free, in the
      three themes Runepinne (the default), Smidd jern and Kjedelig.
- [ ] Carving paced by the measured seconds of work per second of audio on this machine.
- [ ] Menu bar item with the ᚨ template icon: microphone, latest texts, bokmål or nynorsk, ordskatt,
      settings, quit.
- [ ] Settings window with the tabs Generelt, Ordskatt, Historikk and Om.
- [ ] First run: model download with progress (pinned revision, SHA-256), permissions, a test dictation.
- [ ] `skald` command line over a local socket only the user can open; `--json` on every command; writes the
      configuration file directly when the app is not running.
- [ ] Claude skill in `packaging/skills/skald/`, installed with `skald skill install`; `skald mcp`.
- [x] App icon in granite: 1024, 512, 256, 128 and 64 px from the chosen drawing, own drawings for 32 and
      16 px, and the one-colour ᚨ for the menu bar, with the source they are rendered from.
- [ ] `.app` bundle signed with Developer ID and notarized.

## M3 Measurement

Design: [Audio in](design.md#audio-in).

- [ ] Test set: the same sentences read with a good USB microphone, with the built-in Mac microphone and from
      a distance, with references in bokmål.
- [ ] `skald bench`: word error rate and spacing errors (split or joined compounds), per configuration.
- [ ] Compare greedy and beam 5, level normalisation, an 80 Hz high-pass, RNNoise, cutting the silence around
      the speech or not, and NB-Whisper against Bokfink as BF16 and as Q8_0 (Q5_K_M changed 9 of 48 texts in
      M1a and is left out). Keep what helps; write the numbers into `docs/research/`.
- [ ] Bokfink in the model choice, marked beta.

## M4 Windows

- [ ] Right Ctrl hold; a Vulkan build that runs on NVIDIA, AMD and Intel, and says so plainly on a machine
      without Vulkan.
- [ ] Icon and menu in the notification area; the overlay as a topmost window that takes no focus.
- [ ] Paste with Ctrl+V; windows running as administrator cannot receive it, and Skald says so.
- [ ] Zip and manifest in `icebit-no/scoop-bucket`, a winget manifest, signing through SignPath Foundation.
- [ ] Tested on a real Windows machine with an NVIDIA RTX GPU.

## M5 Linux (Omarchy and Hyprland)

- [ ] Hold-to-talk through the GlobalShortcuts portal, or Hyprland `bind`/`bindr` calling `skald start` and
      `skald stop`.
- [ ] Overlay as a layer-shell surface, insertion with wtype, icon through StatusNotifierItem (Waybar).
- [ ] Vulkan build; AUR package `skald-bin`.

## M6 Release and site

Design: [Site](design.md#site).

- [ ] Release pipeline after Ferric's: notarize on the Mac runner, cask to `icebit-no/homebrew-tap`, manifest
      to `icebit-no/scoop-bucket`, AUR.
- [ ] `icebit-no/skald-site`: Astro, an nginx image, app-sync and Caddy on svalbard, at skald.icebit.app.
- [ ] Entry in the icebit.app rack.

## Waiting on Thomas

- The hero of the site: concept A, «Skalden i stamnen», is chosen; it needs final polish and a bokmål version.
- Nordlyd: access to `NbAiLab/nordlyd-asr-qwen06B-nov26preview` must be requested on Hugging Face before it
  can be evaluated.

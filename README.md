# Skald

Norwegian push-to-talk dictation for macOS, Windows and Linux. Hold a key, speak Norwegian, release, and the
text is inserted where your cursor is. Transcription runs on your own machine with the speech models of the
National Library of Norway (Nasjonalbiblioteket). Free and open source: no cloud, no account, no subscription.

> **Status: planning.** There is nothing to install yet. What is built, in which order, is in
> [docs/plan.md](docs/plan.md).

## What it will do

- Hold right ⌘ on macOS, or right Ctrl on Windows and Linux, speak, release.
- Write bokmål or nynorsk, whatever the dialect.
- Use NB-Whisper Large by default, with the National Library's newer NB-ASR Bokfink as a choice.
- Learn your words: a vocabulary sent to the model as context, and corrections for words it hears right but
  writes wrong.
- Never turn silence into text.
- Let an AI agent adjust it: the `skald` command line and a Claude skill (`skald skill install`).

The landing site will be at [skald.icebit.app](https://skald.icebit.app), in nynorsk with bokmål under `/nb`.

## Documents

| File | Holds |
|---|---|
| [docs/design.md](docs/design.md) | How Skald behaves and looks |
| [docs/plan.md](docs/plan.md) | Milestones and what each contains |
| [docs/decisions.md](docs/decisions.md) | What was decided, when and why |
| [docs/research.md](docs/research.md) | Models, licences, existing tools, the stack |
| [docs/working-method.md](docs/working-method.md) | How the work is run |

## Licence

MIT, see [LICENSE](LICENSE). The speech models are made by the National Library of Norway, released under
Apache 2.0, and downloaded on first run; see [docs/research.md](docs/research.md#licences).

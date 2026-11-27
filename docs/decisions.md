# Decisions

The log of what was decided and why, newest first. An entry is written in the pull request that makes the
decision, and that pull request also edits the page where the rule lives ([design.md](design.md),
[plan.md](plan.md)). Those pages say what holds; this one says when and why it came to hold. A later decision
that undoes one adds an entry of its own, and the earlier entry gets one line naming it.

## D-19 — The configuration keeps the keys it does not know

2026-11-27, Claude.

Settings, `skald` and a person with an editor all change one TOML file, and versions of Skald will meet older
and newer files. A missing key takes its default, so an older file loads. A key this version does not know is
read, has no effect, and is written back as it was, at the top, in a section and in each correction, so an
older Skald that saves its settings does not delete those of a newer one; a hold key or model it does not
know is kept as written for the same reason. A key never changes type: a setting that needs another type gets
a new key, so a value of the wrong type is a mistake in the file, and loading fails with the line rather than
putting a default in place of what the user chose. Comments in the file are not kept when it is written. How
often each correction applied is kept on the rule in the same file, next to the words it counts.

## D-18 — Corrections match whole words, ignore case, and the longest match wins

2026-11-27, Claude.

A rule that fired inside another word would break words it was never meant for, so a rule matches only where
its words stand alone; letters, æøå included, and digits belong to a word, and spaces, punctuation and
hyphens end it, so «Post gres SQL-databasen» is corrected. The model capitalises at will and at the start of
every sentence, so case is ignored when matching, and a replacement written in lower case takes a capital
first letter where the text had one; a replacement with capitals, such as «PostgreSQL», is written as given.
The text is read once: where rules overlap, the leftmost match is taken, then the longest, then the earlier
rule, and what a rule writes is never read again, so the result does not depend on rules feeding each other.
Changed: [design.md](design.md#vocabulary-and-corrections).

## D-17 — The app icon is ᚨ cut in light granite

2026-11-27, Thomas.

Four stones were drawn, each with the rune cut as a V-groove and painted red ochre: light granite, dark gneiss,
granite with a cut frame line, and a free-standing standing stone. Light, coarse granite without a frame line was
chosen. The standing stone was liked best, but its outline does not fit among the macOS icons, which share a
rounded square. The frame line was left out; it makes the rune smaller at the small sizes.

The groove's free ends are square at every size, as the rule for free ends says; the chosen drawing had round
ones. From 64 px up the files are the large drawing, which still reads at 64 px when halved down from one large
render. At 32 and 16 px the rune is two to four pixels wide, so those sizes are drawn again with a wider groove,
half the grain at 32 px and none at 16 px. The menu bar rune has a 2 pt stroke, which stays sharp at 1x; 1.5 pt
blurs on one side. Changed: [design.md](design.md#identity), [plan.md](plan.md) M2.

## D-16 — Spacing errors are measured before anything is tuned

2026-11-27, Thomas.

Split and joined words are worst with a distant or poor microphone. diktafon runs whisper-server with
`--beam-size -1`, which decodes greedily, and without VAD. Skald gets beam search 5 from the start; level
normalisation, an 80 Hz high-pass, RNNoise and Bokfink are compared on a recorded test set (`skald bench`)
and kept only where the numbers improve. Changed: [plan.md](plan.md) M3, [design.md](design.md#audio-in).

## D-15 — The site is nynorsk first and shows the models with their error rates

2026-11-27, Thomas.

skald.icebit.app is static Astro, deployed like solveig.icebit.app. Nynorsk at `/`, bokmål under `/nb`. It
names the National Library's models, links to them, and compares word error rates on one benchmark. It shows
the command line and `skald skill install`. The runestone hero was rejected; concept A, a longship whose
skald's voice rises into the northern lights and ends as runes, was chosen.

## D-14 — One Homebrew tap and one Scoop bucket for all icebit apps

2026-11-27, Thomas, on Claude's recommendation.

`icebit-no/homebrew-tap` and `icebit-no/scoop-bucket`, plus winget and the AUR. Homebrew 6 asks the user to
trust each tap, so one shared tap is one question for every icebit app, and there are fewer repositories to
keep. macOS builds are signed with Developer ID and notarized; Windows builds are signed through SignPath
Foundation, which is free for open-source projects.

## D-13 — A Viking-age identity, plain words

2026-11-27, Thomas.

Colours from painted runestones (soot, bone, red ochre) and brooch gold. Display type Eczar, body Alegreya
Sans, runes in Noto Sans Runic. The wordmark has only vertical and diagonal strokes, as runes cut across the
grain of wood. The icon is ᚨ, ansuz, cut into stone and painted red in the groove; an earlier version where
the branches poked out past the stem, and a "blood on stone" variant, were rejected. Viking words appear in a
few places only; buttons and settings say plainly what they do. Changed: [design.md](design.md#identity).

## D-12 — `skald` command line and a Claude skill

2026-11-27, Thomas.

When Skald mishears a word, the user tells their AI assistant, and the assistant fixes it. `skald` talks to
the running app over a local socket only the user can open, the same pattern as Ferric, with `--json` on every
command and `skald mcp` for agents that use MCP. The skill ships with the app and is installed with
`skald skill install`. Changed: [design.md](design.md#command-line).

## D-11 — Silence never becomes text

2026-11-27, Thomas.

Whisper turns silence into «ja», «ja takk» or «den satt». A take passes four checks before anything is
inserted: the level, Silero VAD, the model's no-speech probability and a list of known phrases. Changed:
[design.md](design.md#silence).

## D-10 — Microphones in a priority list

2026-11-27, Thomas.

A microphone that moves between machines on a USB switch comes and goes. Skald uses the first connected
microphone in the user's order, says so in the overlay when it changes, and offers a quick switch in the menu.

## D-9 — Copying to the clipboard is a choice; history keeps five texts

2026-11-27, Thomas. Supersedes diktafon's rule that the text stays on the clipboard.

The text is always inserted at the cursor. One checkbox, «Kopier også teksten til utklippstavlen», off by
default, decides whether it also stays on the clipboard; otherwise the previous clipboard is put back. The five
latest texts are kept on the machine, in the menu and the History tab, with no setting for the number.

## D-8 — Right ⌘ on macOS, right Ctrl on Windows and Linux

2026-11-27, inherited from diktafon. See [CLAUDE.md](../CLAUDE.md) for why.

## D-7 — macOS first, then Windows, then Linux

2026-11-27, Thomas.

macOS replaces diktafon first. Windows targets an NVIDIA RTX machine and any GPU through Vulkan. Linux means
Omarchy and Hyprland first.

## D-6 — Norwegian only

2026-11-27, Thomas.

The output is bokmål or nynorsk, the interface is bokmål, and no other language is added.

## D-5 — NB-Whisper Large by default, Bokfink as a choice

2026-11-27, Thomas.

NB-Whisper Large is the best measured model for Norwegian today. NB-ASR Bokfink02, the National Library's
newer Qwen3-ASR model, is offered as a choice marked beta once M1a shows it runs; Nordlyd follows when it is
open. Models are downloaded on first run from pinned revisions and credited. Changed:
[research.md](research.md#models).

## D-4 — transcribe.cpp as the engine

2026-11-27, Claude, agreed by Thomas.

One engine runs Whisper models as GGUF, Qwen3-ASR (the family Bokfink and Nordlyd belong to) and Silero VAD,
with Metal, Vulkan and CUDA. It is MIT and released often. whisper-rs was the first choice; it pins an older
whisper.cpp and has no safe setter for `carry_initial_prompt`, and it cannot run Qwen3-ASR.

## D-3 — Rust and GPUI

2026-11-27, Thomas.

The overlay must show without taking focus. GPUI's PopUp window is a non-activating panel on macOS and a
topmost window shown without activation on Windows, and Ferric already runs GPUI on all three systems. egui,
iced and Slint take focus on macOS today; Tauri needs a third-party panel plugin, and its transparent windows
turn black on Windows.

## D-2 — Name, licence, repository

2026-11-27, Thomas.

Skald, at skald.icebit.app, MIT, public from the first commit. «Utter» was taken by another dictation app.

## D-1 — Build Skald rather than adopt Handy

2026-11-27, Thomas.

Handy (MIT) runs NB-Whisper as a custom model, but has no exact corrections and cannot bind right ⌘ alone. Skald
is a Norwegian-first product with its own vocabulary workflow, overlay and command line. Handy's issue tracker
is used as a list of problems Skald must avoid. The prototype it replaces, diktafon, stays as it is until
Skald takes over.

# Decisions

The log of what was decided and why, newest first. An entry is written in the pull request that makes the
decision, and that pull request also edits the page where the rule lives ([design.md](design.md),
[plan.md](plan.md)). Those pages say what holds; this one says when and why it came to hold. A later decision
that undoes one adds an entry of its own, and the earlier entry gets one line naming it.

## D-23 — transcribe.cpp as a vendored crate with Skald's patches; 500 ms kept around the speech

2026-11-27, Claude.

Skald builds transcribe.cpp with its two patches (D-22) from a copy of the `transcribe-cpp-sys` crate in
`third_party/`, which the root `Cargo.toml` puts in place of the crates.io crate; the safe `transcribe-cpp`
crate stays from crates.io. The copy is the published crate as it is plus the patches, and a script writes it
from the crate's SHA-256 and the patch files, so what differs from upstream is the patches and nothing else,
and anyone can check that. The other ways each cost more: a patched install elsewhere (`TRANSCRIBE_DIR`) is a
separate build on every machine and in CI; a git submodule needs CI changes; a fork would be a public
repository of Thomas's; applying the patches at build time needs a patch tool on every machine. The price is
about 35 MB of C++ source in the repository (5 MB as the published crate), marked as vendored so pull requests
collapse it. The copy goes when an upstream release carries both changes.

The budget patch lets the prompt and the text before each 30-second window share Whisper's 223 tokens, the
prompt first, as whisper.cpp does: the second of the two fixes engine-check.md names (the first, the prompt
alone at each window's head, was not measured). The 97 s take with the full 213-token ordskatt then has 9.8 %
word errors, against 60.3 % in stock transcribe.cpp and 10.5 % in whisper.cpp, so the meter keeps its 223
tokens.

The silence before and after the speech is cut with 500 ms kept on either side. Cut at Silero's own 30 ms of
padding, the 20 FLEURS clips lost words (5.2 → 5.9 % word errors clean, 20.2 → 23.9 % with noise); with
500 ms they came out about as with the whole take (5.6 % and 19.2 %). A text with no word in it ends a take as
the silence checks do: there is nothing to insert.

Evidence: [research/engine-check.md](research/engine-check.md), the model tests in `crates/skald-engine/tests`,
the margins in `crates/skald-engine/src/vad.rs`. Changed: [plan.md](plan.md) M1b,
[design.md](design.md#silence).

## D-22 — transcribe.cpp's Rust crates, a patched build, NbAiLab's model file, no no-speech check

2026-11-27, Claude; the patched build, the model file and the silence gate settled by the lead in review.

`skald-engine` binds transcribe.cpp through `transcribe-cpp` and `transcribe-cpp-sys`, the crates published from
the transcribe.cpp repository with each release, pinned to the exact version of the library. transcribe-rs, the
first candidate, does not use transcribe.cpp: it wraps whisper.cpp and ONNX Runtime and cannot run Bokfink. A
binding of our own would redo what `transcribe-cpp-sys` already is: FFI generated from the same header and checked
against its ABI hash. The safe crate lacks the Whisper extension's prompt placement and the per-window decoding
trace; Skald uses `transcribe-cpp-sys` for those two until the safe crate has them. The choice holds while
transcribe.cpp is the only engine; a second engine for beam search would reopen it.

Skald builds transcribe.cpp with its own patches until upstream releases them: the budget for a prompt carried
into every 30-second window, without which a full ordskatt breaks long takes, and Bokfink's language names
([bokfink-gguf.md](research/bokfink-gguf.md)). So the ordskatt meter keeps its 223 tokens; the engine caps a
carried prompt at 111 tokens only while the budget patch is not in the build.

First run downloads NbAiLab's own `ggml-model.bin` at a pinned revision with its SHA-256. transcribe.cpp loads it
directly, it gives the reference text, and the National Library hosts it, so Skald hosts no model file.

The silence gate drops the model's no-speech probability, D-11's third check. It never fires: NB-Whisper's is
0.000 on every silent take, and Bokfink has none. Level, Silero VAD and known phrases carry the gate. A model
whose probability works may add the check back.

Evidence: [research/engine-check.md](research/engine-check.md). Changed: [research.md](research.md#stack),
[design.md](design.md#silence) and [First run](design.md#first-run), [plan.md](plan.md) M1b.

## D-21 — Clipboard and paste through NSPasteboard and CoreGraphics on macOS

2026-11-27, Claude.

The previous clipboard is put back as it was, every item with every type it held: rich text, images, files and
types private to an app. arboard handles plain text, HTML and images, so Skald reads and writes NSPasteboard
items itself through objc2, which handy-keys already brings in. The text that is on the clipboard only for the
paste is marked transient, so clipboard history tools skip it. ⌘V is two CoreGraphics key events carrying the
Command flag, which needs no dependency beyond those; enigo is not used.

A mouse click while right ⌘ is held cancels the take, as a key does: ⌘-click is a shortcut.

Restoring the clipboard means reading it, and once macOS enforces pasteboard privacy it may ask the user once
whether Skald may. That is acceptable; if it is denied, the text stays on the clipboard, and first run says so
(M2).

Contents marked `org.nspasteboard.ConcealedType` or `org.nspasteboard.TransientType` are neither read nor put
back; the dictated text stays on the clipboard instead. A password manager clears a secret only while the
clipboard still holds what it put there, and a restore would put the secret back beyond that clearing. Restored
contents carry no marker of their own, or the next dictation would refuse to put back what this one restored.
Changed: [research.md](research.md#stack), [design.md](design.md#recording) (Recording and Insertion).

## D-20 — Microphone fallback, matching by name, a lost microphone and the menu pin

2026-11-27, Claude.

D-10 left four cases open. When no microphone in the order is connected, Skald uses the system's default
input, then the first connected microphone, so a take still records; while any microphone in the order is
connected, the order wins over the default. A device can get a new id when it is plugged into another port, so
an entry whose id is gone matches a connected microphone of the same name that no other entry holds by id. A
microphone that disappears during a take ends the take with what was recorded, and the next take uses the next
microphone; switching in the middle would put two microphones in one take.

Choosing a microphone in the menu pins it: the app puts it first in the order it uses until «Bruk den første
som er koblet til», and while it is unplugged the rest of the order applies. The pin is a setting, so
`skald mic use` sets the same pin. Changed: [design.md](design.md#microphones).

## D-19 — The configuration keeps the keys it does not know

2026-11-27, Claude.

Settings, `skald` and a person with an editor all change one TOML file, and versions of Skald will meet older
and newer files. A missing key takes its default, so an older file loads. A key this version does not know is
read, has no effect, and is written back as it was, at the top, in a section and in each correction, so an
older Skald that saves its settings does not delete those of a newer one; a hold key or model it does not
know is kept as written for the same reason. A key never changes type: a setting that needs another type gets
a new key, so a value of the wrong type is a mistake in the file, and loading fails with the line rather than
putting a default in place of what the user chose. Comments in the file are not kept when it is written.

The file is written only when a setting changes, never after a take: the command line and a person's editor
share it, and a write per take would race both. How often each correction applied is state, kept next to the
history and keyed by the rule's words, whatever their case and spacing, so a rule given a new replacement
keeps its count.

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

The no-speech check was dropped in D-22.

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

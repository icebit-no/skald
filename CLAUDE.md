# Skald

Norwegian-only push-to-talk dictation: a desktop app for macOS, Windows and Linux in Rust on GPUI, transcribing
locally with the National Library of Norway's models through transcribe.cpp. How it behaves and looks is in
`docs/design.md`, in which order it is built in `docs/plan.md`, why in `docs/decisions.md`, the background in
`docs/research.md`, and how the work is run in `docs/working-method.md`. Read your milestone in `docs/plan.md`
before starting.

## Rules

- Code, comments, docs and commit messages are in English. Everything a user reads in the app is Norwegian
  bokmål. The site is nynorsk first, with bokmål under `/nb`.
- Norwegian only. No other interface languages; the one language choice is bokmål or nynorsk output.
- Nothing leaves the machine: no telemetry, no cloud. The network is used for two things only: downloading a
  model (pinned revision, SHA-256 checked) and checking for a new release.
- Only free, open-source dependencies, pinned to exact versions. A pull request that adds one says how much of
  its API we use and what it pulls in.
- Never copy code from GPL or AGPL projects (VoiceInk, FluidVoice, Whispering, Voquill, VoiceTypr). Handy and
  transcribe.cpp are MIT and may be borrowed from, with credit.
- Credit the National Library of Norway for every model it made: in the About tab, on the site and in
  `THIRD-PARTY-NOTICES`.
- Anything a user can set in the settings window, an agent can set through `skald`; both change the same
  configuration.
- User-facing words: the text is «satt inn» where the cursor is; never «innliming». Viking words appear only
  where docs/design.md puts them (rister, ordskatt, Sagaen); buttons and settings say plainly what they do.
- Work on a branch `thomastvedt/<slug>` created from local `main` (no upstream on `origin/main`), one pull
  request per plan item. Tick its checklist in `docs/plan.md` in the same pull request; a decision gets an
  entry in `docs/decisions.md` in the pull request that makes it.

## Settled, inherited from the diktafon prototype

- **Right ⌘ on macOS.** F9 is a media key, MacBooks have no right Ctrl, and right Option is needed for
  `[ ] { } | \ @` on Norwegian layouts. **Right Ctrl on Windows and Linux**: right Alt is AltGr there and
  arrives as Ctrl+Alt.
- **Arm after 200 ms; any other key cancels.** A take starts once the key has been held 200 ms, and another key
  pressed meanwhile cancels it, so ⌘C and ⌘V work as before. A watchdog ends a take whose release was missed.
- **Insert by clipboard and paste**, never synthetic typing: typing is slow and unreliable with æøå. Paste
  only after the key is released. Whether the text stays on the clipboard is the user's choice (D-9).
- **The vocabulary prompt opens with a sentence** that shows inflected forms («merget», «pull-requesten»); a
  list of bare words does not. Whisper keeps the last 223 tokens of a prompt, so the most important names go
  last, and the prompt is carried into every 30-second window.
- **Corrections are added from evidence**: when a word breaks in a real dictation, not when it might.
- **The model stays loaded while the app runs.** Loading it per phrase measured 2.2 s against 1.1 s for 9 s of
  speech on an M4 Pro.
- **Silence never becomes text** (docs/design.md, Silence).
- **Strokes that meet another stroke end butt on its centre line** in the wordmark, the icon and every rune
  drawn; nothing pokes out on the other side. Free ends are square.

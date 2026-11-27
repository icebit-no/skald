# Decisions

The log of what was decided and why, newest first. An entry is written in the pull request that makes the
decision, and that pull request also edits the page where the rule lives ([design.md](design.md),
[plan.md](plan.md)). Those pages say what holds; this one says when and why it came to hold. A later decision
that undoes one adds an entry of its own, and the earlier entry gets one line naming it.

## D-44 — On Linux the overlay is a layer-shell surface, and the app's screen runs there

2026-11-27, Claude; the layer-shell surface before the tray agreed with the lead.

`skald` on Linux runs the screen as on macOS (D-36): the overlay, the first-run window while the model is
missing, and the settings window, which the menu item reaches once StatusNotifierItem comes; until then the
settings are changed with `skald config`, and `--terminal` keeps the loop alone. On Wayland the overlay is a
layer-shell surface on the overlay layer, anchored to the bottom of the output and 32 pt above the area other
surfaces leave free, with no keyboard and an empty input region, so it takes no focus and every click goes
through. The compositor puts it on the output with the focus, so the window names no display. Hyprland and
the other wlroots compositors offer the protocol; GPUI has the surface and the input region, so nothing is
added. On X11 the overlay stays GPUI's pop-up, which is not tried. The accent colour and reduced motion are
not read on Linux, so Kjedelig draws in its own colours, and its blur comes on Hyprland from a layer rule.

Windows and Linux name the keys in the pill in words, «slipp Ctrl» and «Ctrl+Win+X stopper» or
«Ctrl+Super+X stopper», where macOS draws ⌘ and ⌃. The words are wider, so the oldest recording bars give way
to the hint rather than run under it.

Evidence: the overlay on a headless sway with software Vulkan, photographed recording, hands-free, after a
take that heard nothing, and in Kjedelig. Changed: [design.md](design.md#overlay), [plan.md](plan.md) M5.

## D-42 — On Wayland the clipboard goes through data-control in Skald itself, and Ctrl+V through wtype

2026-11-27, Claude; data-control or wl-clipboard, and wtype, offered by the lead's brief for M5.

Skald reads and sets the clipboard on Linux through the wlroots data-control protocol, which Hyprland, the
other wlroots compositors and KDE Plasma offer, rather than through wl-copy and wl-paste. wl-copy offers one
type per copy, so contents put back through it would lose their other types: the HTML beside a browser's
text, the file manager's own type beside a list of files. Through the protocol Skald puts every type back,
as D-21 does on macOS, and counts the selections the compositor announces, which stands in for macOS's
change count, so a copy made while the text is on the clipboard is left in place. wayland-client and the
protocol's crate are in the tree through GPUI at the same versions, so nothing is added; GNOME, which has
neither data-control nor a virtual keyboard for programs, is not served.

Wayland keeps no copy of what is copied: whoever set the selection is asked for it at each paste. A thread
of Skald's keeps the connection and answers for the text and for the contents it put back, so those stay on
the clipboard while Skald runs and go when it quits, unless a clipboard manager keeps them. The text on the
clipboard only for the paste is offered with `x-kde-passwordManagerHint`, the mark of a secret that
clipboard managers there skip, which stands in for macOS's transient type; contents a password manager
marked that way are neither read nor put back, as D-21 has it. A program that takes more than two seconds
per type to hand over its contents, or offers more than 256 MB, leaves the text on the clipboard.

Ctrl+V is sent by wtype, from a virtual keyboard with its own keymap, so it reads as Ctrl+V whatever the
layout. A Wayland program cannot see which keys are down, so the paste is sent as soon as the text is on the
clipboard; the hold key is up by then, since its release ended the take. The insertion is tried in CI on a
headless sway, with wl-clipboard and wev to look at the clipboard and the window in front. Changed:
[design.md](design.md#insertion).

## D-41 — Ctrl+Win+X on Windows; the overlay names an administrator window; no UI Access yet

2026-11-27, the lead.

The hands-free chord on Windows is Ctrl+Win+X, the counterpart of ⌃⌘X: the Windows key stands where ⌘ does,
as right Ctrl stands for right ⌘, and Ctrl+Alt is AltGr. handy-keys keeps the chord from the app in front, as
on macOS, and keeps the Windows key's release from opening the Start menu.

When the window in front runs as administrator (D-38), the overlay says «Kan ikke sette inn i et
administratorvindu · ligger i Siste tekster» for 2 s, as the other messages stand, rather than the general
«Ikke satt inn»: the user can act on it.

Skald does not ask for UI Access, which would let it hear the key in an administrator window and paste into
it: Windows grants it only to a signed binary installed under Program Files, which belongs to M4's packaging
item. Until then Skald says plainly that it cannot insert there. Changed: [design.md](design.md#recording)
(Recording, Overlay, Insertion).

## D-40 — The command line talks to Skald over a named pipe on Windows that only the user can open

2026-11-27, Claude.

On Windows, `skald` and the running Skald talk the protocol of D-28 over a named pipe,
`\\.\pipe\skald-` and a hash of the socket's path in Skald's folders. The pipe namespace is shared by
everyone on the machine, and the path holds the user's own folder, so each user's Skald has a pipe of its own,
and so does a Skald given a folder of its own, as the tests are. The hash ignores case, as Windows paths do.

The pipe's access list names the user alone, with nothing inherited, which is what the socket's 0700 folder and
0600 mode are on Unix. The pipe takes no connections from other machines. Pipe names are first come, first
served, so two more rules keep another program from standing in for Skald: the first instance is created
with the flag that fails when the name is already held, which Skald reports as another Skald running, and the
command line asks which process answers and goes on only when it runs as the same user. A pipe goes away with
its last handle, so a Skald that crashed leaves nothing to clear.

Skald keeps one instance listening at every moment, the next made before the connected one is handed to its
thread, so a command run while Skald answers another never finds the pipe missing and writes the files behind
the running Skald's back. Reads and writes are overlapped, so the command line's waits (30 s, 600 s for a
transcription) and the server's 60 s idle limit hold as they do on the socket. windows-sys, already in the
tree, is the only crate used; the interprocess crate would have brought its own async layers for one pipe.

## D-39 — On Linux the desktop's key bindings hold the key: `skald start`, `skald stop`, `skald toggle`

2026-11-27, Claude; the binding and the portal as the two ways in set by the lead's brief for M5, the
hands-free chord by the lead.

A Wayland compositor gives the keyboard to no program, so Skald on Linux cannot listen for right Ctrl as it
does on macOS and Windows. The desktop holds the key instead: a key binding runs `skald start` as the key goes down and
`skald stop` as it comes up, and each reaches the running Skald over its socket (D-28). On Hyprland the two
lines are `bindn` and `bindrn`; the `n` lets right Ctrl through to the app in front, so right Ctrl+C still
copies. A desktop with the GlobalShortcuts portal can report the key there instead: Skald registers the
shortcut `hold` under `app.icebit.skald` (D-34), and the portal reports the key going down and coming up.
ashpd, which GPUI already takes at the same version, speaks the portal; only its shortcuts feature is new.

Both feed one listener, which applies the hold rules a key heard from the keyboard gets (skald-input's
`HoldMachine`): a take arms after 200 ms and ends at the release, and a press or release reported twice
counts once. What a binding cannot tell is lost. Another key pressed with right Ctrl does not cancel the
take, so a shortcut held past 200 ms records, and the silence gate keeps it from becoming text unless the
user speaks; a release the desktop never reports leaves the take running until the 120 s cap. Reading the
keyboard from `/dev/input`, as handy-keys can on Linux and Voxtype does, would keep every rule, but only for
a user in the `input` group, which gives each of their programs every keystroke, passwords included; that is
the choice Wayland keeps from programs, and it is left to Thomas.

The hands-free chord on Linux is Ctrl+Super+X, the counterpart of ⌃⌘X, set by the lead. A binding reports a
press and nothing else, so it runs `skald toggle`, which presses the chord once in the same listener; a plain
`bind` keeps the chord from the app in front, as ⌃⌘X is kept on macOS (D-36). The portal registers it as a
second shortcut, `hands-free`.

The commands are answered only by a running Skald: without one they fail with the new error code
`not_running`, and on macOS and Windows, where Skald listens to the keyboard itself and its watchdog reads
the key's real state, with `invalid`. They are no MCP tools, since an agent has no reason to hold the key. Until Linux has a screen
(the tray and the overlay, later in M5), `skald` there runs the loop in the terminal, as `--terminal` does,
and text is not inserted: each take is kept in the history and printed. Changed:
[design.md](design.md#recording) (Recording, Command line), [plan.md](plan.md) M5.
Insertion on Linux: D-42. The screen on Linux: D-44.


## D-38 — Right Ctrl, Ctrl+V and the clipboard on Windows; a window run as administrator is checked first

2026-11-27, Claude.

skald-input listens for right Ctrl on Windows through handy-keys' low-level keyboard hook and reads the keys'
state with GetAsyncKeyState. AltGr arrives as left Ctrl with right Alt; handy-keys drops that left Ctrl, so
AltGr never reads as right Ctrl, and during a hold it is another key, which cancels the take. The hook needs
no permission, so the listener never reports one missing. Windows has no hands-free chord yet: design.md
leaves its key to M4, and Ctrl+Alt is AltGr.

Ctrl+V is four SendInput events in one call, left Ctrl down, V down and up, left Ctrl up, sent once every
modifier is up. Apps read a shortcut by its virtual key, so it is Ctrl+V on every layout, which ⌘V on macOS
is not. windows-sys, already in the tree through skald-models, is the only crate used; arboard and enigo are
not.

The clipboard is saved with every format it holds: a format that is a block of memory as its bytes, and the
enhanced metafile as its bits. Handles only their owner understands (owner display, the private and GDI
ranges) are left out, and so are the formats Windows makes again from one that is kept (a bitmap and its
palette from the DIB, a metafile picture from the enhanced metafile). A clipboard holding only formats left
out counts as unreadable, so the text stays on it. D-21's rule for secrets holds with Windows' markers:
contents carrying ExcludeClipboardContentFromMonitorProcessing, or CanIncludeInClipboardHistory set to 0, as
password managers place them, are neither read nor put back. The text there only for the paste carries both,
and CanUploadToCloudClipboard set to 0, so clipboard history and the cloud clipboard skip it; restored
contents carry no marker. The clipboard's sequence number tells a copy made after the paste, as macOS's
change count does.

Windows drops input sent to a window of a higher integrity level, such as an app run as administrator, and
SendInput does not say so (User Interface Privilege Isolation). So once the keys are up, the inserter compares
the integrity level of the window in front with its own; when the window's is higher it pastes nothing,
leaves the clipboard alone and reports why, so the take ends as «Ikke satt inn» with its text in «Siste
tekster», and the terminal says the window runs as administrator. A level that cannot be read counts as not
higher, and the paste is tried. The same rule keeps the hook from seeing keys pressed in such a window, so a
take cannot start there; the check catches the focus moving to one during a take. Running Skald with UI
Access would lift both, but needs a signed binary under Program Files, which is for M4's packaging.
Changed: [research.md](research.md#stack).

The hands-free chord on Windows, the overlay's words for an administrator window and UI Access: D-41.

## D-37 — `skald bench record` takes the test set while Enter is held; 60 sentences written for it

2026-11-27, Claude; «from a distance» as the built-in Mac microphone at 2 m, and Thomas looking through the
sentences before he reads them, settled by the lead in review.

The test set's 60 sentences were written for Skald (`research/test-set/`, MIT) to provoke what M3 measures:
compounds split or joined, hyphens, borrowed words with Norwegian endings, numbers, dates and names. Each
sentence is its own reference, and none uses a form bokmål allows two ways, so a word counted wrong is wrong.

`skald bench record <folder> --microphone <label>` shows them one at a time and records each take through
skald-audio, as Skald records one, into a folder the bench reads: `<label>-<nn>.wav`, 16 kHz mono 16-bit, the
sentence in `<label>-<nn>.txt`, and one manifest line per microphone (`"usb-*" = "usb"`). The labels are the
manifest's three, `usb`, `built-in` and `distance`, and no others, so a typo cannot split the tables. The folder
is the only state: a sentence is recorded once its take and reference are both there, and the take is moved
into place last. A new sitting starts after the last take and offers the skipped sentences at the end, and a
redo replaces the last take. A take under 0.5 s is a slip of the key and is not kept; one below the silence
gate's speech level is kept with a warning, since the bench would count it as silence.

A terminal reports a key going down, and again while it repeats, but not coming up. Holding Enter therefore
needs the kitty keyboard protocol, which Ghostty, and so Herdr, speaks; in other terminals, macOS's Terminal
among them, a take runs from one press of Enter to the next. crossterm 0.29.0 gives raw mode, the protocol and
the key events on all three systems. Without its default features it adds five crates: crossterm,
crossterm_winapi (Windows only), mio, signal-hook and signal-hook-mio.

«From a distance» is the Mac's built-in microphone about 2 m away, in a normal voice, against about 50 cm for
the built-in microphone and 15 to 25 cm for the USB microphone. Thomas looks through the sentences before he
reads them. Evidence: the tests in
`crates/skald-bench/src/record`. Changed: [plan.md](plan.md) M3 and Waiting on Thomas.

## D-36 — The app runs the dictation loop beside GPUI; ⌃⌘X is kept from other apps

2026-11-27, Claude; Silero VAD inside the binary decided in #18's review, the bench and settings wiring asked
by the lead, the three messages and keeping ⌃⌘X from other apps settled by the lead in review.

`skald` with no command runs GPUI on the main thread and #18's dictation loop on a thread of its own, unchanged
in its rules. The loop sends what happens (a take starting, its levels, its end, the microphones, the settings,
the history) as events over a channel to tasks on the main thread, which drive the overlay, the menu bar item
and the settings window. A change from the menu or the window goes through the same running Skald the socket
answers for, so the file is written only when it changes (D-19) and the loop takes it in within a tenth of a
second; a word corrected in Historikk goes to the pipeline's thread, which keeps the history and writes it.
Moving the loop into GPUI's tasks would have meant making its blocking wait for keys asynchronous for no gain.
`--terminal` runs the loop on the main thread without the screen, as #18 did, for debugging.

When the configured model is not in place and checked, or the microphone or Accessibility is not granted,
the app opens the first-run window (D-35) before the loop. It runs the window's download, asks for each
permission with the system's own prompt (AVFoundation's request for the microphone, and for Accessibility
the prompt that also lists Skald in System Settings), starts the loop when the test asks for it, and shows
the menu bar item once the window is done. A loop the test started that stops, as when the model will not
load, says why in the window, which can start it again; closing the window before Skald can take dictation
quits the app. The insertion runs on the pipeline's thread, as every take's does, so the window reads the
test's paste on the main thread while the insertion waits to put the clipboard back. A model given with
`--model` is the developer's, with no first run, and `--terminal` stops at what is missing as #18 did.

The engine must be dropped before the process exits (D-23), so «Avslutt Skald» asks the loop to stop and the
app quits once the loop has dropped the model; when the system ends the app, as at log out, the main thread
waits for the loop. The overlay shows a take's end only while that take is the one on screen, so a take
started while the last one is carved is not replaced by the last one's text. A take that ends without its
text says why, as Hørte ingenting does and for as long: «Ikke satt inn · ligger i Siste tekster» when the
paste was refused, «Transkriberingen feilet» when the model failed, and «Ingen mikrofon er koblet til» when
a take arms without one. The text not inserted is kept in the history, so «Siste tekster» has it.

The microphones connected are listed every three seconds for the menu and the settings window: the system
tells Skald only when the microphone in use changes, and the menu bar item builds its menu from the model it
was last given, with no word to the app as it opens. The ordskatt meter asks the loaded model for each text
once and keeps the count; a count asked while a take is being transcribed waits for that take.

⌃⌘X is caught by skald-input's state machine. While a hands-free take runs the hold key starts nothing and
other keys cancel nothing; a chord pressed during a hold spoils the hold first, as any other key does. The
chord is handed to handy-keys as a blocking hotkey, so it reaches no other app, where it would at best beep.
A hands-free take that ends at the 120 s cap or with its microphone is reported back to the listener, so the
next chord starts a new take rather than stopping one that has ended.

Start at login registers Skald.app as its own login item through SMAppService (macOS 13 and later), called
through the Objective-C runtime: four calls did not warrant a crate. It works only from an app bundle, so a
`skald` run elsewhere says so and leaves login items alone. The setting is applied at start and whenever it
changes; when System Settings must allow the item, Skald says where.

Silero VAD is held in the binary and written into the models folder when the file there is missing or holds
other bytes, compared byte for byte; a test checks the held file against the pinned SHA-256
(`assets/models/README.md`). Changed: [design.md](design.md#recording) (Recording, Overlay),
[plan.md](plan.md) M2.

## D-35 — The first-run window is a view over the app's download and test, and reads the permissions itself

2026-11-27, Claude; the test's text field, «Hopp over», skald-ui and the permission checks as functions settled
by the lead in review.

`skald-firstrun` draws the three steps of First run as the settings window draws its tabs (D-33): gpui-kit's
headless button, progress bar and text input, the same colours in light and dark mode, the system's UI font.
The colours, a text field's colours in them and gpui-kit's setup are in a small crate of their own,
`skald-ui`, which both windows take, so they cannot drift apart and gpui-kit is set up once whichever window
opens first. The overlay has its themes (D-24) and takes neither.

The window is a view over a model the app hands it, and each request goes back as an action. It does no file
or network work: the app runs the download through skald-models (D-26), asks the system for the permissions
and runs the test take. The model takes skald-models' status, events and errors as they are, so the app passes
them on, and the window turns each error into words the user can act on. The permissions the window reads
itself, through two functions the app passes in, when it opens and each time it comes to the front, as it does
when the user comes back from System Settings; the app can also tell it to read them, as when a prompt was
answered while the window kept the front. macOS says whether the microphone was refused or never asked for,
but not for Accessibility, so Accessibility counts as not asked until the window has asked for it once and
as refused after that, which turns «Gi tilgang» into «Åpne Systeminnstillinger».

The speed is what arrived over the last five seconds, shown once a second has passed, so a stall shows soon
and a burst does not swing the time left. A retry starts the measure over, since the wait before it says
nothing about the connection. A file of the pinned size that was never checked offers «Kontroller filen», and
a stopped download goes on from what it kept with «Fortsett».

The user moves between the steps freely, and on while the model downloads, so the permissions are granted in
the minutes a 3.1 GB file takes. The test take is inserted as every take is, into a text field in the window
that has the focus while the test listens, so the key, the microphone, the model and the insertion are all
tried, and macOS asks its clipboard question at first run rather than in the middle of work. Each take starts
with the field empty. When the text did not reach the field, as when the focus moved, the window shows what
Skald heard and says so. gpui-kit's text field takes ⌘V. The insertion waits on its own thread to put the
clipboard back while the window reads the paste on the main thread, so the app runs it on another thread.
The test may be skipped with «Hopp over», which becomes «Ferdig» once a take was heard; either waits until
Skald can take dictation. Changed: [design.md](design.md#first-run), [plan.md](plan.md) M2.

## D-34 — Skald.app is `app.icebit.skald`, for Apple Silicon, with the microphone entitlement alone

2026-11-27, the lead (Apple Silicon only, the microphone reason), Claude (the rest).

The bundle identifier is `app.icebit.skald`, the site's domain reversed. macOS files the microphone and
Accessibility grants under the identifier and the signing team, so neither changes once a release is out: a new
identifier or team would make every user grant both again. `LSUIElement` keeps Skald out of the Dock, as a menu
bar app. The bundle's development region is `nb`, so the system's own words inside the app are bokmål. The
microphone question gives the reason design.md, First run, quotes.

Every build, ad hoc ones too, is signed with the hardened runtime and the one entitlement the runtime would
otherwise withhold that Skald uses: audio input. Accessibility is a grant the user gives, not an entitlement;
Metal, GPUI and the event tap need none. Releases are signed with the Developer ID in the login keychain,
notarized with an App Store Connect API key and stapled (packaging/macos/README.md). The `.icns` is packed from
`assets/icon/` on every build rather than kept in the repository, so the icon has one source.

Skald ships for Apple Silicon only (arm64). An Intel Mac would run NB-Whisper Large slowly, and Metal hangs on
Intel Macs with AMD GPUs in Handy (research.md, Existing tools). M6 may reopen this. Changed:
[design.md](design.md#first-run) (the microphone question), [research.md](research.md#stack) (Packaging),
[plan.md](plan.md) M2.

## D-33 — The settings window draws gpui-kit's headless controls and sends each change to the app

2026-11-27, Claude; the light colours, the type, what «Bytt» keeps and the settings' names settled by the lead
in review.

`skald-settings` takes GPUI through gpui-kit at the overlay's revision and draws gpui-kit's headless
controls (`gpui_kit::base`: the text input, switch, checkbox and button) in Skald's colours. They come with
gpui-kit whatever its features, so the crate adds no dependency. gpui-kit's styled component library is left
out: turned on for one crate it is built into the binary for all of them, with a file watcher, a translation
crate and an icon set, and its own words, such as the text field's menu, are English and Chinese.

The window is a view over a model the app hands it, and each change goes back as an action. A change to the
configuration is applied by the function the tests run, and the app writes the file (D-19); the window shows
the change at once and takes the app's next model as the truth. Every change has a `skald` command that makes
the same change. Downloading a model is not a setting and has none yet. «Kopier» and the links in Om are the
window's own.

Dark mode gives the identity's colours their roles. The identity names no light ground, so light mode turns
bone and soot around, keeps gold for buttons and what is chosen, and writes links in soot and warnings in the
darker ochre, since gold does not read as text on light ground. Text is the system's UI font, at 12 to 13 pt
among the system's own windows. A model can be chosen once it is downloaded and checked, as with
`skald model use`. On macOS the hold key is right ⌘ or right Control; elsewhere only right Ctrl is offered.
The engine's credits in Om (transcribe.cpp, ggml, Silero VAD) are kept in `skald-settings` beside the
models' credits from `skald-models`.

In Historikk, a click picks a word and a shift-click takes in the words up to it, so a phrase the model split,
as «Post gres SQL», can be corrected. «Bytt» replaces the words in the history entry, which the app keeps
with `History::replace`, so «Kopier» and «Siste tekster» give the corrected text; it also sends a correction
and a word as the two boxes ask. «Rett dette automatisk neste gang» starts ticked, since a word that broke in
a real dictation is the evidence a correction needs, and «Legg … i ordskatten» unticked. Changed:
[design.md](design.md#settings), [plan.md](plan.md) M2.

## D-32 — `skald bench` reads spacing errors from the word alignment; RNNoise through nnnoiseless

2026-11-27, Claude; hyphen differences as a count of their own, nnnoiseless as a plain dependency and the
command in bokmål settled by the lead in review.

`skald bench <folder>` runs a test set through configurations and counts word errors, and spacing errors apart
from them. The reference and the text go through one normalisation (`crates/skald-bench/src/normalise.rs`):
lower case, punctuation removed, a hyphen, full stop or apostrophe inside a word kept, digit groups and decimal
marks written one way, and numbers not turned into words. Word errors are the usual count. A split (one
reference word against two or more words of the text that join to it, hyphens ignored) and a join (the other
way round) are steps of that same alignment, each costing the substitution and insertions or deletions it
stands for, so they never change the word error count; of the alignments with the fewest errors, the one with
the most splits and joins is taken. Read from the alignment, every spacing error is counted in the word error
rate exactly once, and the rate without spacing errors is the rest. A word that differs only in its hyphens
(«nord-østersjøen», «nordøstersjøen») is a hyphen difference: a substitution, counted in the word error rate
and in the rate without spacing errors, and also counted on its own beside splits and joins; where the
alignment could put the substitution elsewhere at the same cost, it takes the hyphen difference. On the 20
FLEURS clips, before
it split letters from the number they follow, the bench gave what engine-check.md measured with its own
helper: 5.6 % word errors with the silence cut and 5.2 % without, over 426 words. Splitting them makes
FLEURS's «2.4GHz» the same words as the model's «2,4 GHz», and the two rates 4.9 % and 4.4 % over 429 words.

A configuration is a list of words (`nb-whisper-large,greedy,trim,highpass`). Skald's own is the default, and
without `--config` the bench runs it and then each setting changed on its own. The level check reads the take
as recorded, as the meter does; the filters run before VAD, in the order high-pass, RNNoise, level
normalisation. Beam search and Bokfink are named and report that they are not available until the engine has
them. The command speaks bokmål and fails as the other commands do (D-28).

The high-pass (80 Hz, second-order Butterworth) and level normalisation (one gain per take, the loudest 60 ms
reading to −14 dBFS, no sample above −1 dBFS) are in `skald-audio`, where the app would take them from. RNNoise
comes from nnnoiseless 0.5.2, a Rust port of Xiph's RNNoise with its original weights (BSD-3-Clause; its last
functional release was in December 2022, a test fix followed in December 2025). RNNoise works at 48 kHz, so
the bench converts the take there and back. It stays in `skald-bench` until M3 shows it helps; if it does not,
the option and the crate go. It adds five crates: nnnoiseless, easyfft, array-init, generic_singleton and
anymap3. It is a plain dependency rather than one behind a cargo feature, so CI always builds and lints what
ships. Once the binary takes the command, nnnoiseless ships in `skald`, and its BSD notice belongs in
THIRD-PARTY-NOTICES.

Evidence: the tests in `crates/skald-bench/src` and `crates/skald-audio/src/filter.rs`, and the smoke run in
the pull request. Changed: [plan.md](plan.md) M3.

## D-31 — `skald mcp` written by hand on MCP 2025-11-25, and the skill installed file by file

2026-11-27, Claude.

`skald mcp` speaks the Model Context Protocol over standard input and output through a JSON-RPC layer of its
own on serde_json, which skald-control uses already, rather than the rmcp SDK. It needs four methods and the
notifications it ignores, and the commands it calls are synchronous; rmcp would bring tokio, schemars and
their trees, and macros that describe each tool a second time beside the command it calls. Each tool is a
command, named as its request on the socket (`fix_add`), taking the request's fields as its arguments, and
carried out by the code that carries out the command line's command, through the running Skald or on the
files, so it answers with the object `--json` prints. Arguments are checked against the tool's own list
first: a misspelt optional argument is refused rather than read as left out, since `mic_use` without `name`
removes the pin. A tool that fails answers with the command's error and `isError`, so the agent reads it; only
a call to a tool that does not exist is an error of the protocol.

It speaks 2025-11-25, the newest revision that opens with `initialize`, and answers a client that asks for
2025-06-18, 2025-03-26 or 2024-11-05 in that one, since they differ in nothing these tools use; Ferric's server
answers the same way through rmcp. 2026-07-28 replaces the handshake with metadata on every request, which a
local server started for one client gains nothing from; a client that speaks only that revision is not served
until it is added.

The skill is SKILL.md and a reference of every command, held in the binary, so the skill installed is the one
written for the commands beside it. `skald skill install` writes them into the skills folder of Claude Code's
configuration, `~/.claude` or `CLAUDE_CONFIG_DIR` when it is set, or the folder named with `--dir`. As
Ferric's install does, it writes and removes Skald's files one by one, so a folder that holds anything else
keeps it, and it refuses a folder whose SKILL.md is another skill's. Changed: [design.md](design.md#command-line),
[plan.md](plan.md) M2.

## D-30 — The overlay window opens per appearance; the runes show once the text has arrived

2026-11-27, Claude; the runes, how long the two messages stand and the bars under reduced motion settled by the
lead in review.

`skald-overlay` opens the overlay's window when the pill appears and closes it once the pill has faded out,
on the screen with the keyboard focus each time (AppKit's main screen), centred 32 pt above the Dock. GPUI
cannot move a window, and a closed window draws nothing between takes. The window is a GPUI PopUp, a
non-activating panel that never takes focus. GPUI caps a window without focus at about 30 frames a second
through `inactive_frame_interval`, and the pill never has focus, so the overlay turns the cap off; the window
draws at the display's rate while it shows. GPUI has no option for click-through or for leaving out AppKit's
window shadow, so both are set on the native window. GPUI's blurred background, which Kjedelig uses, covers the
whole window as a rectangle, the shadow's room included; the overlay sizes the blur to the pill and masks it to
the capsule.

The chisel's pace is the median of the latest eight takes' seconds of work, from the release to the text, per
second of audio, starting from 0.12 (NB-Whisper Large on an M4 Pro, M1a). It is kept while the app runs. The
bars run linear in decibels from −60 dBFS, below the room tone skald-core measured, to −15 dBFS. A letter with
an accent is carved as the letter without it.

The text does not exist while the chisel moves, so the runes cannot stand behind it from the start as the
drawings show. Until the text arrives, the bars the chisel has passed are drawn as fresh cuts; when it arrives,
the runes of the carved stretch appear, the chisel runs out in 150 ms, the row of at most 40 runes, the start
of the text, stands 0.4 s, and Satt inn follows. Hørte ingenting stands 2 s and Mikrofon byttet 2.5 s; a
microphone change during a take waits until the take is done. With reduced motion the recording bars are a
still picture of the last 2.4 s, replaced once a second with the time, so the pill still shows that the
microphone hears. Changed: [design.md](design.md#overlay), [plan.md](plan.md) M2.

## D-29 — The microphone opens at the press; one Skald at a time

2026-11-27, the lead (the press, the permission check, the sounds), Thomas (the indicator), Claude (the rest).

The microphone opens when the hold key goes down rather than when the take arms 200 ms later, so the first
words are kept. skald-input reports the press, and every press ends in either armed or not armed, so a
microphone opened for a shortcut such as right ⌘C is closed again and its audio thrown away. The price is that
macOS shows its microphone indicator for a moment at each right-⌘ shortcut, and a Bluetooth headset in use as
the microphone drops to call quality for that moment; design.md already suggests putting headsets last. Thomas
accepted the indicator to keep the first 200 ms.

The start and end sounds are diktafon's: the system sound Tink when a take arms and Pop as its text is
inserted, honouring the sounds setting. Each plays in an `afplay` process of its own, because diktafon found
that a sound played on the main thread can stall it for ten seconds when an output will not start.

macOS gives a program it refused the microphone digital silence rather than an error, so skald-audio asks
AVFoundation whether access is granted, which never prompts. Without Accessibility or the microphone, Skald
says which and where in System Settings to grant it, and stops.

The terminal loop reads the model files where D-26 puts them, or where `--model` and `--vad` say.

A lock beside the state file lets one Skald at a time take dictation, so a second one started by mistake says
so and stops instead of pasting every take twice.

The configuration is read again when the file changes. A file that cannot be read keeps the settings in use
and says why, as does a file that disappears, since an editor may remove it for a moment while saving; a new
model is used from the next start, since loading one takes seconds. While Skald takes dictation it answers the
command line on its socket (D-28); a change made there is written to the file and taken in the same way within
a tenth of a second. A socket that cannot be made leaves Skald taking dictation, and the commands work on the
files. Changed: [design.md](design.md#recording),
[plan.md](plan.md) M1b.

## D-28 — The command line's protocol, and the same commands whether Skald runs or not

2026-11-27, Claude; skald-control depending on skald-core alone agreed with the lead.

`skald` and the running Skald talk JSON lines over the socket, one request and one answer, each with the
protocol version, so a Skald updated while it ran tells the command line to start it again instead of
misreading it. The socket's folder is made 0700 before the socket is created, and the socket 0600; the folder
keeps others out in the moment between. A socket nobody answers on is what a crash leaves, and binding
replaces it; anything else at that path is left alone. Windows gets a named pipe in M4.

Every command is carried out by the same code on top of one trait, which the app implements and the command
line implements on the files while Skald is not running, so the answer and its `--json` object are the same
either way, with `running` saying which. While Skald is not running, a change is written under a lock on the
configuration's folder, so commands an agent runs at once do not lose each other's change, and the file is
replaced in one step. What the files cannot say (the microphones connected, the models downloaded, a
transcription) comes from a second trait the binary implements, so skald-control depends on skald-core alone.
The ordskatt's token count needs the model's tokenizer, so it is shown only while Skald runs.

`skald config` takes the configuration file's keys and values, and true or false also as ja, nei, på or av.
It sets only values this version knows, though the file keeps the others (D-19), and it points settings with
a command of their own to that command. `skald vocab sentence` sets the ordskatt's sentence, which the
settings window can change too, and `skald mic use --first` removes the pin. `skald model use` refuses a model
that is not downloaded. `skald transcribe` prints the text Skald would insert, with the model's text before
corrections and the rules that applied. A command exits with 0 when done, 1 when it failed and 2 for wrong
arguments; with `--json`, an error is `{"error": {"code", "message"}}` on standard output. Changed:
[design.md](design.md#command-line) (Command line, Microphones), [plan.md](plan.md) M2.

## D-27 — The menu ticks the microphone in use and pins the pinned one

2026-11-27, Claude.

design.md named the «Mikrofon» submenu's entries but not how it shows the pin. The tick marks the microphone
a take uses now, which is what the menu is opened to check; the system's pin symbol marks the microphone
chosen in the menu, and «Bruk den første som er koblet til» is ticked while nothing is pinned. A pinned
microphone that is unplugged stays listed, greyed with its pin, so the user sees why the order applies and
can remove the pin.

On macOS the heading «Skald · bokmål» is AppKit's section header from macOS 14 on: a disabled line is greyed
whatever colour its title is given, and only the hint is meant to be greyed. The dot is drawn by a view over
the template icon rather than inside it, so the menu bar still tints the rune for a light or dark bar; it is
the darker of the identity's ochre or gold on a light bar and the lighter on a dark one. A separator stands
before «Avslutt Skald», as in macOS menus; design.md's single line for the last group was a compact list.
Changed: [design.md](design.md#menu).

## D-26 — Models downloaded with ureq over rustls, checked by the system's verifier

2026-11-27, Claude; the folder layout set by the lead.

`skald-models` fetches a model's file with ureq, a blocking HTTP client, over rustls with ring, and has the
system check the server's certificate (rustls-platform-verifier: Security.framework on macOS, CryptoAPI on
Windows, the system's certificate store on Linux), so a certificate the user's system trusts or revokes counts
the same in Skald. A download is one long GET on a thread of its own, with a range when it goes on from a
stopped one. An async client (reqwest) would bring tokio and hyper for that; native-tls would mean OpenSSL to
build and ship on Linux and Windows. ring is in the tree for TLS and also gives the SHA-256, so no hashing
crate is added. Hugging Face redirects the pinned URL to its CDN; the redirect is followed over HTTPS only, and
the range goes with it. Nothing else is contacted.

The models folder holds a folder per model, named by its id, with the file in it as the binary loads it
(`nb-whisper-large/ggml-model.bin`), and Silero VAD's file at its top. A file takes that name only after its
SHA-256 matched, and the hash is then written beside it (`ggml-model.bin.sha256`). So `skald model list` knows a
file is checked without reading 3 GB, a file put there by hand is checked once before it is used, and a pin
moved to a new file is not mistaken for the old one. A stopped download's part carries the start of its hash in
its name, so it is never continued against another file.

A broken connection is tried again up to five times, after 1, 2, 4, 8 and 16 s, and the count starts over
whenever a try brings more of the file; then the download stops and keeps what arrived for the next one. The
free-space check asks macOS for the room it would give a file the user asked for, which counts the purgeable
space that statvfs leaves out.

Evidence: the tests in `crates/skald-models/tests/download.rs`, and the pinned file fetched for a few seconds,
stopped and resumed, with the bytes at the resume point matching the server's. Changed:
[research.md](research.md#stack).

## D-25 — One binary is both the app and its command line

2026-11-27, Claude.

`skald` with no command runs Skald: in M1b the dictation loop in a terminal, from M2 with the menu bar item and
the overlay. With a command (`skald last`, `skald vocab add …`) it is the command line: it talks to the running
Skald over the local socket, or writes the configuration file directly when Skald is not running (design.md,
Command line). The `.app` bundle's executable is the same binary. One binary means one build, one signature and
one version for the app and the tool an agent uses, so they never disagree about the configuration. GPUI joins
the workspace rather than a workspace of its own, because there is no separate check its tree would burden.

## D-24 — The overlay has three themes, Runepinne by default

2026-11-27, Thomas.

Three Viking directions were drawn for the overlay: forged iron, a rune stick of dark oak, and a strip of the
icon's granite. Thomas liked the rune stick best and made it the default, kept forged iron as a choice, and left
out the stone. A plain theme that looks like the rest of the system joins them, because the iron plate looked
more like macOS than Windows, and a dictation overlay sits over everyone's work: on macOS it follows the HUD
style, on Windows 11 its acrylic surface. It is called Kjedelig, which says what it is. The theme applies to the
overlay only. In the rune stick the bars stand 8 pt apart while carving so each holds one rune. Changed:
[design.md](design.md#overlay), [plan.md](plan.md) M2.

## D-23 — transcribe.cpp as a vendored crate with Skald's patches; 500 ms kept around the speech

2026-11-27, Claude; the known-phrase limit, the cut and the VAD model settled by the lead in review.

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

Decided by the lead in review. A known phrase counts as silence only under 350 ms of speech, not 1000 ms: just
above the 250 ms VAD must find before the model runs, and below the shortest real word measured («Hei.»,
380 ms). With 1000 ms a spoken «Ja.» (444 ms) was never inserted, while VAD alone stopped every take without
speech (0 ms). The silence around the speech stays cut at 500 ms, as design.md says, and M3 compares cutting
it or not. Silero VAD ships inside the app (MIT, 1.2 MB) instead of being downloaded on first run. The engine
has to be dropped before the process exits, since the Metal backend aborts at exit while a model is loaded;
that is for the integration item.

Evidence: [research/engine-check.md](research/engine-check.md), the model tests in `crates/skald-engine/tests`,
the margins in `crates/skald-engine/src/vad.rs`. Changed: [plan.md](plan.md) M1b and M3,
[design.md](design.md#silence) and [First run](design.md#first-run), the default of
`known_phrase_max_speech_ms`.

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

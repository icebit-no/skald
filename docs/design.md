# Design

How Skald behaves and looks. Everything a user reads is Norwegian bokmål; the quoted texts below are the app's
own words.

## Recording

- Hold right ⌘ (macOS) or right Ctrl (Windows, Linux). The take arms after 200 ms; another key pressed while it
  is held cancels the take. A mouse click counts as another key, since ⌘-click is a shortcut.
- On Linux a Wayland desktop gives the keyboard to no program, so the desktop's own key binding holds the key:
  it runs `skald start` as right Ctrl goes down and `skald stop` as it comes up, or reports the key through the
  GlobalShortcuts portal. Skald then knows that key alone, so another key does not cancel the take, and a
  release the desktop never reports ends the take at the cap. The binding lines are in
  [packaging/linux](../packaging/linux/README.md).
- The microphone opens when the key goes down, so the words said before the take arms are kept. A press that
  does not arm, as in ⌘C, closes it again and throws its audio away.
- So macOS shows its microphone indicator for a moment at each right-⌘ shortcut; that is accepted for the first words.
- A take ends when the key is released, at the cap, or when its microphone disappears.
- ⌃⌘X starts and stops a hands-free take on macOS, Ctrl+Win+X on Windows, where the Windows key stands for ⌘,
  and Ctrl+Super+X on Linux, where a key binding of the desktop runs `skald toggle`. Ctrl+Alt is AltGr on
  Norwegian layouts.
  While a hands-free take runs, the hold key starts nothing and other keys cancel nothing, so the user can type
  and use shortcuts; the chord itself never reaches the app in front.
- A take is capped at 120 seconds. On macOS the system sound Tink plays when a take arms and Pop as its text is
  inserted; on Linux the freedesktop sound theme's «audio-volume-change» and «complete» play, through pw-play
  or paplay, where they are installed. The sounds can be turned off.

## Overlay

A pill near the bottom of the active screen, about 430 × 58 pt. It never takes focus and lets clicks through. On
Linux under Wayland it is a layer-shell surface, which the compositor puts on the output with the focus. Its
look is a theme, chosen under «Overleggets utseende» in Generelt; the states, their texts and their timing are the
same in every theme. The settings window has no themes.

| Theme | Look |
|---|---|
| Runepinne (standard) | A rune stick of dark oak: the level bars are knife notches, and the runes are cut with a knife behind the chisel |
| Smidd jern | A hammered iron plate with domed rivets; the gold edge is two Urnes-style serpents biting each other's tails |
| Kjedelig | Looks like the rest of the system: on macOS a HUD panel in the system's material and type, on Windows 11 its acrylic surface and Segoe UI, on Linux the desktop's own colours. No ornament and no runes: the chisel line crosses the bars in the system's accent colour |

The colours named below are the two Viking themes'; Kjedelig uses the system's accent and label colours.

| State | What it shows |
|---|---|
| Opptak | A glowing ember-red dot, the elapsed time, the last 2.4 s as 40 bars (3 pt wide, 6 pt apart) scrolling smoothly at 60 fps, and «slipp ⌘» («slipp Ctrl» on Windows and Linux) |
| Rister | On release the bars fold into the whole take; a gold chisel line moves across it, and behind it the bars, now 8 pt apart so each holds one rune, become runes: the dictated text transliterated to the Younger Futhark. «rister …» |
| Satt inn | A gold tick and the start of the text, in plain type, for about 1.5 s |
| Hørte ingenting | «Hørte ingenting · lyttet på <mikrofon>» |
| Ikke satt inn | «Ikke satt inn · ligger i Siste tekster», when the text could not be inserted |
| Administratorvindu | «Kan ikke sette inn i et administratorvindu · ligger i Siste tekster», on Windows, when the window in front runs as administrator |
| Transkriberingen feilet | «Transkriberingen feilet», when the model failed on the take |
| Ingen mikrofon | «Ingen mikrofon er koblet til», when a take arms without one |
| Mikrofon byttet | «<mikrofon> · koblet til, brukes nå», once, when the active microphone changes |
| Håndfritt | A padlock instead of the dot, and «⌃⌘X stopper» («Ctrl+Win+X stopper» on Windows, «Ctrl+Super+X stopper» on Linux) |

The chisel's speed comes from this machine's measured seconds of work per second of audio, kept from recent
takes, so it reaches the end about when the text arrives. Past 96 % it slows down instead of stopping. It never
carves the runes of a text that does not exist yet: until the text arrives, the bars it has passed are fresh
cuts. When the text arrives, the carved stretch shows its runes, the chisel runs to the end in 150 ms, the row
stands 0.4 s, and Satt inn follows.

Hørte ingenting, Ikke satt inn, Administratorvindu, Transkriberingen feilet and Ingen mikrofon stand 2 s, and
Mikrofon byttet 2.5 s.

With reduced motion asked for, the pill shows each state without movement; the recording bars are a still picture
of the last 2.4 s, replaced once a second.

Transliteration to the Younger Futhark: a ᛅ, b/p ᛒ, c/g/k/q ᚴ, d/t ᛏ, e/i/j ᛁ, f ᚠ, h ᚼ, l ᛚ, m ᛘ, n ᚾ,
o/u/v/w/y/ø ᚢ, r ᚱ, s/x/z ᛋ, æ ᛅ, å ᚬ, and ᛫ between words.

## Silence

Whisper turns silence into «ja», «ja takk» or «den satt». A take is transcribed only after these checks, in order:

1. **Level:** the loudest reading never reached speech level, so the model is not run.
2. **Speech detection:** Silero VAD finds less than 250 ms of speech. Silence before and after the speech is cut
   before the model sees the audio.
3. **Known phrases:** the whole text is a known silence phrase and VAD found almost no speech. The list grows
   from real cases.

Any of them ends the take with «Hørte ingenting», and nothing is inserted; so does a text with no word in it,
only punctuation or nothing.

## Insertion

The text is put on the clipboard and pasted with ⌘V (Ctrl+V) after the key is released, with a trailing space so
consecutive takes do not run together. Then the previous clipboard is put back, unless «Kopier også teksten til
utklippstavlen» is ticked. Corrections are applied before insertion.

Putting the previous clipboard back means reading it, and macOS may ask once whether Skald may do that. If the
user says no, the text stays on the clipboard. Contents marked concealed or transient, as password managers
mark a secret, are never read or put back; the text stays on the clipboard instead, so the secret is not
restored where the manager's own clearing can no longer reach it.

On Windows a window run as administrator refuses keystrokes from an app that is not, so Skald pastes nothing
into it, leaves the clipboard alone and says so (Administratorvindu); the text is in «Siste tekster». Windows
does not show Skald the hold key while such a window is in front either, so a take cannot start there.

On Linux the clipboard is read and set through Wayland's data-control protocol, and Ctrl+V comes from a virtual
keyboard (wtype). Wayland keeps no copy of what is copied, so contents Skald put back stay on the clipboard
while Skald runs and go when it quits, unless a clipboard manager keeps them. The text on the clipboard only for
the paste is offered as a secret, the mark clipboard managers skip there, and contents a password manager
marked the same way are neither read nor put back.

## Microphones

The user orders the microphones in Generelt. Skald uses the first one that is connected and falls back to the
next when it disappears. When none of them is connected, it uses the system's default input, then the first
connected microphone. A microphone whose id has changed, as when it is plugged into another port, is
recognised by its name. A microphone that disappears during a take ends the take with what was recorded; the
next take uses the next one.

The menu's «Mikrofon» submenu switches at once by pinning the chosen microphone ahead of the order; while it
is unplugged the order applies, and «Bruk den første som er koblet til» removes the pin. `skald mic use` sets
the same pin, and `skald mic use --first` removes it. Bluetooth headsets drop to call quality while their microphone is open, so the help text
suggests putting them last.

## Vocabulary and corrections

**Ordskatt** is the context sent to the model before each take: a sentence that shows inflected forms, then
words and names. A meter shows tokens used of 223; Whisper keeps the end, so the most important names go last.
With Bokfink the same words are sent as keyword context.

**Rettinger** are exact replacements for words the model hears right but writes wrong: «Post gres SQL» →
«PostgreSQL», «mercha» → «merget». Each rule counts how often it applies, so the rules that work are visible.
A rule matches whole words only and ignores case; a replacement written in lower case gets a capital first
letter where the text had one, so a sentence still starts with a capital. Where two rules match at the same
place, the longer match wins, and a replacement is never corrected again.

## History

The five latest texts, kept on this machine only. They are in the menu under «Siste tekster» (click to copy) and
in the History tab, where a word can be clicked to correct it: «Bytt "mercha" med [merget]», with «Rett dette
automatisk neste gang» and «Legg "merget" i ordskatten».

## Menu

The menu bar item (notification area on Windows, Waybar on Linux) is ᚨ as a one-colour template icon, with a red
dot while recording and a gold dot while transcribing.

- Skald · <bokmål|nynorsk>
- «Hold høyre ⌘ for å snakke» (greyed)
- Mikrofon ▸ · Siste tekster ▸ · Skriftspråk ▸
- Ordskatt … · Innstillinger … ⌘,
- Avslutt Skald ⌘Q

A separator stands after the hint, after «Skriftspråk» and before «Avslutt Skald». On Windows and Linux the hint
reads «Hold høyre Ctrl for å snakke». «Skriftspråk» holds «Bokmål» and «Nynorsk». VoiceOver reads the item as
«Skald», during a take «Skald · Opptak» or «Skald · Rister», and the pin as «Festet».

In «Mikrofon» the microphone in use is ticked and the pinned one carries a pin; a pinned microphone that is
unplugged stays listed, greyed, and «Bruk den første som er koblet til» is ticked while nothing is pinned.
«Siste tekster» is greyed while it holds no text, and a long text is cut with «…» and shown whole as its
tooltip; Windows' menus have no tooltips, so there it shows its start and copies whole.

On Linux the item is a StatusNotifierItem, which Waybar shows: the rune in bone, for the dark panels Hyprland
desktops have, with the same dots. A click on it opens the menu. The pinned microphone is marked «festet» in
words, since the menu there has no pin, and the menu has no shortcuts and no tooltips.

## Settings

| Tab | Holds |
|---|---|
| Generelt | Hold-to-talk key; bokmål or nynorsk; microphone order; talemodell (NB-Whisper Large, NB-ASR Bokfink beta, Nordlyd greyed until open); «Overleggets utseende» (Runepinne, Smidd jern, Kjedelig); «Kopier også teksten til utklippstavlen»; start at login; sounds |
| Ordskatt | The sentence, the words with the token meter, the corrections with their counts, «+ Ny retting», «Prøv på en tekst» |
| Historikk | The five latest texts, «Kopier», correct a word |
| Om | Version, models and engine with their licences, source link, «Ingenting sendes fra maskinen» |

The settings are named «Tasten du holder inne for å snakke» (Høyre ⌘ or Høyre Control on macOS, Høyre Ctrl
elsewhere), «Skriftspråk», «Mikrofoner», «Talemodell», «Overleggets utseende», «Kopier også teksten til
utklippstavlen», «Start Skald ved innlogging» and «Lyd når opptaket starter og slutter»; Ordskatt holds
«Setningen», «Ord og navn» and «Rettinger». The window follows the system's light or dark mode, in its UI font.

## First run

1. Download the model, NbAiLab's own `ggml-model.bin` at a pinned revision, with size, progress and SHA-256.
   Silero VAD (MIT, 1.2 MB) ships inside the app and is credited with the other notices; it is not downloaded.
2. Permissions: Microphone, and on macOS Accessibility «for tasten og innsettingen». macOS asks for the
   microphone with Skald's reason: «Skald tar opp det du sier mens du dikterer, og gjør det om til tekst på
   maskinen. Ingenting sendes fra maskinen.»
3. A test: «Hold høyre ⌘ og si "Hei, dette er en prøve"».

The window, «Kom i gang med Skald», shows the steps as «Talemodell», «Tilganger» and «Prøv» and opens on the
first one not done. The user may go on while the model downloads. The model shows its name, size, licence and
credit, then «Last ned», the progress with speed and time left and «Avbryt», «Fortsett» after a stop, and a
plain reason with «Prøv igjen» after an error. Each permission shows its state, with «Gi tilgang» while the
system can still ask and «Åpne Systeminnstillinger» once it has refused, and is read again when the window
comes back to the front. On macOS the step says that macOS may ask once whether Skald may read the clipboard
(Insertion). The test take is inserted into a field in the window, «Teksten settes inn her»; when it does
not arrive there, the window shows «Skald hørte:» and the text. «Hopp over» leaves the test out and becomes
«Ferdig» once a take was heard; both wait until the model is checked and the permissions are granted.

On Windows the model computes on the GPU through Vulkan, on NVIDIA, AMD and Intel alike. A machine with no
Vulkan driver runs it on the CPU, and Skald says so when the model loads: «Fant ingen skjermkortdriver med
Vulkan, så talemodellen regner på prosessoren og bruker lengre tid.»

## Command line

`skald` talks to the running app over a local socket only the user can open; when the app is not running it
writes the configuration file directly. Every command takes `--json`.

| Command | Does |
|---|---|
| `skald last`, `skald history` | The latest text, or the five latest |
| `skald fix list\|add\|remove` | Corrections |
| `skald vocab list\|add\|remove\|sentence` | Ordskatt, with the token count while Skald runs, and the sentence it opens with |
| `skald config get\|set` | Key, bokmål or nynorsk, the overlay's look, copy to clipboard, start at login, sounds |
| `skald mic list\|use\|order` | Microphones and their order |
| `skald model list\|use` | Models, and which are downloaded |
| `skald transcribe <file.wav>` | Transcribe a file, so an agent can try a correction |
| `skald start`, `skald stop` | Press and release the hold key, for a key binding of the desktop on Linux |
| `skald toggle` | Start or stop a hands-free take, for a key binding of the desktop on Linux |
| `skald bench <folder>` | Run a test set through the configurations and count errors |
| `skald skill install\|uninstall [--dir DIR]` | Install the Claude skill that teaches an agent these commands, or take it out |
| `skald mcp` | The same commands as MCP tools over standard input and output |

An exchange it is made for: «Skald skrev "mercha", det skal være "merget"» → `skald fix add "mercha" "merget"`
and `skald vocab add merget`.

The skill goes into Claude Code's own skills, `~/.claude/skills/skald`, or the folder named; an install replaces
Skald's files and leaves anything else in the folder alone. Each MCP tool is a command named as it is on the
socket (`fix_add`, `mic_use`), takes the same arguments and answers with the same JSON as `--json`.

## Audio in

Spacing errors (compounds split, words joined) grow with a distant or poor microphone. Skald decodes with beam
search 5. Level normalisation, an 80 Hz high-pass, RNNoise and the choice of model are kept only where
`skald bench` shows they help (M3).

## Site

skald.icebit.app, static Astro. Nynorsk at `/`, bokmål under `/nb`, the language switch in the corner.

1. Hero: the longship concept, wordmark, «Du snakkar. Skalden skriv.», the line «Hald inne ein tast, snakk
   norsk, slepp. Teksten står der markøren er. Talemodellar frå Nasjonalbiblioteket, rett på maskina di.
   Gratis og open kjeldekode.», and the download button for the visitor's system.
2. Three points: «Kan norsk», «Held tett», «Lærer orda dine».
3. The models: what NB-Whisper is, word errors per 100 words on FLEURS for each model with links, Bokfink
   without a bar until it is measured the same way.
4. For AI agents: the command line and `skald skill install`.
5. Install commands (brew, winget, AUR) and «Sagaen», the changelog, from the releases.

## Identity

| Token | Value | Use |
|---|---|---|
| soot | `#12100d` | Ground |
| hearth | `#1b1813` | Surfaces |
| iron | `#25211b` | Raised surfaces |
| seam | `#3a3329` | Lines |
| bone | `#ede4d0` | Text |
| ash | `#ab9f88` | Secondary text |
| ochre | `#c2412d` / `#e2664e` | Recording, the rune's paint |
| gold | `#d9a94e` / `#f0c674` | Carving, highlights, buttons |

Type: Eczar for display, Alegreya Sans for text, Noto Sans Runic for runes, IBM Plex Mono for code.

**Wordmark.** SKALD in strokes only, vertical and diagonal, stroke width 11 in a 364 × 100 box, mitre joins.
Strokes that meet another stroke have butt caps and end on its centre line; free ends are square:

```svg
<g fill="none" stroke-linejoin="miter" stroke-miterlimit="6">
  <path stroke-linecap="square" d="M60 6 L12 40 L54 62 L6 94"/>
  <g transform="translate(84 0)"><path stroke-linecap="square" d="M6 4 V96"/><path stroke-linecap="butt" d="M6 56 L59.9 2.1 M26 36 L60.6 100.85"/></g>
  <g transform="translate(166 0)"><path stroke-linecap="square" d="M4 96 L30 16.5 L56 96"/><path stroke-linecap="butt" d="M11.85 72 L42.9 56"/></g>
  <g transform="translate(244 0)"><path stroke-linecap="square" d="M8 4 V94"/><path stroke-linecap="butt" d="M8 92 L48.9 71.5"/></g>
  <g transform="translate(310 0)"><path stroke-linecap="square" d="M6 4 V96"/><path stroke-linecap="butt" d="M6 6 L54 50 L6 94"/></g>
</g>
```

**Icon.** ᚨ, ansuz, the rune of Odin, who gave people the mead of poetry; the Old English rune poem calls it «the
source of all speech». It is cut as a V-groove into light, coarse-grained grey granite and painted red ochre
inside the groove. The stone fills the rounded square that macOS icons share, with no frame line around the rune.
Light falls from the top left, so the shadow in the groove shows that the rune is cut, not painted on. The groove's
free ends are square at every size. The files from 64 px up are the large drawing; the 32 and 16 px files are
drawn again with a wider groove, half the grain at 32 px and none at 16 px. The menu bar icon is the flat rune in
one colour with a 2 pt stroke, without stone.

**Words.** Viking words appear only in «rister …», «Ordskatt», «Sagaen» and the site's tagline. Everything else
says plainly what it does.

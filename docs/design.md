# Design

How Skald behaves and looks. Everything a user reads is Norwegian bokmål; the quoted texts below are the app's
own words.

## Recording

- Hold right ⌘ (macOS) or right Ctrl (Windows, Linux). The take arms after 200 ms; another key pressed while it
  is held cancels the take.
- ⌃⌘X starts and stops a hands-free take on macOS. The key on Windows and Linux is chosen in M4 and M5; Ctrl+Alt is AltGr on Norwegian layouts.
- A take is capped at 120 seconds. Start and end sounds can be turned off.

## Overlay

A pill near the bottom of the active screen, 430 × 58 pt, drawn as an iron plate: near-black fill, a thin gold
edge, a rivet at each end. It never takes focus and lets clicks through.

| State | What it shows |
|---|---|
| Opptak | A glowing ember-red dot, the elapsed time, the last 2.4 s as 40 bars (3 pt wide, 6 pt apart) scrolling smoothly at 60 fps, and «slipp ⌘» |
| Rister | On release the bars fold into the whole take; a gold chisel line moves across it, and behind it the bars become runes: the dictated text transliterated to the Younger Futhark. «rister …» |
| Satt inn | A gold tick and the start of the text, in plain type, for about 1.5 s |
| Hørte ingenting | «Hørte ingenting · lyttet på <mikrofon>» |
| Mikrofon byttet | «<mikrofon> · koblet til, brukes nå», once, when the active microphone changes |
| Håndfritt | A padlock instead of the dot, and «⌃⌘X stopper» |

The chisel's speed comes from this machine's measured seconds of work per second of audio, kept from recent
takes, so it reaches the end about when the text arrives. Past 96 % it slows down instead of stopping. When the
text comes early, it runs to the end in 150 ms. With reduced motion asked for, the pill shows each state without
movement.

Transliteration to the Younger Futhark: a ᛅ, b/p ᛒ, c/g/k/q ᚴ, d/t ᛏ, e/i/j ᛁ, f ᚠ, h ᚼ, l ᛚ, m ᛘ, n ᚾ,
o/u/v/w/y/ø ᚢ, r ᚱ, s/x/z ᛋ, æ ᛅ, å ᚬ, and ᛫ between words.

## Silence

Whisper turns silence into «ja», «ja takk» or «den satt». A take is transcribed only after these checks, in order:

1. **Level:** the loudest reading never reached speech level, so the model is not run.
2. **Speech detection:** Silero VAD finds less than 250 ms of speech. Silence before and after the speech is cut
   before the model sees the audio.
3. **The model's doubt:** the no-speech probability is above its threshold.
4. **Known phrases:** the whole text is a known silence phrase and VAD found almost no speech. The list grows
   from real cases.

Any of them ends the take with «Hørte ingenting», and nothing is inserted.

## Insertion

The text is put on the clipboard and pasted with ⌘V (Ctrl+V) after the key is released, with a trailing space so
consecutive takes do not run together. Then the previous clipboard is put back, unless «Kopier også teksten til
utklippstavlen» is ticked. Corrections are applied before insertion.

## Microphones

The user orders the microphones in Generelt. Skald uses the first one that is connected and falls back to the
next when it disappears. The menu's «Mikrofon» submenu switches at once and has «Bruk den første som er koblet
til». Bluetooth headsets drop to call quality while their microphone is open, so the help text suggests putting
them last.

## Vocabulary and corrections

**Ordskatt** is the context sent to the model before each take: a sentence that shows inflected forms, then
words and names. A meter shows tokens used of 223; Whisper keeps the end, so the most important names go last.
With Bokfink the same words are sent as keyword context.

**Rettinger** are exact replacements for words the model hears right but writes wrong: «Post gres SQL» →
«PostgreSQL», «mercha» → «merget». Each rule counts how often it applies, so the rules that work are visible.

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
- Ordskatt … · Innstillinger … ⌘, · Avslutt Skald ⌘Q

## Settings

| Tab | Holds |
|---|---|
| Generelt | Hold-to-talk key; bokmål or nynorsk; microphone order; talemodell (NB-Whisper Large, NB-ASR Bokfink beta, Nordlyd greyed until open); «Kopier også teksten til utklippstavlen»; start at login; sounds |
| Ordskatt | The sentence, the words with the token meter, the corrections with their counts, «+ Ny retting», «Prøv på en tekst» |
| Historikk | The five latest texts, «Kopier», correct a word |
| Om | Version, models and engine with their licences, source link, «Ingenting sendes fra maskinen» |

## First run

1. Download the model, with size, progress and checksum.
2. Permissions: Microphone, and on macOS Accessibility «for tasten og innsettingen».
3. A test: «Hold høyre ⌘ og si "Hei, dette er en prøve"».

## Command line

`skald` talks to the running app over a local socket only the user can open; when the app is not running it
writes the configuration file directly. Every command takes `--json`.

| Command | Does |
|---|---|
| `skald last`, `skald history` | The latest text, or the five latest |
| `skald fix list\|add\|remove` | Corrections |
| `skald vocab list\|add\|remove` | Ordskatt, with the token count |
| `skald config get\|set` | Key, bokmål or nynorsk, copy to clipboard, sounds |
| `skald mic list\|use\|order` | Microphones and their order |
| `skald model list\|use` | Models, and which are downloaded |
| `skald transcribe <file.wav>` | Transcribe a file, so an agent can try a correction |
| `skald bench <folder>` | Run a test set through the configurations and count errors |
| `skald skill install` | Install the Claude skill that teaches an agent these commands |
| `skald mcp` | The same actions as MCP tools |

An exchange it is made for: «Skald skrev "mercha", det skal være "merget"» → `skald fix add "mercha" "merget"`
and `skald vocab add merget`.

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
source of all speech». It is cut into stone as a V-groove and painted red inside the groove. The 16 and 32 pt
sizes get their own drawings. The final version is waiting on Thomas.

**Words.** Viking words appear only in «rister …», «Ordskatt», «Sagaen» and the site's tagline. Everything else
says plainly what it does.

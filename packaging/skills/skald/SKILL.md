---
name: skald
description: Fixes words that Skald, the user's Norwegian push-to-talk dictation app, writes wrong, by adding a correction (retting) and, where the model needs to hear the word, a word in its ordskatt, through the skald command line. Use when the user says Skald wrote, heard or spelled a word or name wrong (for example «Skald skrev "mercha", det skal være "merget"» or «Skald får aldri med seg navnet Sølvi»), asks to add a word, name or correction to Skald, or asks what Skald wrote last. Also reads and changes Skald's settings, microphones and speech model when the user asks.
---

# Skald

Skald turns speech into text on the user's own machine: they hold a key, speak Norwegian and let go, and
the text is «satt inn» where the cursor is. It transcribes with speech models made by the National Library
of Norway (Nasjonalbiblioteket): NB-Whisper Large by default, and NB-ASR Bokfink as a beta. Nothing leaves
the machine, and nothing this skill does sends anything anywhere.

The model mostly hears right. Where it does not, Skald has two remedies, and this skill applies them:

- **Rettinger** (corrections) are exact replacements made in the text before it is inserted. They are for a
  word the model hears but writes wrong the same way each time: «mercha» → «merget», «Post gres SQL» →
  «PostgreSQL».
- **Ordskatt** is the context sent to the model before each take: a sentence, then words and names. It is for
  a word the model does not know and writes differently each time, usually a name or a term.

## Reaching Skald

Run `skald` in the shell with `--json` on every command, and read the JSON, never the text meant for a person.
Every command you need works whether Skald is running or not. `running` in the answer says which; a change
made while Skald is not running is written to its settings file and applies when it starts.

When Skald's MCP server is registered (`claude mcp add --scope user skald -- skald mcp`), its tools are the
same commands with the same answers: the tool `fix_add` is `skald fix add`, and so on. Use either, not both
for one change. [commands.md](commands.md) lists every command, its tool and its answer.

If the shell does not find `skald`, tell the user that Skald's command line is not installed, and stop.

## Rules

- **Corrections come from evidence.** Add a rule only for a word that broke in a real dictation: one the user
  reports, or one you see in `skald last` or `skald history`. Never for a word that might break, never a list
  of likely mistakes, never the other inflections of a word you have seen broken in one form. A rule made on
  a guess can break text that was right.
- **Look before you change.** Read what Skald wrote (`skald last --json`, `skald history --json`) and the rules
  there are (`skald fix list --json`) before adding one, and use the wrong form exactly as Skald wrote it.
- **Never invent settings.** Change only what a command takes: the keys `skald config get --json` lists, with
  the values an error message names. Never edit Skald's files by hand.
- **Ask first** before removing a correction or a word the user did not name, and before replacing the
  sentence the ordskatt opens with.
- **Answer in the user's language.** The user speaks Norwegian; answer in Norwegian, bokmål unless they write
  nynorsk. Error messages from `skald` are bokmål written for the user: pass them on as they are. Text Skald
  puts where the cursor is was «satt inn»; never call it «limt inn».

## When Skald wrote a word wrong

«Skald skrev "mercha", det skal være "merget".»

1. **Find it.** Run `skald last --json`, and `skald history --json` (the five latest texts, newest first) if
   the word is not in `last.text`. Note the wrong form exactly as it stands, as several words where the model
   split one («Post gres SQL»). If it is in none of them and the user did not quote it, ask what Skald wrote;
   do not guess.
2. **Check the rules.** Run `skald fix list --json`. A rule for the same words takes the new replacement; tell
   the user what it was. If what came out wrong is itself a rule's replacement, that rule is the problem:
   say so, and change it rather than add a second one.
3. **Add the correction.** `skald fix add "mercha" "merget" --json`. Write the replacement in lower case
   unless the word always has capitals («PostgreSQL», «Sølvi»): a lower-case replacement takes a capital where
   the text had one, so a sentence still starts with a capital. A rule matches whole words and ignores case,
   so «mercha» → «merget» leaves «merche» alone; add a rule for that form when it breaks too.
4. **Decide on the ordskatt.** Add the right word when the model needs to hear it: a name, a term, an English
   word used in Norwegian («pull-requesten»), or a word the model wrote differently from take to take. A
   common Norwegian word written wrong needs the correction only. `skald vocab add "merget" --json`; read
   *The ordskatt* below for where it goes and what it costs.
5. **Tell the user** in a line or two what changed: «Skald skriver nå "merget" der modellen skriver "mercha",
   og "merget" ligger sist i ordskatten.» If `running` was false, add «Skald kjører ikke, så endringen gjelder
   når Skald starter.» A correction changes the texts to come, not the one already inserted.

## The ordskatt

`skald vocab list --json` shows `sentence`, `words` and, while Skald runs, `tokens` as `{"used", "limit"}`.

- The model keeps only the end of the ordskatt, its last 223 tokens. So the most important names go last,
  and `skald vocab add` puts a new word last, where it weighs most. A word already there keeps its place; to
  move it last, remove it and add it again.
- When `used` is over `limit`, the start is falling away. Tell the user, and offer to remove words that no
  longer matter (`skald vocab remove "word" --json`) on their yes. Without `tokens`, Skald is not running and
  the count is unknown: say so, and keep additions few.
- Add a word once, in the form the user says most; a name with spaces in quotes:
  `skald vocab add "Ola Nordmann" --json`.
- The sentence shows words in their inflected forms («Vi merget pull-requesten i går.»), which teaches the
  model more than a list of bare words. When a new word inflects, offer a sentence that holds it, and set it
  with `skald vocab sentence "…" --json` only on the user's yes: it replaces the whole sentence.

## Trying a correction on a recording

When the user has the speech as a WAV file, `skald transcribe /full/path/opptak.wav --json` shows what Skald
would insert now: `text` after corrections, `model_text` before them, the rules that applied in `corrections`,
and in `heard_nothing` why it heard no speech, if it did not. Run it after a change to show that the change
works. While Skald is not running, the command loads the model first, which takes several seconds.

## Other things the user may ask

Each is one command; [commands.md](commands.md) has them all.

| The user says | Run |
|---|---|
| «Hva skrev Skald sist?» | `skald last --json` |
| «Fjern rettingen for mercha» | `skald fix remove "mercha" --json` |
| «Skriv nynorsk» | `skald config set language nynorsk --json` |
| «Slå av lydene» | `skald config set sounds false --json` |
| «Bruk USB-mikrofonen» | `skald mic list --json`, then `skald mic use "…" --json` with the name it lists |
| «Hvilken talemodell bruker Skald?» | `skald model list --json` |

## Errors

A command that fails prints `{"error": {"code", "message"}}` and exits with 1, or with 2 for wrong arguments.
`not_found` says what does not exist and, for a microphone or a model, which do; `invalid` says what is wrong
with a value and, for a setting, which values it takes; `version` means Skald was updated while it ran and
must be started again. Pass the message on, and do not retry with guessed values.

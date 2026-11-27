# Skald's commands

Every command takes `--json` and then prints one JSON object with `running`: true when Skald was running and
carried the command out itself, false when the command read or wrote Skald's files directly. Over MCP
(`skald mcp`), each command is a tool named after it, taking the arguments named below and answering with the
same object, as `structuredContent` and as JSON text.

Put words with spaces in quotes. A word that starts with `-` goes after `--`:
`skald fix add -- "-aktig" "aktig" --json`.

## Texts

**`skald last`** · tool `last`. The latest text Skald inserted, after corrections:
`{"running", "last": {"text", "unix_ms", "microphone"}}`, with `last` null before the first take. `unix_ms`
is when the take ended, in milliseconds since 1970-01-01 UTC, or 0 when unknown.

**`skald history`** · tool `history`. The five latest texts, newest first: `{"running", "texts": [...]}`, each
as in `last`.

## Corrections

**`skald fix list`** · tool `fix_list`. `{"running", "corrections": [{"from", "to", "applied"}]}`, in the order
the user keeps them; `applied` counts how often the rule has changed a text.

**`skald fix add <from> <to>`** · tool `fix_add` with `from` and `to`. Writes `to` wherever the model writes
`from`. `{"running", "from", "to", "previous"}`: `previous` is the replacement a rule for the same words had
before, null for a new rule, and equal to `to` when nothing changed.

How a rule matches: whole words only, where letters, æøå included, and digits make a word and spaces,
punctuation and hyphens end it, so «Post gres SQL-databasen» holds «Post gres SQL». Case is ignored, and a
replacement written in lower case takes a capital first letter where the text had one. Where rules overlap,
the leftmost match is taken, then the longest, then the earlier rule, and a replacement is never corrected
again.

**`skald fix remove <from>`** · tool `fix_remove` with `from`. `{"running", "from", "to"}`, the rule removed;
`not_found` when no rule has those words.

## Ordskatt

**`skald vocab list`** · tool `vocab_list`. `{"running", "sentence", "words", "tokens"}`. `words` are in prompt
order, the most important last. `tokens` is `{"used", "limit"}` while Skald runs and null when it does not,
since counting needs the loaded model.

**`skald vocab add <word>`** · tool `vocab_add` with `word`. Adds the word last.
`{"running", "word", "changed", "tokens"}`; `changed` is false when the word was there already, and it then
keeps its place. One word or name at a time.

**`skald vocab remove <word>`** · tool `vocab_remove` with `word`. Takes the word out, written exactly as
`vocab list` shows it. Answers like `vocab add`.

**`skald vocab sentence <sentence>`** · tool `vocab_sentence` with `sentence`. Replaces the sentence the
ordskatt opens with; an empty one removes it. Answers like `vocab list`.

## Settings

**`skald config get [<key>]`** · tool `config_get`, with `key` or without. All settings,
`{"running", "settings": {"<key>": <value>}}`, or one, `{"running", "key", "value"}`.

**`skald config set <key> <value>`** · tool `config_set` with `key` and `value`. `{"running", "key", "value"}`,
with the value as stored: a string, or true or false.

| Key | Values |
|---|---|
| `hold_key` | `right-command`, `right-control` |
| `language` | `bokmal`, `nynorsk` |
| `overlay_theme` | `runepinne`, `smidd-jern`, `kjedelig` |
| `copy_to_clipboard` | `true`, `false` |
| `start_at_login` | `true`, `false` |
| `sounds` | `true`, `false` |

`true` and `false` are also taken as `ja` and `nei`, or `på` and `av`. A key may be written with hyphens.
The rest of the configuration has commands of its own: the model `skald model use`, the microphones
`skald mic`, the ordskatt `skald vocab`, the corrections `skald fix`.

## Microphones

**`skald mic list`** · tool `mic_list`.
`{"running", "microphones": [{"name", "position", "connected", "pinned", "active"}], "pinned", "active"}`.
`position` is the place in the order, from 1, or null when the microphone is not in it; `active` is the one
the next take uses.

**`skald mic use <name>`** · tool `mic_use` with `name`. Uses this microphone ahead of the order while it is
connected. `skald mic use --first`, or the tool without `name`, removes that choice, so the first connected
microphone in the order is used. `{"running", "pinned"}`. The name as `mic list` gives it; case is ignored.

**`skald mic order <name> ...`** · tool `mic_order` with `names`, a list. Sets the whole order.
`{"running", "order"}`. A Bluetooth headset drops to call quality while its microphone is open, so suggest
putting one last.

## Models

The speech models are made by the National Library of Norway (Nasjonalbiblioteket).

**`skald model list`** · tool `model_list`. `{"running", "models": [{"id", "name", "downloaded", "active"}]}`;
`active` is the model in use.

**`skald model use <id>`** · tool `model_use` with `id`. Switches to a downloaded model, by its id or its name;
`invalid` when it is not downloaded. `{"running", "id", "name"}`.

## Transcribing

**`skald transcribe <file.wav>`** · tool `transcribe` with `path`, the file's full path. Transcribes the file
as a take and applies the corrections:
`{"running", "text", "model_text", "corrections": [{"from", "to"}], "heard_nothing"}`. `text` is what Skald
would insert and `model_text` the model's text before corrections, both null when Skald heard nothing;
`corrections` are the rules that changed the text; `heard_nothing` is null, or why: `below_speech_level`,
`too_little_speech`, `no_words` or `known_phrase`. Without a running Skald, the command loads the model
itself first.

## Errors

`{"error": {"code", "message"}}`, with the message in bokmål for the user. The exit code is 1, or 2 for
`usage`.

| Code | Means |
|---|---|
| `usage` | Wrong arguments; the message shows how the command is used |
| `not_found` | No such correction, word, microphone, model or setting |
| `invalid` | A value the setting or command does not take |
| `unreadable` | A file Skald keeps cannot be read as it is; the message names the file and the line |
| `io` | Reading or writing a file, or reaching Skald, failed |
| `version` | Skald was updated while it ran; it must be quit and started again |
| `protocol` | Skald answered with something this command does not understand |
| `failed` | The work itself failed, such as loading the model |

## Installing

`skald skill install [--dir DIR]` writes this skill into `~/.claude/skills/skald` (or DIR), replacing an older
copy; `skald skill uninstall [--dir DIR]` takes it out. `skald mcp` serves the tools above over standard input
and output; these three are commands only, not tools.

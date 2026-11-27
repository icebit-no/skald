# Test set

The sentences Skald's own test set is read from, for M3 in [docs/plan.md](../../docs/plan.md): the same 60
sentences read with a good USB microphone, with the Mac's built-in microphone and from a distance, then run
through `skald bench` to see which settings and models split or join fewer words.

`sentences.txt` holds one sentence per line, numbered from the top. Each line is also the reference the take
is scored against, in bokmål, so it is written the way Skald should write what was said.

## How the sentences were made

Claude wrote them for Skald in November 2026. They are original and MIT like the rest of the repository. They
name no private person and no company: the first names are common ones, and the other names are places and
people from Norwegian history.

They are chosen to provoke what M3 measures:

- compounds that get split, long and short: «kommunestyrerepresentantene», «oppvaskkummen», «lammelår»;
- compounds with a hyphen: «USB-kabelen», «tv-serien», «17. mai-feiringen», «norsk-amerikansk»;
- borrowed words with Norwegian endings: «pull-requesten», «merget», «deployet», «committet», «releasen»;
- words that must stay apart or together: «i dag», «etter hvert», «i hvert fall», «altfor», «annenhver», «hver
  dag» beside «hverdager»;
- numbers, dates and amounts: «3. desember», «1 249 kroner», «12,5 prosent»;
- names: «Hardangervidda», «Nord-Norge», «Fridtjof Nansen»;
- and ordinary messages and notes, as most dictation is.

`skald bench` turns no numbers into words (docs/decisions.md, D-32), so «tre» and «3» differ. The sentences
write numbers as digits and clock times in words, and avoid forms bokmål allows two ways («i stedet» and
«isteden», «havna» and «havnen»), so a word counted wrong is wrong.

## Recording

Look through `sentences.txt` before reading it; a take keeps the reference it was read with, so a sentence
changed later only changes the takes read after. `skald bench record` shows one sentence at a time and records
it while Enter is held:

```sh
cargo run --release -p skald-bench --example bench -- record ~/skald-testsett --microphone usb --device "…"
```

`--microphone` is `usb`, `built-in` or `distance`, the labels the bench reports by. `--device` names the
microphone as `skald mic list` shows it, or part of the name; without it the system's default input is used,
so give it on every run. `--help` has the rest. Run it once per microphone into the same folder:

- `usb`: the USB microphone 15 to 25 cm in front of the mouth, as it is meant to be used.
- `built-in`: the Mac's own microphone, sitting at it as usual, about 50 cm away.
- `distance`: the Mac's own microphone about 2 m away, in a normal voice. Hold Enter on an external keyboard,
  and read from an external screen or from the terminal with its text enlarged.

Start reading once «● Tar opp» shows, since the microphone takes a moment to open. Read as you would dictate,
at a normal pace, in the same quiet room for all three. Then measure the set:

```sh
cargo run --release -p skald-bench --example bench -- ~/skald-testsett
```

Recordings of anyone's voice are never committed; the repository ignores `.wav`. Keep the folder outside the
repository.

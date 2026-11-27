# Bokfink02 as GGUF in transcribe.cpp

M1a, the Bokfink block of [plan.md](../plan.md). Checked on 2026-11-27.

**In short.** NB-ASR Bokfink02 converts to GGUF with transcribe.cpp's own Qwen3-ASR converter. NbAiLab's custom
tokenizer works without changes, and so do keywords. The language prefixes do not: stock transcribe.cpp 0.5.1 cannot
send `language Norwegian<asr_text>` or `language Norwegian Nynorsk<asr_text>`. Without a prefix, Bokfink's output is
unusable. A small patch to transcribe.cpp (120 lines added) fixes this (in `research/bokfink/`); it belongs upstream.
With the patch, transcribe.cpp on Metal writes the same text as NbAiLab's `inference.py` on 47 of the 48 runs (20
clips in bokmål and nynorsk, and 4 clips with keywords in both). The one difference is a rounding effect of NbAiLab's
BF16 run: in float32, NbAiLab's script writes the same text as transcribe.cpp on all 48 runs. On this Mac, Bokfink
transcribes 9.24 s of speech in 261 ms with the BF16 GGUF on the GPU (median of 20 runs with the model loaded, 35
times faster than real time), and in 197 ms with Q8_0.

## What was run

| | |
|---|---|
| Machine | Apple M4 Pro: 14-core CPU (10 performance, 4 efficiency), 20-core GPU, 48 GB; macOS 26.7.3 (25G342) |
| Model | [NbAiLab/nb-asr-qwen06b-bokfink02-beta](https://huggingface.co/NbAiLab/nb-asr-qwen06b-bokfink02-beta) at revision 71292119b4505ab54c8a48a0ffcd2784b25f1c71; `model.safetensors` SHA-256 `6beaa1b49913e98174e3b040d101f8d1be3d58891a70449275149cb5755f7e1a`. All 14 files match the hashes in the repository's `release_provenance.json` |
| Engine | [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) v0.5.1, commit 4a4e08f99c62ae1036551c259477fe4f3127f092, Release build with Metal (`GGML_METAL=ON`, embedded shader library), Apple clang 21.0.0, CMake 4.5.1. Built in a scratch folder outside this repository, with and without the patch below |
| Reference | NbAiLab's `inference.py` from the model folder, imported unchanged by `research/bokfink/reference.py`: `BokfinkASR` with the Transformers backend, BF16, greedy, on **this Mac's CPU** (6 threads). It needs no CUDA, so the Windows PC was not used (see Open items) |
| Python | 3.12.11 through uv 0.11.32, in a scratch venv; every package pinned in `research/bokfink/requirements.txt` (compiled from `requirements.in`: qwen-asr 0.0.6, transformers 4.57.6, torch 2.11.0, accelerate 1.12.0, gguf 0.18.0, numpy 2.4.4, …) |
| Clips | The first 20 sentences of the FLEURS `nb_no` test split ([google/fleurs](https://huggingface.co/datasets/google/fleurs), CC BY 4.0) at revision 70bb2e84b976b7e960aa89f1c648e09c59f894dd, fetched with `research/clips/fetch.py` from the engine-check branch (commit 2401fbce5b8743a1ccad52f7ee566c249494a2fa); all 20 WAV hashes match that script's list. 6.8–17.6 s each, 432 words |

The scripts in `research/bokfink/`:

| File | Does |
|---|---|
| `reference.py` | Transcribes a clip folder with NbAiLab's `inference.py`, once per language, plus keyword runs |
| `transcribe_cpp.py` | The same with `transcribe-cli` and a GGUF; writes the same JSON lines |
| `compare.py` | Joins the two and prints the tables below |
| `prompt_check.py` | Checks that NbAiLab's prompt has the token layout transcribe.cpp builds |
| `bench.c` | Times `transcribe_run` with the model loaded, through transcribe.cpp's C API |
| `keywords.tsv` | The keywords for the keyword runs |
| `transcribe-cpp-qwen3-asr-language-names.patch` | The transcribe.cpp patch, against 4a4e08f |

Commands, with `$TC` the transcribe.cpp checkout, `$BUILD` its build folder, `$MODEL` the downloaded model folder and
`$CLIPS` the clip folder:

```sh
uv venv --python 3.12 venv && VIRTUAL_ENV=venv uv pip sync research/bokfink/requirements.txt
python3 research/clips/fetch.py --out $CLIPS --count 20

git -C $TC checkout 4a4e08f99c62ae1036551c259477fe4f3127f092
git -C $TC apply research/bokfink/transcribe-cpp-qwen3-asr-language-names.patch
cmake -S $TC -B $BUILD -DCMAKE_BUILD_TYPE=Release -DTRANSCRIBE_METAL=ON -DTRANSCRIBE_BUILD_TOOLS=ON
cmake --build $BUILD -j 8 --target transcribe-cli transcribe-quantize

venv/bin/python -I $TC/scripts/convert-qwen3_asr.py $MODEL nb-asr-bokfink02-BF16.gguf \
  --repo-id NbAiLab/nb-asr-qwen06b-bokfink02-beta --variant nb-asr-bokfink02-beta \
  --name "NB-ASR Bokfink02 beta" --author "National Library of Norway (NB-ASR, NbAiLab)" \
  --organization NbAiLab \
  --language nb=Norwegian --language "nn=Norwegian Nynorsk" --language en=English
$BUILD/bin/transcribe-quantize nb-asr-bokfink02-BF16.gguf nb-asr-bokfink02-Q8_0.gguf --quant Q8_0
$BUILD/bin/transcribe-quantize nb-asr-bokfink02-BF16.gguf nb-asr-bokfink02-Q5_K_M.gguf --quant Q5_K_M

R=research/bokfink
venv/bin/python -I -W ignore $R/reference.py --model $MODEL --clips $CLIPS --out reference.jsonl \
  --keywords $R/keywords.tsv
venv/bin/python -I -W ignore $R/reference.py --model $MODEL --clips $CLIPS --out reference-fp32.jsonl \
  --keywords $R/keywords.tsv --float32
python3 $R/transcribe_cpp.py --cli $BUILD/bin/transcribe-cli --model nb-asr-bokfink02-BF16.gguf \
  --clips $CLIPS --out transcribe_cpp.jsonl --keywords $R/keywords.tsv
python3 $R/compare.py reference.jsonl transcribe_cpp.jsonl --clips $CLIPS
venv/bin/python -I -W ignore $R/prompt_check.py --model $MODEL

clang -O2 -I $TC/include $R/bench.c $BUILD/src/libtranscribe.a $BUILD/ggml/src/libggml.a \
  $BUILD/ggml/src/ggml-metal/libggml-metal.a $BUILD/ggml/src/libggml-cpu.a $BUILD/ggml/src/libggml-base.a \
  -framework Metal -framework Foundation -framework Accelerate -lc++ -o bokfink-bench
./bokfink-bench nb-asr-bokfink02-BF16.gguf $CLIPS/fleurs-nb-1994.wav nb 20 3
```

The GGUFs made this way:

| File | Bytes | SHA-256 |
|---|---:|---|
| `nb-asr-bokfink02-BF16.gguf` | 1 571 490 208 | `2207cf66ad84619f691e964027a13e5e195be29e3157e47b288317c8da65cd51` |
| `nb-asr-bokfink02-Q8_0.gguf` | 850 423 648 | `b074bc30068368f6aece078d75c01efc306fbc4a6702bfc2762e4d98694e907e` |
| `nb-asr-bokfink02-Q5_K_M.gguf` | 645 356 384 | `ebf3eb50a959ac2432c97e9c21a60b8d2d372c8c4b1d4d1c4878052e02d57f9a` |

## Conversion

transcribe.cpp's `scripts/convert-qwen3_asr.py` converts Bokfink as it stands, with one flag: it only accepts
the variant names `qwen3-asr-0.6b` and `qwen3-asr-1.7b` and stops with `KeyError: 'nb-asr-bokfink02'` otherwise.
With `--variant qwen3-asr-0.6b` it writes a working BF16 GGUF in about 10 s, with all 611 tensors of the checkpoint
consumed (the output head is tied to the embedding, as in Qwen's release). The architecture, hyperparameters, audio
token ids and frontend are those of Qwen3-ASR 0.6B; only the weights and token names differ. But the stock
converter labels that file «Qwen3-ASR 0.6B» by the Alibaba Qwen Team, and lists Qwen's 30 languages from
`config.json`, which Bokfink inherited unchanged and which do not include Norwegian. The patch adds flags for the
name, author, organization and languages; the commands above use them.

## The custom tokenizer: works as it is

Bokfink's «custom tokenizer» is Qwen's tokenizer with seven reserved tokens renamed. `vocab.json` and `merges.txt`
are byte-identical to Qwen/Qwen3-ASR-0.6B's (same SHA-256). The added tokens differ only in ids 151677–151683:
Qwen's `<blank1>`…`<blank7>` are Bokfink's `<verbatim>`, `<reading_optimised>`, `<summary>`, `<inaudible>`,
`<laughter>`, `<applause>` and `<cough>`. The converter reads the added tokens from `tokenizer_config.json`, so the
GGUF carries the new names, typed as control tokens. The chat template differs too (it keeps user text after the
audio), but transcribe.cpp builds its prompt in code, not from the template.

`prompt_check.py` renders NbAiLab's prompt with NbAiLab's `make_prompt` and processor, and compares it token by
token with the layout transcribe.cpp's `build_prompt_tokens` assembles. They are identical for both language
prefixes, with no keywords, one keyword and three keywords:

```
same  nob  keywords=''  138 tokens, 120 audio
same  nob  keywords='mosasaurer'  142 tokens, 120 audio
same  nob  keywords='Nord-Østersjøen Tromsø Nordlandssykehuset'  156 tokens, 120 audio
      nob prefix ids [11528, 44621, 151704] = ['language', 'ĠNorwegian', '<asr_text>']
same  nno  keywords=''  141 tokens, 120 audio
same  nno  keywords='mosasaurer'  145 tokens, 120 audio
same  nno  keywords='Nord-Østersjøen Tromsø Nordlandssykehuset'  159 tokens, 120 audio
      nno prefix ids [11528, 44621, 451, 1872, 81587, 151704] = ['language', 'ĠNorwegian', 'ĠN', 'yn', 'orsk', '<asr_text>']
```

Nothing has to change for the tokenizer. One difference in the output path, not seen in these runs: NbAiLab's
script decodes with `skip_special_tokens=True`, which drops control tokens, while transcribe.cpp's Qwen3-ASR family
decodes every generated token to text (its `keep_special_tags` switch is honoured only by Canary, Parakeet and
SenseVoice). If Bokfink emits `<inaudible>`, `<laughter>`, `<applause>` or `<cough>`, transcribe.cpp would return the
tag as text. None appeared in the 48 runs or the silence runs; read speech gives the model no reason to.

## Language prefixes: missing, patch needed

**What stock transcribe.cpp does.** A caller asks for a language with a BCP-47 code (`transcribe_run_params.language`,
`-l` on the CLI). The dispatcher (`src/transcribe-asr.cpp`) rejects any code not in the GGUF's `general.languages`,
and `encode_language_prefix` in `src/arch/qwen3_asr/model.cpp` turns the code into a name through a fixed table,
`k_qwen3_asr_language_names`, which holds Qwen's 30 languages. Norwegian is in neither, so `-l nb` and `-l no` stop
with «unsupported language». There is no other way in: Qwen3-ASR does not take the generic `prefix` field, and the
prefix ends in the `<asr_text>` control token, which plain text cannot carry.

**Without a prefix, Bokfink is unusable.** Bokfink was trained with a prefix in every example. Without one, of the
first five clips one came out in English («He did not set numbers for the cuts…»), two began with stray text
(«1000. Som en godt…», «# Da demningen…»), one with a stray space, and one was right; clip 1970 came back empty.

**The patch** (`research/bokfink/transcribe-cpp-qwen3-asr-language-names.patch`, 120 lines added, 27 removed) lets
a GGUF name its own languages:

- `scripts/convert-qwen3_asr.py`: `--language CODE=NAME` (repeatable) writes the codes to `general.languages` and the
  names to a new string array, `stt.qwen3_asr.language_names`, in the same order. `--name`, `--author`,
  `--organization` and `--description` set the `general.*` identity, and any `--variant` is accepted when `--name`
  is given. When there is no `chat_template.json`, it reads `chat_template.jinja`. Without the new flags, the output
  is unchanged.
- `src/arch/qwen3_asr/model.cpp` and `qwen3_asr.h`: the loader reads `stt.qwen3_asr.language_names` when present
  (one name per `general.languages` entry, else a load error). `encode_language_prefix` looks a code up there
  before the fixed table, and the detected language in auto mode is mapped back the same way. Without the key,
  behaviour is unchanged.

transcribe.cpp's own Qwen3-ASR tests that this touches pass with the patch: `transcribe_qwen3_asr_smoke` and
`transcribe_qwen3_asr_bpe_parity` (the parity test ran on a GGUF of Bokfink made with the stock converter, whose
BPE is identical to Qwen's). `transcribe_qwen3_asr_batch_truncation` fails: it expects Qwen's long English transcript of
`samples/jfk.wav` to overflow a 3 072-token context, and Bokfink, run without a language as the test does, stops
before that. The test is written for Qwen's weights, not for the code the patch changes.

**Where it belongs: upstream.** The change is about Qwen3-ASR fine-tunes in general, not about Skald: any fine-tune
that adds languages or output controls through the language header (Nordlyd will be the next) needs it. It is
small, keeps every existing GGUF working, and follows how transcribe.cpp already carries per-model metadata in
`stt.*` keys. Skald needs only to pass `nb` or `nn`. Kept in Skald instead, the patch would mean building a fork of
transcribe.cpp for every release. Skald can build with the patch until upstream takes it or a change like it.

## Keyword context: works as it is

NbAiLab puts keywords in the SYSTEM message as plain words separated by spaces, with no label. transcribe.cpp's
Qwen3-ASR family writes the generic `vocabulary` terms into the system message joined by single spaces
(`encode_system_context`, `src/arch/qwen3_asr/model.cpp`), followed by the `prompt` field after a space. That is
the same text in the same slot, and `prompt_check.py` shows the same tokens. All 8 keyword runs below match the
reference. Skald passes its words as `vocabulary` terms, one term per word or name. The CLI's `--vocabulary` splits
on commas; through the C API each array entry is one term. Over budget, transcribe.cpp drops terms from the end of
the list, which only happens near the 65 536-token context.

The keywords changed the text: in bokmål, «mosauer» became «mosasaurer», «skipssekvenser» became
«skipsekskursjoner» and «Nordøstersjøen» became «Nord-Østersjøen». The two unrelated words added to clip 2000
did not appear in the text. «802.11n» changed the nynorsk run but not the bokmål one.

## Verbatim style: not reachable, not needed for the beta

NbAiLab's verbatim style is `\n<verbatim>` in the USER message after the audio. transcribe.cpp writes `<|im_end|>`
right after the audio and has no field for text there; the `prompt` field goes to the system message, and
`<verbatim>` is a control token that transcribe.cpp refuses in plain text. Adding it would need a new run parameter
or family option in transcribe.cpp, which is a design question for upstream. The default, reading-optimised output,
is what a dictation app inserts, and it is what every run here used.

## Per-clip results

### transcribe.cpp (BF16 GGUF, Metal) against NbAiLab's script as published (BF16, CPU)

**47 of 48 runs match exactly.**

#### Bokmål (`language Norwegian<asr_text>`): 20 of 20 exact

| Clip | Exact | Word differences (reference → transcribe.cpp) | Text |
|---|---|---|---|
| 1661 | yes |  | Han satte ikke tall for kuttene, og sa at disse ville bli foretatt basert på Kinas økonomiske produksjon. |
| 1664 | yes |  | Rør sammen de to tørre pulverne, og så, med rene, våte hender, press den sammen til en ball. |
| 1680 | yes |  | Som en godt avrundet idrettsutøver kan tigeren klatre, ikke veldig bra, svømme, hoppe over store anstander og trekke med fem ganger styrken til et sterkt menneske. |
| 1686 | yes |  | Prosessen der en organisasjon jobber sammen for å overvinne et hinder, kan dermed føre til en ny, innovativ prosess som kan bidra til å dekke behovene til kunder. |
| 1721 | yes |  | Da demningen ble bygd i 1963, ble det satt stopper for den sesongmessige flommen som spredte sedimenteringen nedover elven. |
| 1766 | yes |  | Noen atomer har ustabile kjerner, noe som betyr at de har en tendens til å falle fra hverandre mer eller mindre uprovosert. |
| 1768 | yes |  | De fleste bygningene på kantene av komplekset har blitt gjenoppbygd for å kunne gi turister en bedre idé om hvordan de opprinnelige ble bygd. |
| 1802 | yes |  | Det er en kombinasjon av elementer med både masse og interpersonlig kommunikasjon på internett. |
| 1811 | yes |  | 802.1 standarden driver på frekvensene 2,4 GHz og 5,0 GHz. |
| 1833 | yes |  | Filosofen Aristoteles hadde teorier om at alt innebærer en miks av ett eller flere av jordens fire elementer. De fire elementene er jord, vann, luft og ild. |
| 1898 | yes |  | Han, Wales, var i utgangspunktet uærlig med oss fra begynnelsen, først i atferden sin som om dette var av juridiske årsaker, for det andre ved å gi inntrykk av at han hørte på oss rett opp til slettingen av kunsten sin. |
| 1906 | yes |  | Dette er ikke et farvel, dette er lukkingen av et kapittel og åpningen av et nytt. |
| 1930 | yes |  | Seks gisler, inkludert barna og de gamle, samt de filippinske fotografene, ble tidlig løslatt. |
| 1970 | yes |  | Vi vet ikke helt sikkert, men den kan ha hatt en forkjært tunge. Kostholdet inneholdt skilpadder, storfisk, andre mosauer, og den kan også ha vært en kannibal. |
| 1971 | yes |  | Sterk vind, hagl, masse nedbør og skogbranner er former for effekter av ekstremvær, som for eksempel tordenvær, tornadoer, styrtregn og sykloner. |
| 1981 | yes |  | Dersom du bare går i land ved bruk av skipssekvenser, vil du ikke behøve et eget visum fra 2009. |
| 1994 | yes |  | Hvis du allerede kan et romansk språk, vil det være lettere for deg å lære portugisisk. |
| 2000 | yes |  | Dersom du skal over Nordøstersjøen på vinterstid, bør du kontrollere hyttens beliggenhet, da det å kjøre gjennom is avgir ekstremt mye støy for de som er mest berørte. |
| 2001 | yes |  | Reisende er sterkt rådet til å være klar over enhver risiko for uvær som berører deres område, da det kan påvirke på eventuelle reiseplaner. |
| 2006 | yes |  | Elementene kalsium og kalium blir sett på som metall, selvfølgelig har man også metaller som sølv og gull. |

#### Nynorsk (`language Norwegian Nynorsk<asr_text>`): 19 of 20 exact

| Clip | Exact | Word differences (reference → transcribe.cpp) | Text |
|---|---|---|---|
| 1661 | yes |  | Han sette ikkje tal for kutta, og sa at desse ville bli gjort basert på den økonomiske produksjonen i Kina. |
| 1664 | yes |  | Rør saman dei to tørre pulvera, og så, med reine, våte hender, press han saman til ein ball. |
| 1680 | yes |  | Som ein godt avrunda idrettsutøvar kan tigeren klatra, ikkje veldig bra, symja, hoppa over store anstandar og trekkja med fem gonger styrken til eit sterkt menneske. |
| 1686 | yes |  | Prosessen der ein organisasjon jobbar saman for å overvinna eit hinder, kan dermed føre til ein ny, innovativ prosess som kan bidra til å dekka behova til kundar. |
| 1721 | yes |  | Då demninga vart bygd i 1963, vart det sett stoppar for den sesongmessige flaumen som spreidde sedimenteringa nedover elva. |
| 1766 | yes |  | Nokre atom har ustabile kjernar, noko som betyr at dei har ein tendens til å falle frå kvarandre meir eller mindre uprovosert. |
| 1768 | yes |  | Dei fleste bygningane på kantane av komplekset har blitt gjenoppbygde for å kunne gje turistar ein betre idé om korleis dei opphavlege vart bygde. |
| 1802 | yes |  | Det er ein kombinasjon av element med både masse og interpersonleg kommunikasjon på internett. |
| 1811 | **no** | «802.1-standarden» → «802.1 standarden» | 802.1-standarden driv på frekvensane 2,4 GHz og 5,0 GHz. |
| 1833 | yes |  | Filosofen Aristoteles hadde teoriar om at alt inneber ein miks av eitt eller fleire av dei fire elementa i jorda. Dei fire elementa er jord, vatn, luft og eld. |
| 1898 | yes |  | Han, Wales, var i utgangspunktet uærleg med oss frå byrjinga, fyrst i åtferda si, som om dette var av juridiske årsaker, for det andre ved å gje inntrykk av at han høyrde på oss rett opp til slettinga av kunsten sin. |
| 1906 | yes |  | Dette er ikkje eit farvel, dette er lukkinga av eit kapittel og opninga av eit nytt. |
| 1930 | yes |  | Seks gislar, inkludert barna og dei gamle, og dei filippinske fotografane, vart tidleg lauslatne. |
| 1970 | yes |  | Vi veit ikkje heilt sikkert, men han kan ha hatt ein forkjørt tange. Kosthaldet inneheldt skjelpadder, storfisk, andre mosauroar, og han kan òg ha vore ein kannibal. |
| 1971 | yes |  | Sterk vind, hagl, masse nedbør og skogbrannar er former for effektar av ekstremvêr, som til dømes torevêr, tornadoar, styrtregn og syklonar. |
| 1981 | yes |  | Dersom du berre går i land ved bruk av skipssekursjonar, vil du ikkje trenge eit eige visum frå 2009. |
| 1994 | yes |  | Viss du allereie kan eit romansk språk, vil det vera lettare for deg å læra portugisisk. |
| 2000 | yes |  | Dersom du skal over Nordaustsjøen på vinterstid, bør du kontrollere plasseringa til hytta, då det å køyra gjennom is gjev ekstremt mykje støy for dei som er mest ramma. |
| 2001 | yes |  | Reisande er sterkt rådd til å vera klar over alle risikoar for uvêr som rører området deira, då det kan påverke på eventuelle reiseplanar. |
| 2006 | yes |  | Elementa kalsium og kalium blir sett på som metall. Sjølvsagt har ein òg metall som sylv og gull. |

#### Bokmål (`language Norwegian<asr_text>`), with keywords: 4 of 4 exact

| Clip | Keywords | Exact | Word differences (reference → transcribe.cpp) | Text |
|---|---|---|---|---|
| 1811 | 802.11n | yes |  | 802.1 standarden driver på frekvensene 2,4 GHz og 5,0 GHz. |
| 1970 | mosasaurer | yes |  | Vi vet ikke helt sikkert, men den kan ha hatt en forkjært tunge. Kostholdet inneholdt skilpadder, storfisk, andre mosasaurer, og den kan også ha vært en kannibal. |
| 1981 | skipsekskursjoner | yes |  | Dersom du bare går i land ved bruk av skipsekskursjoner, vil du ikke behøve et eget visum fra 2009. |
| 2000 | Nord-Østersjøen Tromsø Nordlandssykehuset | yes |  | Dersom du skal over Nord-Østersjøen på vinterstid, bør du kontrollere hyttens beliggenhet, da det å kjøre gjennom is avgir ekstremt mye støy for de som er mest berørte. |

#### Nynorsk (`language Norwegian Nynorsk<asr_text>`), with keywords: 4 of 4 exact

| Clip | Keywords | Exact | Word differences (reference → transcribe.cpp) | Text |
|---|---|---|---|---|
| 1811 | 802.11n | yes |  | 802.11n standarden driv på frekvensane 2,4 GHz og 5,0 GHz. |
| 1970 | mosasaurer | yes |  | Vi veit ikkje heilt sikkert, men han kan ha hatt ein forkjølt tange. Kosthaldet inneheldt skjelpadder, storfisk, andre mosasaurer, og han kan òg ha vore ein kannibal. |
| 1981 | skipsekskursjoner | yes |  | Dersom du berre går i land ved bruk av skipsekskursjoner, vil du ikkje trenge eit eige visum frå 2009. |
| 2000 | Nord-Østersjøen Tromsø Nordlandssykehuset | yes |  | Dersom du skal over Nord-Østersjøen på vinterstid, bør du kontrollere plasseringa til hytta, då det å køyra gjennom is gjev ekstremt mykje støy for dei som er mest ramma. |

### The one difference is rounding in the reference

On nynorsk clip 1811, NbAiLab's BF16 run writes «802.1-standarden» and transcribe.cpp «802.1 standarden», with the
CPU and the Metal backend alike. `reference.py --float32` runs NbAiLab's script with the model cast to float32 (it
is the same script; only the weights' type changes). In float32 it writes «802.1 standarden», and **all 48 runs
match transcribe.cpp exactly**. The BF16 and float32 runs of the reference differ on that one run. So the GGUF
computes what the checkpoint computes; PyTorch's BF16 rounding tips a near tie. NbAiLab's BF16 run keeps every
intermediate result in BF16, while transcribe.cpp keeps the weights in BF16 and the intermediate results in float32.

### Quantized GGUFs

The same 48 runs with GGUFs quantized by `transcribe-quantize`:

| GGUF | Size | Same as NbAiLab BF16 | Same as NbAiLab float32 | Word errors against FLEURS, bokmål, 20 clips |
|---|---:|---:|---:|---:|
| BF16 | 1.57 GB | 47 of 48 | 48 of 48 | 20 of 432 (4.6 %) |
| Q8_0 | 850 MB | 44 of 48 | 45 of 48 | 20 of 432 (4.6 %) |
| Q5_K_M | 645 MB | 40 of 48 | 39 of 48 | 22 of 432 (5.1 %) |

What Q8_0 changed, against the float32 reference: bokmål 1981 «skipssekvenser» → «skipssekursjoner», bokmål 2006
«metall, selvfølgelig» → «metall. Selvfølgelig», nynorsk 1981 «skipssekursjonar» → «skipssekskursjonar». Q5_K_M
changed nine runs, among them bokmål 2001 «sterkt rådet» → «sterk råd» and nynorsk 1811 with keywords «802.11n
standarden driv» → «802.11n-drivarar». The error rates are for context only: 20 clips are too few to tell the
GGUFs apart, and FLEURS spells numbers and punctuation its own way. The error rate in M3 decides.

### Silence

Five seconds each of digital silence, white noise peaking at −50 dBFS and pink noise peaking at −30 dBFS, made with
ffmpeg (`-f lavfi -i anullsrc=r=16000:cl=mono`, `anoisesrc=r=16000:a=0.003:c=white`,
`anoisesrc=r=16000:a=0.03:c=pink`, `-t 5`): in bokmål, both engines return empty text for all three. In nynorsk, both
return «Det er.» for all three. That is the model, not the engine.

## Time to transcribe 9 s of speech

Clip `fleurs-nb-1994`: 9.24 s, 16 words («Hvis du allerede kan et romansk språk, vil det være lettere for deg å lære
portugisisk.»), with the bokmål prefix, on Metal. `bench.c` loads the model once, runs the clip 3 times untimed, then
times 20 calls of `transcribe_run`. The wall time covers everything a dictation costs once the model is loaded: mel,
encoder, prompt and decoding. Each series ran under the shared GPU timing lock, with a watcher that logs any other
speech engine running at the same time; it logged none.

Series at 15:52, load average 4.62 / 4.01 / 4.89 at the start:

| GGUF | Median | Fastest – slowest | Mel | Encoder | Decoder | Times faster than real time |
|---|---:|---:|---:|---:|---:|---:|
| BF16 | **261 ms** | 257–274 ms | 1.6 ms | 36 ms | 222 ms | 35 |
| Q8_0 | **197 ms** | 196–199 ms | 1.6 ms | 36 ms | 158 ms | 47 |
| Q5_K_M | **186 ms** | 185–188 ms | 1.6 ms | 37 ms | 146 ms | 50 |

An earlier series at 15:47 (load average 3.50 / 4.91 / 5.64) gave 263, 198 and 186 ms. Most of the time goes to
decoding, one token at a time, so a longer text takes longer; the encoder is a small share. Loading the model took
140–290 ms with the file already in memory. The first load after the build took about 5 s, mostly compiling Metal
shaders, which happens once per machine and build.

Series that overlapped with other jobs on the GPU (another worker's `whisper-cli` and `engine-probe` runs, logged
by the watcher) had every second run 300–500 ms slower and medians up to 700 ms. Their fastest runs agree with the
table. A dictation in Skald will compete with other apps on the GPU in the same way.

For comparison only: NbAiLab's script on the CPU (6 threads, not tuned) took 3.6 s for this clip in BF16 and 1.5 s
in float32.

## Licence and attribution, if we host the GGUF

Bokfink is Apache 2.0, built on Qwen3-ASR 0.6B, which is also Apache 2.0. Neither repository has a NOTICE file. A
GGUF converted by us and hosted by us is a derivative work under the licence and needs, next to the file:

- the Apache 2.0 licence text (the `LICENSE` file from NbAiLab's repository);
- a notice that the file was changed: converted from NbAiLab/nb-asr-qwen06b-bokfink02-beta at revision
  71292119b4505ab54c8a48a0ffcd2784b25f1c71 to GGUF with transcribe.cpp at commit 4a4e08f and the patch above; tensors
  renamed, the mel filterbank and window added, and, if so, quantized to Q8_0 or another type;
- the attribution: NB-ASR Bokfink02 by the NB-ASR project at the National Library of Norway (Nasjonalbiblioteket),
  on the Qwen3-ASR architecture by the Alibaba Qwen team, with links to both model cards;
- NbAiLab's own caveats: a beta checkpoint, not a production guarantee, and the weights give no rights to the
  training data;
- the same identity inside the GGUF (`general.name`, `general.author`, `general.license`, `general.description`),
  which the patched converter writes.

Skald credits the model in the About tab, on the site and in `THIRD-PARTY-NOTICES` in any case (CLAUDE.md).

Two things on the model card bear on hosting. It calls the release a «private beta» for «authorised evaluation»,
and says the repository «is private, not public-and-gated». But on 2026-11-27 the repository is public: Hugging
Face reports it as neither private nor gated, and every file downloads without an account. The card also names the
repository `NbAiLab/nb-asr-bokfink02-beta`, which now redirects to `nb-asr-qwen06b-bokfink02-beta`. The licence
allows redistribution either way. Whether to host a copy before asking NbAiLab is Thomas's decision (below).

## What works and what does not

| | Stock transcribe.cpp 0.5.1 | With the patch |
|---|---|---|
| Conversion to GGUF | Yes, with `--variant qwen3-asr-0.6b`; labelled as Qwen's model | Yes, with its own name and languages |
| Custom tokenizer | Yes | Yes |
| `language Norwegian<asr_text>` | No: «unsupported language» | Yes, `-l nb` |
| `language Norwegian Nynorsk<asr_text>` | No | Yes, `-l nn` |
| Keyword context | Yes, `vocabulary` | Yes |
| Verbatim style | No | No |
| Same text as NbAiLab's script | Not possible without a prefix | 47 of 48 runs (BF16 reference), 48 of 48 (float32 reference) |
| Control tokens dropped from the text | No (none seen) | No (none seen) |

## Open items

- **No CUDA reference.** NbAiLab documents its script for a CUDA GPU; here it ran on this Mac's CPU, in BF16 as
  published and in float32. The Windows PC with an NVIDIA GPU was busy and not used. A CUDA run in BF16 may tip
  other near ties the way the CPU's BF16 run tipped clip 1811; the float32 run is the one the GGUF should match,
  and it does on all 48 runs.
- **Upstream review** of the patch has not happened; it is a local branch in a scratch checkout of transcribe.cpp.
- **Control tokens in the output** are untested: read speech and silence gave the model no reason to emit them.

## What Skald needs before Bokfink can be offered as a beta choice

1. **transcribe.cpp that takes a GGUF's language names.** Send the patch upstream to handy-computer/transcribe.cpp, and
   build Skald with it until a release carries it. The binding work in M1b does not change: Bokfink runs through the
   same `transcribe_run` as NB-Whisper.
2. **A Bokfink GGUF to download** at a pinned NbAiLab revision with a SHA-256, converted with the patched converter
   (`nb`, `nn`, `en` declared). Someone has to host it, and one quantization has to be chosen (questions below).
3. **Always a language for Bokfink.** Skald's bokmål or nynorsk setting maps to `nb` or `nn`, and Skald never runs
   Bokfink without one. The language codes differ per model, so the engine layer needs them per model.
4. **Vocabulary per model.** For NB-Whisper, Skald's prompt is a sentence with inflected forms, the most important
   names last (CLAUDE.md). Bokfink's card asks for plain words separated by spaces, with no sentence and no
   instructions. Skald needs to send its vocabulary to Bokfink as a list of terms, not as the Whisper sentence.
5. **The silence gate without a no-speech probability.** D-11's third check, the model's no-speech probability, is
   Whisper's; Qwen3-ASR has none. Bokfink returns empty text on silence and noise in bokmål, but «Det er.» in
   nynorsk, so the level and VAD checks must run before Bokfink, and «Det er.» goes in the list of known phrases.
6. **Control tokens out of the text.** transcribe.cpp returns any control token Bokfink emits (`<laughter>`,
   `<inaudible>`) as text, where NbAiLab's script drops it. Either transcribe.cpp's Qwen3-ASR family honours
   `keep_special_tags` like its other families (a second small upstream change), or Skald removes the tags before
   inserting the text.
7. **Long takes.** transcribe.cpp runs Qwen3-ASR on the whole take in one call (up to about 87 minutes); there are no
   30-second windows. The Whisper rule of carrying the prompt into every window does not apply.
8. **Its measurement in M3.** Word error rate and spacing errors on Skald's own recordings against NB-Whisper Large,
   for the quantization chosen, before the beta label goes on.

## Questions for Thomas

- **Hosting.** Should Skald host the converted GGUF itself (for example on Hugging Face under icebit-no) with the
  licence text and attribution above? Or should we first ask NbAiLab, given that the model card still calls the
  release a private beta, or ask them to publish a GGUF themselves?
- **Quantization.** BF16 (1.57 GB) gives the reference text; Q8_0 (850 MB) changed 3 of 48 runs and Q5_K_M
  (645 MB) changed 9. Which one goes on to M3, or should M3 compare two?
- **Upstream.** May the transcribe.cpp patch be offered upstream as a pull request from Thomas's account?

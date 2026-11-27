# NB-Whisper and Silero VAD in transcribe.cpp

M1a, the «NB-Whisper and VAD» block of [plan.md](../plan.md). Checked on 2026-11-27. The Bokfink block is in
[bokfink-gguf.md](bokfink-gguf.md).

**In short.**

- **Conversion works.** transcribe.cpp's converter turns NbAiLab's NB-Whisper Large checkpoint into a GGUF. It writes
  the same text as whisper.cpp with NbAiLab's `ggml-model.bin` on all 20 clean FLEURS clips, greedy and beam 5
  alike. transcribe.cpp also loads `ggml-model.bin` itself, with the same text as the converted GGUF on 40 of 40
  clips. First run downloads that file from NbAiLab (decided in review).
- **The prompt works; carrying it does not yet.** The vocabulary prompt works as an initial prompt: 10 of 23 terms
  right against 4 without it. Carrying it into every 30-second window works only with a small prompt. transcribe.cpp
  0.5.1 puts the prompt plus up to 223 tokens of earlier text into each window, so a full ordskatt (213 tokens)
  breaks a 97 s take (61 % word errors). A small change fixes this; Skald builds transcribe.cpp with it until
  upstream releases it, so the meter keeps its 223 tokens (decided in review).
- **Silero VAD separates speech from silence on every take.** It finds no speech in any of 10 takes without speech
  and at least 380 ms in every take with speech, so the 250 ms rule has room on both sides. NB-Whisper's no-speech
  probability is 0.000 on every silent take, so the no-speech check is dropped from design.md (decided in review,
  D-22). NB-Whisper marks silence with the text «<|nocaptions|>» instead, and transcribe.cpp suppresses that text
  (as Hugging Face does); NB-Whisper then invents a sentence. The VAD check is what keeps silence out.
- **No beam search.** transcribe.cpp 0.5.1 decodes Whisper greedily. On these clips beam 5 (in whisper.cpp) brought
  no gain: the same text on 20 of 20 clean clips, and on 20 noisy clips 3 better, 1 worse, 1 mixed, word errors
  20.1 % greedy against 20.4 % with beam 5. The recommendation is greedy until M3 measures it on Skald's own
  recordings (D-16 says otherwise; a question for Thomas below).
- **Binding: transcribe.cpp's own Rust crates**, `transcribe-cpp` and `transcribe-cpp-sys` 0.5.1. transcribe-rs does
  not use transcribe.cpp at all. It wraps whisper.cpp and ONNX Runtime and cannot run Bokfink.
- **Speed:** with the model loaded, 9.24 s of speech takes 834 ms greedy on the M4 Pro's GPU (11× faster than real
  time; median of 10 runs). Q8_0 takes 766 ms. whisper.cpp takes 944 ms greedy and 1071 ms with beam 5.

## What was run

| | |
|---|---|
| Machine | Apple M4 Pro: 14-core CPU (10 performance, 4 efficiency), 20-core GPU, 48 GB; macOS 26.7.3 (25G342). Shared with other work; see the timing section for the load |
| Engine | [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) v0.5.1, commit `4a4e08f99c62ae1036551c259477fe4f3127f092`, Release build with Metal (on by default on Apple Silicon) and `TRANSCRIBE_BUILD_TOOLS=ON`, Apple clang 21.0.0, CMake 4.5.1, built outside the repository |
| Rust binding | `transcribe-cpp` 0.5.1 and `transcribe-cpp-sys` 0.5.1 from crates.io, built from their vendored source with the default `metal` feature, Rust 1.98.1. Used by `research/engine-check/probe` |
| Comparison | whisper.cpp 1.9.6 from Homebrew (`whisper-cli`, `whisper-server`); its source read at tag v1.9.6, commit `96ec4d9823e9d0211944172da35f9f29134af990` |
| Model | [NbAiLab/nb-whisper-large](https://huggingface.co/NbAiLab/nb-whisper-large) at revision `8c6249fdeeb4dcd05e5735a4c39640607eb6e4ac`. Shards: `model-00001-of-00002.safetensors` SHA-256 `4bdf9a2c…0e589a`, `model-00002-of-00002.safetensors` `a39147be…d283c`. `ggml-model.bin` (F16, 3.10 GB) `0f2f66f22e11a7c7da3c582d8e5c89cb2c0011753ba9c7c9731e320a4ba33e76`; `ggml-model-q5_0.bin` (1.08 GB) `feb5951ae694a62cfeb81fb501f6cfa8cc50d96bcddb1e4e8215f7006bac23a2`. All match the repository's LFS hashes |
| Converter | transcribe.cpp's `scripts/convert-whisper.py` in its pinned environment (`scripts/envs/whisper`, `uv.lock`: torch 2.11.0, transformers 5.6.1, safetensors 0.7.0), through `research/engine-check/convert_nb_whisper.py` |
| GGUFs made | F32 6.18 GB `b8abad46b7467a6764b1ac3a9f24b7f9fac16c0b8df218a8099a851e5a878057`; F16 3.11 GB `2a965b0f72fb586894d007a8a5a6e9b5d24fa653456a4611344f06df912b7afb`; Q8_0 1.67 GB `36d82eef54e3cb0bb1b63bbec0bf95437d1ff6b5da0c83b439b83925f5ae6ce3`; Q5_K_M 1.16 GB `e69b1f1ebc97a596c18f3196c05059021b33a4282353203aa0ed6c4d908ac40d` (`transcribe-quantize` from the F32) |
| VAD | [handy-computer/silero-vad-v6.2-gguf](https://huggingface.co/handy-computer/silero-vad-v6.2-gguf) at `a9b2d61c20ae4ddf3644edb34969e8185a485448`, `silero-vad-v6.2-F32.gguf` SHA-256 `0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6` (MIT, Silero Team). Runs on the CPU, the backend transcribe.cpp picks for VAD |
| Speech clips | The first 20 sentences of the FLEURS `nb_no` test split ([google/fleurs](https://huggingface.co/datasets/google/fleurs) at `70bb2e84b976b7e960aa89f1c648e09c59f894dd`, CC BY 4.0), fetched by `research/clips/fetch.py` |
| Noisy clips | The same 20 with a synthetic room echo and pink noise at 5 dB SNR, as from a distant microphone (`make_clips.py noisy`) |
| Vocabulary clips | 8 sentences with developer words and a 97 s take, spoken by macOS `say -v Nora`, plus 5 one-word takes (`make_clips.py vocab`) |
| Takes without speech | Digital silence, pink noise at −60 and −40 dBFS, 50 Hz hum, a breath-like noise burst, a click, typing (all synthetic), and 1.5 s of room tone before the speaker starts in three FLEURS clips (`make_clips.py silence`) |
| Decoding | Language `no` (bokmål), greedy at temperature 0 with transcribe.cpp's default fallback (Whisper's recipe: temperature step 0.2, compression ratio 2.4, log probability −1.0, no-speech 0.6), timestamps off unless a table says otherwise. whisper.cpp: `-l no -nt`, `-bs 1` (greedy) or `-bs 5` |

`tc` in the tables is transcribe.cpp, `wc` is whisper.cpp. Word error rates are computed on lowercase words without
punctuation (hyphens and decimal points kept), against FLEURS's own text, which writes numbers as digits and keeps
abbreviations such as «f.eks.».

### How to run it again

```sh
# Clips (FLEURS, standard library only) and synthetic takes
python3 research/clips/fetch.py --out $CLIPS
python3 research/engine-check/make_clips.py vocab   --out $TAKES
python3 research/engine-check/make_clips.py silence --out $TAKES --fleurs $CLIPS
python3 research/engine-check/make_clips.py noisy   --out $NOISY --fleurs $CLIPS

# transcribe.cpp with Metal, outside the repository
git clone https://github.com/handy-computer/transcribe.cpp && git -C transcribe.cpp checkout 4a4e08f99c62ae1036551c259477fe4f3127f092
cmake -S transcribe.cpp -B $BUILD -DCMAKE_BUILD_TYPE=Release -DTRANSCRIBE_BUILD_TOOLS=ON && cmake --build $BUILD -j 8

# NB-Whisper Large: the 14 small files and two shards of revision 8c6249fd… into $HF, then
uv sync --frozen --project transcribe.cpp/scripts/envs/whisper
PY=transcribe.cpp/scripts/envs/whisper/.venv/bin/python
$PY research/engine-check/merge_safetensors.py $HF $MERGED
$PY research/engine-check/convert_nb_whisper.py transcribe.cpp $MERGED nb-whisper-large-F32.gguf \
    --repo-id NbAiLab/nb-whisper-large --revision 8c6249fdeeb4dcd05e5735a4c39640607eb6e4ac
$BUILD/bin/transcribe-quantize nb-whisper-large-F32.gguf nb-whisper-large-F16.gguf --quant F16   # also Q8_0, Q5_K_M

# The probe, then each step (each prints its tables and writes JSON to --results)
cargo build --release --manifest-path research/engine-check/probe/Cargo.toml
RUN="python3 research/engine-check/run.py --probe $PROBE --gguf nb-whisper-large-F16.gguf --bin ggml-model.bin \
     --gpu-lock $LOCK --results $RESULTS"
$RUN parity --clips $CLIPS
$RUN parity --clips $NOISY --pattern 'noisy-*.wav' --fleurs $CLIPS --name parity-noisy
$RUN parity --clips $CLIPS --also nb-whisper-large-Q8_0.gguf nb-whisper-large-Q5_K_M.gguf ggml-model-q5_0.bin --name quant
$RUN prompt --takes $TAKES --prompt research/engine-check/prompts/ordskatt.txt \
     --long-prompt research/engine-check/prompts/ordskatt-long.txt --timestamps none --name prompt-ts-none   # and auto
$RUN vad --vad silero-vad-v6.2-F32.gguf --clips $CLIPS --takes $TAKES
$RUN speed --clip $CLIPS/fleurs-nb-1994.wav --extra-gguf nb-whisper-large-Q8_0.gguf nb-whisper-large-Q5_K_M.gguf
```

The results these commands wrote are in `research/engine-check/results/`. `--gpu-lock` names a directory shared with
other work on the same Mac: the timing step creates it (atomically, waiting while it exists) and removes it after,
and every other engine call waits while it exists.

## 1. Conversion, and the same text as whisper.cpp

The converter needed two things it does not do itself. NbAiLab ships the weights in two shards and the converter
reads one `model.safetensors`, so `merge_safetensors.py` joins them tensor for tensor. And the converter refuses
variant names it does not list, so `convert_nb_whisper.py` registers `nb-whisper-large` and calls the converter
unchanged. No weight or tokenizer was touched; the GGUF's metadata names NbAiLab as author and Apache 2.0 as licence.

**Clean FLEURS: the same text everywhere.** On all 20 clips, five configurations write the same text: the converted
GGUF in transcribe.cpp (`tc-gguf`), NbAiLab's `ggml-model.bin` loaded by transcribe.cpp (`tc-bin`), and whisper.cpp
greedy, greedy with `--suppress-nst`, and beam 5 with `--suppress-nst`. The first 10 clips are item 1 of the plan;
the other 10 are the same check repeated.

| clip | words | word errors | where the text differs from the reference |
|---|---:|---:|---|
| 1970 | 27 | 4 | «Vi vet ikke» → «Du vet det ikke»; tunge → tange; den → det |
| 1994 | 16 | 0 | |
| 2000 | 28 | 1 | Nord-Østersjøen → Nordøstersjøen |
| 1981 | 19 | 1 | skipsekskursjoner → skipsekursjoner |
| 1680 | 26 | 1 | avstander → anstander |
| 1766 | 22 | 0 | |
| 1664 | 18 | 1 | dem → den |
| 1768 | 24 | 2 | kantene → kanten; opprinnelig → opprinnelige |
| 1686 | 27 | 1 | an «en» added |
| 1802 | 14 | 0 | |
| **first 10** | **221** | **11 (5.0 %)** | |
| 1811 | 7 | 5 | 802.11n → 802,1; 2.4Ghz → 2,4 GHz; 5.0Ghz → 5,0 GHz |
| 1971 | 22 | 3 | «og» dropped; f.eks. → for eksempel |
| 1930, 1661, 2006, 2001, 1833 | | 0 | |
| 1906 | 16 | 1 | ett → et |
| 1898 | 41 | 1 | atferden → adferden |
| 1721 | 19 | 1 | sedimentering → sedimenteringen |
| **all 20** | **428** | **22 (5.2 %)** | identical in all five configurations |

**Noisy FLEURS: the converter is exact; the engines differ in two close calls.** On the 20 noisy clips:

| clip | tc-gguf | tc-bin | wc greedy | wc greedy, `-sns` | wc beam 5, `-sns` |
|---|---:|---:|---:|---:|---:|
| 1970 | 18.5 | 18.5 | 22.2 | 22.2 | 22.2 |
| 1994 | 6.2 | 6.2 | 6.2 | 6.2 | 6.2 |
| 2000 | 21.4 | 21.4 | 100 «<\|nocaptions\|>» | 21.4 | 21.4 |
| 1981 | 42.1 | 42.1 | 100 «<\|nocaptions\|>» | 42.1 | 36.8 |
| 1680 | 26.9 | 26.9 | 100 «<\|nocaptions\|>» | 26.9 | 26.9 |
| 1766 | 4.5 | 4.5 | 4.5 | 4.5 | 4.5 |
| 1664 | 5.6 | 5.6 | 5.6 | 5.6 | 5.6 |
| 1768 | 4.2 | 4.2 | 4.2 | 4.2 | 4.2 |
| 1686 | 7.4 | 7.4 | 7.4 | 7.4 | 7.4 |
| 1802 | 7.1 | 7.1 | 7.1 | 7.1 | 7.1 |
| 1811 | 142.9 | 142.9 | 100 «<\|nocaptions\|>» | 142.9 | 142.9 |
| 1971 | 36.4 | 36.4 | 100 «<\|nocaptions\|>» | 36.4 | 36.4 |
| 1930 | 0.0 | 0.0 | 0.0 | 0.0 | 7.1 |
| 1661 | 22.2 | 22.2 | 22.2 | 22.2 | 22.2 |
| 1906 | 6.2 | 6.2 | 6.2 | 6.2 | 6.2 |
| 1898 | 26.8 | 26.8 | 100 «<\|nocaptions\|>» | 26.8 | 31.7 |
| 2006 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 2001 | 50.0 | 50.0 | 100 «<\|nocaptions\|>» | 50.0 | 50.0 |
| 1721 | 31.6 | 31.6 | 31.6 | 31.6 | 26.3 |
| 1833 | 3.7 | 3.7 | 0.0 | 0.0 | 0.0 |
| **all 20, word errors %** | **20.1** | **20.1** | **44.7** | **20.1** | **20.4** |

- The converted GGUF and `ggml-model.bin` write the same text in transcribe.cpp on all 40 clips, clean and noisy: the
  conversion is faithful.
- transcribe.cpp and whisper.cpp (with `--suppress-nst`) write the same text on 38 of 40 clips. The two differences
  are single words on noisy clips, one won by each engine («Vi»/«Du» in 1970, «viks»/«miks» in 1833): small
  numerical differences between the two engines' kernels decide close calls.
- **whisper.cpp without `--suppress-nst` writes «<|nocaptions|>» for 9 of the 20 noisy clips** and drops the
  speech. NB-Whisper spells that marker as ordinary text (` <`, `|`, `no`, `ca`, `ptions`, `|`, `>`), so it is not
  the special no-speech token whisper.cpp always suppresses. The model's own `generation_config.json` lists those
  pieces in `suppress_tokens`; transcribe.cpp applies that list as Hugging Face does, and whisper.cpp applies it
  only with `--suppress-nst`. diktafon's whisper-server runs without that flag.

**Quantized files** against the F16 GGUF (number of clips with different text, and word errors over the 20):

| file | size | clean: differ | clean: errors | noisy: differ | noisy: errors |
|---|---:|---:|---:|---:|---:|
| GGUF F16 (converted) | 3.11 GB | – | 5.2 % | – | 20.1 % |
| NbAiLab `ggml-model.bin` (F16) | 3.10 GB | 0 | 5.2 % | 0 | 20.1 % |
| GGUF Q8_0 | 1.67 GB | 0 | 5.2 % | 1 | 20.4 % |
| GGUF Q5_K_M | 1.16 GB | 1 | 5.2 % | 6 | 19.4 % |
| NbAiLab `ggml-model-q5_0.bin` | 1.08 GB | 4 | 5.4 % | 9 | 20.8 % |

On these 40 clips no quantization changed the error rate by more than 0.7 points; Q8_0 is the closest to F16.
Decided in review: first run downloads NbAiLab's `ggml-model.bin`, pinned and hashed, from NbAiLab's repository;
Skald hosts no model file.

## 2. The vocabulary prompt

The test words are developer words NB-Whisper breaks without help. `say -v Nora` reads eight sentences («Jeg har
merget pull-requesten og kjørt migreringen mot PostgreSQL.», «Kan du rebase branchen før vi merger den til
main?», …), and a 97 s take spreads the same eight sentences among plain sentences. Most of them fall in the
second, third and fourth 30-second window. Two prompts, both written as design.md says (a sentence with inflected
forms, then words, the most important last), are in `research/engine-check/prompts/`:

- `ordskatt.txt`, 67 tokens: «Jeg har merget pull-requesten, rebaset branchen, pushet commiten og deployet releasen.
  Secret, main, Metal, Macen, SQLite, Kubernetes-clusteret, GitHub, NB-Whisper, Transcribe.cpp, PostgreSQL, Skald.»
- `ordskatt-long.txt`, 213 tokens: the same with 30 more tool names before the important ones, near the 223 limit.

A term counts when it is written exactly, as a whole word; at the start of a sentence its first letter may differ.
The configurations:

| name | what transcribe.cpp gets |
|---|---|
| none | no prompt |
| prompt | the Whisper extension's `initial_prompt`; it primes the first window only |
| prompt+condition | the same, and `condition_on_prev_tokens`: each window sees the text before it, starting with the prompt, which slides out once that text passes 223 tokens |
| prompt+carry | `prompt_condition = ALL_SEGMENTS`: the prompt heads every window, followed by up to 223 tokens of earlier text (transcribe.cpp accepts `ALL_SEGMENTS` only together with `condition_on_prev_tokens`) |
| …, capped | as carry, with `max_prev_context_tokens` = 223 − the prompt's tokens |
| wc … | whisper.cpp greedy with `--prompt` and `--carry-initial-prompt` (and `--suppress-nst`) |

**Short clips** (one window each, so the window rules do not matter), terms written right, timestamps off:

| clip | none | prompt | prompt+carry | long prompt+carry | long prompt, capped | wc prompt+carry | wc long prompt+carry |
|---|---|---|---|---|---|---|---|
| vocab-01 | 1/3 | 2/3 | 2/3 | 2/3 | 2/3 | 2/3 | 2/3 |
| vocab-02 | 1/4 | 2/4 | 2/4 | 1/4 | 1/4 | 1/4 | 1/4 |
| vocab-03 | 0/3 | 2/3 | 2/3 | 2/3 | 0/3 | 2/3 | 1/3 |
| vocab-04 | 0/3 | 0/3 | 0/3 | 0/3 | 1/3 | 0/3 | 0/3 |
| vocab-05 | 0/1 | 0/1 | 0/1 | 0/1 | 0/1 | 0/1 | 0/1 |
| vocab-06 | 1/3 | 2/3 | 2/3 | 2/3 | 1/3 | 2/3 | 2/3 |
| vocab-07 | 0/4 | 0/4 | 0/4 | 0/4 | 0/4 | 0/4 | 0/4 |
| vocab-08 | 1/2 | 2/2 | 2/2 | 2/2 | 1/2 | 1/2 | 1/2 |
| **total** | **4/23** | **10/23** | **10/23** | **9/23** | **6/23** | **8/23** | **7/23** |

prompt+condition and prompt+carry, capped write the same as prompt+carry here. With segment timestamps the totals
are the same except prompt and carry, 11/23. What the prompt changes, «none» → «prompt»:

- «Jeg har merget **Ulrekøsten** og kjørt migreringen mot **Kostgresskul**.» → «Jeg har merget **pull-requesten** og
  kjørt migreringen mot **postgreSQL**.»
- «**Hulrekuesten** feiler fordi kubenetes klusteret mangler en **sikret**.» → «**Pull-requesten** feiler fordi
  kubenetes-klusteret mangler en **secret**.»
- «Vi har **merket** den i går» → «Vi har **merget** den i går»; «Har du pushet **committen**» → «**commiten**».
- Unchanged: «**SKAL** setter inn teksten», «**Transribbe CPP** kjører **NB visper**», «**SQ Lite** til
  **PostgreSQ**». Nora pronounces these so that the model hears other words; that is what design.md's corrections
  («Rettinger») are for. A synthetic voice is a hard test for some words and an easy one for others; M3's recordings
  will tell more.

**The 97 s take**, terms written right by the window the sentence starts in, and word errors over the whole take:

| | terms | none | prompt | prompt+condition | prompt+carry | prompt+carry, capped | long prompt+carry | long prompt, capped | wc prompt+carry | wc long prompt+carry |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **timestamps off** | | | | | | | | | | |
| window 1 | 5 | 1 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| window 2 | 12 | 2 | 2 | 6 | 6 | 6 | 2 | 5 | 6 | 6 |
| window 3 | 6 | 1 | 1 | 2 | 3 | 3 | 1 | 2 | 3 | 2 |
| word errors % | | 26.7 | 26.7 | 16.2 | 16.2 | 15.4 | **60.3** | 17.4 | 15.0 | **10.5** |
| **segment timestamps** | | | | | | | | | | |
| window 1 | 5 | 2 | 2 | 2 | 2 | 2 | 3 | 2 | 2 | 2 |
| window 2 | 12 | 3 | 3 | 6 | 6 | 6 | 1 | 5 | 6 | 6 |
| window 3 | 6 | 2 | 2 | 2 | 2 | 3 | 1 | 2 | 3 | 2 |
| word errors % | | 18.2 | 19.0 | 11.3 | 18.6 | 15.8 | **61.1** | 17.4 | 15.0 | **10.5** |

(whisper.cpp ran with timestamps off in both halves; its columns repeat.)

- **A prompt on the first window only does nothing for the rest** («prompt» equals «none» from window 2 on).
- **Carrying works with the 67-token prompt**: window 2 goes from 2 to 6 of 12 terms and window 3 from 1 to 3 of 6,
  as in whisper.cpp.
- **Carrying the 213-token prompt breaks the take in transcribe.cpp.** From the second window on, the text falls
  apart: «Torsdag hadde vi et langt-push-committen til gitt-hub.no. Jeg trenger også hjelprist myndt mynd Ellerst.
  passet». The library reported no error. The reason is in `src/arch/whisper/model.cpp`: with `ALL_SEGMENTS` each
  window's prefix is the marker, the prompt (up to 223 tokens) and then up to 223 tokens of earlier text, as in
  Hugging Face's `prompt_condition_type="all-segments"`. That is up to 447 of the decoder's 448 positions before the
  window's own text. Even the 67-token prompt with segment timestamps dropped three sentences of window 3 (18.6 %).
  whisper.cpp's `carry_initial_prompt` takes the prompt first and fills only the rest of 224 tokens with earlier
  text, and handles the long prompt best of all (10.5 %).
- **Capping the earlier text is a Skald-side workaround with a limit.** `max_prev_context_tokens` = 223 − prompt
  keeps everything within whisper.cpp's budget, and the 67-token prompt then does as well as whisper.cpp (15.4 %,
  3 of 6 in window 3). But transcribe.cpp caps the prompt itself at the same number. A prompt over 111 tokens
  loses its head: the 213-token prompt kept its last 10 tokens («…PostgreSQL, Skald.»). It still helped a little,
  because the most important names come last.
- **A sentence that straddles a window boundary can be lost, with or without timestamps.** With timestamps off,
  transcribe.cpp cuts the audio at exactly 30, 60 and 90 s, and «Det tok lengre tid enn ventet, …» (29.4–35.4 s) is
  missing in every transcribe.cpp configuration that otherwise works. With segment timestamps and the earlier text
  carried it survives, but «Vi bytter fra SQLite til PostgreSQL i neste release.» (58.2–62.6 s) goes instead; that
  is still the lowest word error rate here (11.3 %). whisper.cpp with the long prompt kept both. Most dictations are
  under 30 s, where this does not apply.

The fix belongs upstream, and either of two small changes in `whisper_run` would do: let `ALL_SEGMENTS` run
without `condition_on_prev_tokens` (the code path that puts only the prompt at each window's head already exists;
only the validation refuses it), or budget the earlier text as 223 minus the prompt, as whisper.cpp does. Decided
in review: Skald builds transcribe.cpp with this change as its own patch until upstream releases it.

**Counting the prompt's tokens.** NbAiLab's `tokenizer.json` has BPE dropout 0.2 set (left from training), so Hugging
Face's tokenizers library counts the same text differently from one run to the next: 206, 211 and 214 tokens for
one draft of the long prompt. With dropout off it counts exactly what transcribe.cpp's tokenizer counts. Skald's
meter should count with transcribe.cpp (`Model::tokenize`, text with a leading space), not with `tokenizer.json`.

## 3. Silero VAD, and takes without speech

Silero VAD v6.2 through the binding's `VadSession`, with Silero's defaults (threshold 0.5, segments of 250 ms or
less dropped, 100 ms of silence ends a segment, 30 ms padding). «No minimum» is the same run with segments of any
length kept.

**Takes with speech:**

| takes | length s | speech found ms | segments |
|---|---|---|---|
| 20 FLEURS clips | 6.8–17.6 | 4728–13204 (each clip ≥ 4.7 s) | 1–8 |
| 8 vocabulary sentences | 2.7–4.6 | 2573–4517 | 1 each |
| «Ja.» with 1 s of silence on each side | 2.39 | 444 | 1 |
| «Nei.» | 2.36 | 412 | 1 |
| «Takk.» | 2.46 | 476 | 1 |
| «OK.» | 2.66 | 732 | 1 |
| «Hei.» | 2.37 | 380 | 1 |

Every take with speech has at least 380 ms; «no minimum» finds the same in each (per take in
`results/vad.json`). Leading and trailing silence is cut: FLEURS clips start with 0.5–2.5 s before the speaker.

**Takes without speech**, VAD and what NB-Whisper writes for them:

| take | length s | speech found ms (no minimum) | highest frame p | NB-Whisper, no-speech gate off | no-speech p | avg log p | NB-Whisper, default gate | whisper.cpp, nothing suppressed |
|---|---|---|---|---|---|---|---|---|
| digital silence | 3.0 | 0 (0) | 0.009 | «Dessuten er det vanskelig å se hvordan de har det.» | 0.000 | −1.57 | «… og jeg vil at du skal føle det sånn.» | «<\|nocaptions\|>» |
| pink noise −60 dBFS | 3.0 | 0 (0) | 0.031 | «... og hennes eksklusive ansiktspersonlighet.» | 0.000 | −1.46 | «...» | «<\|nocaptions\|>» |
| pink noise −40 dBFS | 3.0 | 0 (0) | 0.027 | «protest Energi-spørsmål» | 0.000 | −2.54 | «Er det en svigermus?» | «<\|nocaptions\|>» |
| 50 Hz hum −40 dBFS | 3.0 | 0 (0) | 0.019 | «... og at det er vanskelig å forstå.» | 0.000 | −0.85 | «... og at det er vanskelig å forstå.» | «<\|nocaptions\|>» |
| breath-like burst | 3.0 | 0 (0) | 0.084 | «...» | 0.000 | −0.89 | «...» | «<\|nocaptions\|>» |
| click | 3.0 | 0 (0) | 0.023 | «... og en hull i skjæren.» | 0.000 | −1.51 | «Hva er det som er galt med huset?» | «<\|nocaptions\|>» |
| typing | 3.0 | 0 (0) | 0.035 | «Ja.» | 0.000 | −2.25 | «... ...» | «<\|nocaptions\|>» |
| room tone, FLEURS 1802 | 1.5 | 0 (0) | 0.103 | «...» | 0.000 | −6.27 | «Alisa Alóga lays науч report» | «<\|nocaptions\|>» |
| room tone, FLEURS 1970 | 1.5 | 0 (0) | 0.087 | «RISADAS» | 0.000 | −2.48 | «Viktig at det føres regnskap til store områder.» | «<\|nocaptions\|>» |
| room tone, FLEURS 2006 | 1.5 | 0 (0) | 0.032 | «Hva er det som skjer?» | 0.000 | −0.90 | «Hva er det som skjer?» | «<\|nocaptions\|>» |

- **Silero finds no speech in any of them**, not even with no minimum; no frame reaches 0.11.
- **NB-Whisper's no-speech probability is 0.000 on every one**, even on digital silence. transcribe.cpp's no-speech
  skip needs the probability above 0.6 (and the average log probability below −1.0), so it never fires. The
  sentences in the «default gate» column are what the temperature fallback produced instead. They change from run
  to run (the fallback samples).
- **NB-Whisper's own silence marker is the text «<|nocaptions|>»**, which it writes for all ten when nothing is
  suppressed, but also for 9 of the 20 noisy speech clips (section 1). transcribe.cpp suppresses it, as the model's
  `generation_config.json` asks.

**For the 250 ms rule** (design.md, Silence): it holds with room on these takes, from 0 ms in every take without
speech to at least 380 ms in the shortest real word. Two details for M1b:

- Silero's own minimum already drops each segment of 250 ms or less. Skald's rule is about the take, so Skald should
  add up the segments of a run with Silero's defaults and compare the sum with 250 ms. A take of several very short
  bursts then counts as nothing, which is what the rule wants.
- The breath, click and typing here are synthetic, and the room tone comes from three studio-like FLEURS recordings.
  M3's recordings with real microphones are the real test.

What this means for design.md's four checks: level and VAD work; **the third check, the model's doubt, does not work
with NB-Whisper** (nor with Bokfink, which has no such probability; [bokfink-gguf.md](bokfink-gguf.md)); the fourth,
known phrases, cannot catch what NB-Whisper invents here, because it is a different sentence each time. Decided in
review: the third check is dropped (D-22). Level, VAD and known phrases carry the gate, and a model whose
probability works may add the check back.

## 4. The Rust binding

The plan named two options, transcribe-rs or our own binding over transcribe.cpp's C API. There is a third, and it
is the one to take: **transcribe.cpp's own Rust crates**, `transcribe-cpp` (safe API), `transcribe-cpp-sys` (raw
FFI) and `transcribe-cpp-types` (plain data), published from the transcribe.cpp repository in lockstep with each
release.

| What Skald needs | transcribe-rs 0.3.12 | transcribe-cpp 0.5.1 | Our own over the C API |
|---|---|---|---|
| Uses transcribe.cpp (D-4) | **No.** Whisper through whisper-rs 0.16 (whisper.cpp); other models through ONNX Runtime | Yes, the same library at the same version | Yes |
| NB-Whisper as GGUF or ggml `.bin` | `.bin` through whisper.cpp | Both (section 1) | Both |
| Vocabulary prompt carried into every window | Initial prompt only; whisper-rs has no setter for `carry_initial_prompt` (D-4) | `initial_prompt` and `condition_on_prev_tokens` in the safe API; `prompt_condition` (`ALL_SEGMENTS`) only through `-sys` | Everything in the C API |
| Silero VAD | Silero v4 through ONNX Runtime | `VadSession` in the safe API (used for section 3) | Yes |
| No-speech probability per window | whisper.cpp's | Only through `-sys` (`transcribe_get_whisper_chunk_trace`); the safe `Session` has no raw handle, so a caller who needs it uses `-sys` for the whole Whisper session | Yes |
| Beam search 5 | Yes (whisper.cpp) | No: transcribe.cpp 0.5.1 has none for Whisper | No, same library |
| Qwen3-ASR for Bokfink | Not released; an unmerged branch from March 2026 runs stock Qwen3-ASR through ONNX, without Bokfink's tokenizer or prefixes | Yes, same `run` call; needs the language-name patch ([bokfink-gguf.md](bokfink-gguf.md)) | Yes, same patch |
| Metal, Vulkan, CUDA builds | Metal, Vulkan, CUDA for whisper.cpp; a second set of providers for ONNX Runtime | Cargo features `metal` (default), `vulkan`, `cuda`, `rocm`; `shared` and `dynamic-backends` for runtime-loaded backends | We write the build script |
| How current it stays | 0.3.12 on 2026-10-02; follows whisper-rs | Same commit as the C library; 28 releases since June 2026; the FFI is generated and checked against the C header's ABI hash in CI | We regenerate on every update |
| Licence, size | MIT; whisper-rs, ort, ndarray, ureq optional | MIT; `thiserror`, `log`; builds the vendored C++ with CMake | MIT; bindgen at build or generated code kept by us |

**Why.** transcribe-rs is not a binding to transcribe.cpp: taking it would mean whisper.cpp plus ONNX Runtime, two
runtimes where D-4 chose one, and no Bokfink. Our own binding would redo what `transcribe-cpp-sys` already is: FFI
generated from the same header, checked against the header's ABI hash, released with the library. The safe crate
built here with Metal on the first try, ran every model in this report, and its VAD API was used as is. Two things
Skald needs are missing from the safe API, both small, and both available through `-sys` today: `prompt_condition`
in `WhisperRunOptions`, and the per-window decoding trace (no-speech probability, average log probability) on
`Session`. Skald's engine crate uses the safe API, and `-sys` for those two until the binding has them; whether we
offer them upstream is question 2 below. Pin both crates exactly (`=0.5.1`), since they move with the C library.

The decision is D-22 in [decisions.md](../decisions.md).

### Beam search: three ways, and the one recommended

D-16 says Skald gets beam search 5 from the start; the M1b item says «with beam search 5». transcribe.cpp 0.5.1 has
no beam search for Whisper (the family notes list it as a later step; there is no open issue or pull request for
it). The options:

| | What it takes | What it costs | What it gives |
|---|---|---|---|
| **a. Beam search in transcribe.cpp** (upstream pull request, or a patch Skald carries) | Beam search in Whisper's decode loop in `src/arch/whisper/model.cpp` (the greedy loop, temperature fallback, KV cache per beam, timestamps, the batch path), checked against Hugging Face's `num_beams=5` the way the project checks everything | The largest piece of work: several days with the parity tests, and a patch to carry if upstream does not take it. At run time about 13 % more (whisper.cpp: 944 → 1071 ms for 9.24 s) | Beam 5 in the one engine |
| **b. whisper.cpp for Whisper models**, transcribe.cpp for Qwen3-ASR and VAD | A whisper.cpp binding next to transcribe.cpp. Each vendors its own copy of ggml; linked statically into one program their ggml symbols clash, so one of them goes in a shared library or both share one ggml | Two engines, two Metal/Vulkan/CUDA build matrices, a larger app; whisper-rs lags whisper.cpp and lacks a safe `carry_initial_prompt` setter (D-4); `suppress_nst` must be set or «<\|nocaptions\|>» gets through | Beam 5 today; whisper.cpp's prompt budgeting (better with a long prompt, section 2) |
| **c. Greedy until M3 measures it** | Nothing now. M3 already compares greedy and beam 5 (plan.md); since the engines write the same text greedy, the beam column can come from whisper.cpp on the same recordings | D-16 and the M1b item change their wording | Beam 5 only if Skald's own recordings show it helps; then option a, upstream |

**Recommended: c.** On the evidence here beam 5 does not pay. On 20 clean clips it wrote exactly what greedy wrote.
On 20 noisy clips it changed 5: better in 3 («Nordøstersjøen», «eget visum», «spredte»), worse in 1
(«fotogratene»), mixed in 1, and 20.4 % word errors against 20.1 % greedy. The one split compound in those clips
(«skips sekskursjoner») stayed split. That is too little to justify a second engine (b) or days of decoder work (a)
before M3. Option c keeps transcribe.cpp as the only engine, so the binding above holds for every model. If Thomas
picks b, the binding for Whisper models changes with it.

## 5. Time to transcribe 9 s of speech

FLEURS clip 1994, 9.24 s with 5.1 s of speech («Hvis du allerede kan et romansk språk, vil det være lettere for deg å
lære portugisisk.», the same text from every engine and file). Model loaded first, two warm-up runs, then 10 timed
runs; wall clock around `transcribe_run` (transcribe.cpp) or around the HTTP request to a whisper-server of our own
on another port (whisper.cpp, default 4 threads; diktafon's own server was left alone). Each series held the GPU
lock, and no other transcription process ran. The Mac was not otherwise idle: Rust builds of other work were running,
and the load average is given for each series.

**Series 1**, 2026-11-27 15:57, load average 4.49 at the start (7.39 at the end):

| engine, file, decoding | median ms | range ms | faster than real time | encode ms | decode ms |
|---|---:|---:|---:|---:|---:|
| tc, GGUF F16, greedy | **834** | 830–837 | 11.1× | 443 | 389 |
| tc, GGUF Q8_0, greedy | 766 | 764–768 | 12.1× | 462 | 301 |
| tc, GGUF Q5_K_M, greedy | 805 | 804–807 | 11.5× | 498 | 302 |
| wc, `ggml-model.bin`, greedy | 944 | 942–948 | 9.8× | | |
| wc, `ggml-model.bin`, beam 5 | **1071** | 1054–1104 | 8.6× | | |

**Series 2**, 16:04, load average 6.39 at the start (4.49 at the end), adding NbAiLab's files in transcribe.cpp:

| engine, file, decoding | median ms | range ms | faster than real time |
|---|---:|---:|---:|
| tc, GGUF F16, greedy | 872 | 870–875 | 10.6× |
| tc, GGUF Q8_0, greedy | 795 | 771–797 | 11.6× |
| tc, GGUF Q5_K_M, greedy | 813 | 812–815 | 11.4× |
| tc, `ggml-model.bin`, greedy | 844 | 843–850 | 11.0× |
| tc, `ggml-model-q5_0.bin`, greedy | 782 | 780–786 | 11.8× |
| wc, `ggml-model.bin`, greedy | 951 | 948–953 | 9.7× |
| wc, `ggml-model.bin`, beam 5 | 1067 | 1065–1070 | 8.7× |

- Greedy transcribe.cpp is 11–12 % faster than greedy whisper.cpp with the same file; beam 5 costs whisper.cpp
  12–13 % more than greedy. The heavier load in series 2 cost transcribe.cpp's F16 run 4.5 %.
- The encoder takes about half the time whatever the file; quantization saves on the decoder.
- Loading the F16 model took 3.8 s the first time and 0.1–0.7 s once the file was in the page cache. The first
  run after loading took up to 20 % longer than the median. CLAUDE.md's rule stands: load once, keep it loaded.
- diktafon's prototype measured 1.1 s for 9 s of speech on the same chip (CLAUDE.md); the 834–944 ms here are in
  line with that.

## What works and what does not

Works:

- NB-Whisper Large converted to GGUF, or NbAiLab's `ggml-model.bin` loaded as is, on Metal, with the same text as
  whisper.cpp (40 of 40 clips between the two files, 38 of 40 between the engines).
- The vocabulary prompt as initial prompt; carried into every window with a prompt of up to 111 tokens and the
  earlier text capped.
- Silero VAD v6.2 in transcribe.cpp, from Rust, on the CPU: no speech found in any silent take, at least 380 ms in
  every take with speech.
- The model's `suppress_tokens` keep «<|nocaptions|>» out of the text, where whisper.cpp needs `--suppress-nst`.
- `transcribe-cpp` 0.5.1 builds with Metal and runs all of it.

Does not work, or not yet:

- Beam search: transcribe.cpp has none for Whisper.
- Carrying a prompt over 111 tokens into every window: it breaks long takes (61 % word errors with 213 tokens) in
  stock transcribe.cpp 0.5.1. The fix is small, and Skald's build carries it as a patch.
- NB-Whisper's no-speech probability: 0.000 on everything, so the check is dropped from the silence gate (D-22).
- A sentence that straddles a 30-second boundary can be lost, with timestamps off or on.
- `prompt_condition` and the per-window trace are not in the safe Rust API.

## What M1b needs from the engine

1. **`skald-engine` on `transcribe-cpp` and `transcribe-cpp-sys`, both `=0.5.1`, feature `metal`.** The safe API for
   loading, sessions and VAD; `-sys` for the Whisper run extension's `prompt_condition` and
   `max_prev_context_tokens`, and for the window trace, until the safe crate has them.
2. **transcribe.cpp built with Skald's own patches until upstream releases them** (decided in review): the budget
   for a prompt carried into every window (section 2), and Bokfink's language names
   ([bokfink-gguf.md](bokfink-gguf.md)). `transcribe-cpp-sys` builds the C++ it vendors, so the patched build
   either replaces that copy (a patched copy of the crate through `[patch.crates-io]`) or points the crate at a
   patched transcribe.cpp installed elsewhere (`TRANSCRIBE_DIR`). The patches would also be offered upstream if
   Thomas agrees (question 2 below).
3. **The model file** (decided in review): NbAiLab's `ggml-model.bin` (3.10 GB, SHA-256 `0f2f66f2…`) from
   NbAiLab's repository at revision `8c6249fd…`, which transcribe.cpp loads directly. It gives the reference text,
   and Skald hosts no model file (design.md, First run).
4. **Decoding:** language `no` for bokmål and `nn` for nynorsk; greedy (temperature 0 with the default fallback)
   unless Thomas keeps beam 5, which transcribe.cpp cannot do yet; timestamps off for takes up to 30 s. For longer
   takes, segment timestamps with the earlier text carried gave the fewest errors, but each mode lost one sentence
   at a window boundary (section 2); M3's long recordings should settle it.
5. **The prompt:** Whisper's `initial_prompt`, never the generic `prompt`/`vocabulary` fields: those fit terms by
   dropping from the end of the list, and design.md puts the most important names last. Carry it into every window
   with `ALL_SEGMENTS` and `condition_on_prev_tokens`. With the budget patch in the build, the meter keeps its 223
   tokens. Only while the patch is not in does the engine cap: `max_prev_context_tokens` = 223 − prompt tokens for
   prompts up to 111 tokens, and `condition_on_prev_tokens` alone, without carrying, above that.
6. **Token counting for the meter:** `skald-core` takes a `CountTokens`; the engine implements it with
   `Model::tokenize(" " + text)`, not with NbAiLab's `tokenizer.json` (dropout).
7. **The silence gate** (design.md, Silence): level, then Silero VAD with Silero's defaults, the segments added up
   and compared with 250 ms, then the model only if there is enough speech, then known phrases. The model's
   no-speech check is dropped (D-22). Known phrases help with Bokfink's «Det er.»
   ([bokfink-gguf.md](bokfink-gguf.md)), not with what NB-Whisper invents.
8. **Per model, from the Bokfink report:** language codes per model, vocabulary as a sentence for Whisper and a term
   list for Bokfink, control tags removed from Bokfink's text, no 30-second windows for Qwen3-ASR, and the
   language-name patch to transcribe.cpp. All of it goes through the same `transcribe_run`, so none of it changes
   the binding.

## Decided in review

The lead settled three of the questions this report raised; D-22 records them.

- **The no-speech check is dropped** from design.md, Silence. It never fires with NB-Whisper (0.000 on every silent
  take) or with Bokfink, which has no such probability. Level, VAD and known phrases carry the gate. A model whose
  probability works may add the check back.
- **Skald builds transcribe.cpp with its own patches until upstream releases them:** the budget for a carried
  prompt (section 2) and Bokfink's language names. The ordskatt meter keeps its 223 tokens; the engine caps a
  carried prompt at 111 tokens only while the budget patch is not in the build.
- **First run downloads NbAiLab's `ggml-model.bin`** at revision `8c6249fd…`, SHA-256 `0f2f66f2…`: it gives the
  reference text, the National Library hosts it, and Skald hosts no model file (design.md, First run).

## Questions for Thomas

1. **Beam search 5 (D-16, the M1b item).** transcribe.cpp cannot do it. Recommended: greedy now, and let M3 compare
   greedy with beam 5 on Skald's recordings (the beam runs through whisper.cpp), with beam search added to
   transcribe.cpp upstream only if it helps. The alternatives are writing beam search into transcribe.cpp first, or a
   second engine (whisper.cpp) for Whisper models. Agree to change D-16 and the M1b item accordingly?
2. **Upstream.** May the two changes (transcribe.cpp: the carried prompt's budget; the Rust binding:
   `prompt_condition` and the window trace) be offered as pull requests to handy-computer/transcribe.cpp from
   Thomas's account?

## Credits

- NB-Whisper Large by the National Library of Norway (Nasjonalbiblioteket, NbAiLab), Apache 2.0.
- FLEURS by Google (Conneau et al., 2022, «FLEURS: Few-shot Learning Evaluation of Universal Representations of
  Speech»), CC BY 4.0; the 20 test sentences and their reference texts appear in the tables and in
  `research/engine-check/results/`.
- Silero VAD by the Silero Team, MIT; its GGUF from handy-computer.
- transcribe.cpp and its Rust crates by handy-computer, MIT; whisper.cpp by the ggml authors, MIT.
- The vocabulary takes are synthetic speech from macOS's Nora voice.

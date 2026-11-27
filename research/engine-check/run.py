#!/usr/bin/env python3
"""Run the NB-Whisper and Silero VAD checks and print the report's tables.

Each step writes its raw results as JSON to --results and prints Markdown.
Python standard library only. The engines are external programs:

    --probe        engine-probe, built from research/engine-check/probe
    --whisper-cli  whisper.cpp's CLI (Homebrew whisper-cpp)
    --whisper-server  whisper.cpp's server, for timing with the model loaded

Steps (see docs/research/engine-check.md for the commands that produced it):

    parity   transcribe.cpp (converted GGUF, and NbAiLab's ggml-model.bin
             loaded directly) against whisper.cpp greedy and beam 5
    prompt   the vocabulary prompt: none, first window only, conditioned on
             earlier text, carried into every window; short clips and a
             97 s take
    vad      Silero VAD on speech and on takes without speech, and what
             NB-Whisper writes for the takes without speech
    speed    time to transcribe one clip with the model loaded, median of runs
"""

from __future__ import annotations

import argparse
import json
import re
import signal
import statistics
import subprocess
import sys
import time
import urllib.request
import uuid
from pathlib import Path

# Words the vocabulary prompt teaches, per vocabulary clip. A term counts when
# it appears as a whole word, written exactly; at the start of a sentence its
# first letter may also differ in case.
TERMS = {
    "vocab-01": ["merget", "pull-requesten", "PostgreSQL"],
    "vocab-02": ["rebase", "branchen", "merger", "main"],
    "vocab-03": ["Pull-requesten", "Kubernetes-clusteret", "secret"],
    "vocab-04": ["SQLite", "PostgreSQL", "release"],
    "vocab-05": ["Skald"],
    "vocab-06": ["pushet", "commiten", "GitHub"],
    "vocab-07": ["Transcribe.cpp", "NB-Whisper", "Metal", "Macen"],
    "vocab-08": ["merget", "deployen"],
}
WINDOW_S = 30.0


# ---------------------------------------------------------------- text scoring


def normalize(text: str) -> list[str]:
    """Lowercase words without punctuation; hyphens and decimal points stay."""
    text = text.lower()
    text = re.sub(r"(?<!\d)[.,](?!\d)|[.,](?=\s|$)", " ", text)
    text = re.sub(r"[!?;:«»\"“”()\[\]]", " ", text)
    return text.split()


def word_errors(reference: str, hypothesis: str) -> tuple[int, int]:
    ref, hyp = normalize(reference), normalize(hypothesis)
    row = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        prev, row[0] = row[0], i
        for j, h in enumerate(hyp, 1):
            prev, row[j] = row[j], min(row[j] + 1, row[j - 1] + 1, prev + (r != h))
    return row[len(hyp)], len(ref)


def wer(pairs: list[tuple[str, str]]) -> float:
    errors = words = 0
    for ref, hyp in pairs:
        e, n = word_errors(ref, hyp)
        errors += e
        words += n
    return 100.0 * errors / max(words, 1)


def term_found(term: str, text: str) -> bool:
    if re.search(rf"(?<![\w-]){re.escape(term)}(?![\w-])", text):
        return True
    flipped = term[0].swapcase() + term[1:]
    return re.search(rf"(?:^|[.!?:]\s+|[«\"]){re.escape(flipped)}(?![\w-])", text) is not None


def reference(wav: Path) -> str:
    return wav.with_suffix(".txt").read_text(encoding="utf-8").strip()


# ---------------------------------------------------------------- engines


def probe(args, model: str, wavs: list[Path], *extra: str) -> list[dict]:
    if not getattr(args, "holds_gpu_lock", False):
        wait_for_gpu(args)
    cmd = [args.probe, "asr", "--model", model, *extra, *map(str, wavs)]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return [json.loads(line) for line in out.stdout.splitlines() if line.startswith("{")]


def probe_vad(args, wavs: list[Path], *extra: str) -> list[dict]:
    wait_for_gpu(args)
    cmd =[args.probe, "vad", "--model", args.vad, *extra, *map(str, wavs)]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return [json.loads(line) for line in out.stdout.splitlines() if line.startswith("{")]


def prompt_tokens(args, files: list[str]) -> list[int]:
    wait_for_gpu(args)
    cmd =[args.probe, "tokens", "--model", args.gguf, *files]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return [int(json.loads(line)["tokens"]) for line in out.stdout.splitlines() if line.startswith("{")]


def whisper_cpp(args, wav: Path, *extra: str) -> str:
    wait_for_gpu(args)
    cmd =[args.whisper_cli, "-m", args.bin, "-l", "no", "-nt", "-np", *extra, str(wav)]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return " ".join(line.strip() for line in out.stdout.splitlines() if line.strip())


def tc_texts(rows: list[dict]) -> dict[str, str]:
    return {row["file"]: row["text"] for row in rows}


def save(args, name: str, data) -> None:
    results = Path(args.results)
    results.mkdir(parents=True, exist_ok=True)
    (results / f"{name}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


def md_table(header: list[str], rows: list[list]) -> str:
    lines = ["| " + " | ".join(header) + " |", "|" + "|".join("---" for _ in header) + "|"]
    lines += ["| " + " | ".join(str(c) for c in row) + " |" for row in rows]
    return "\n".join(lines)


# ---------------------------------------------------------------- steps


def step_parity(args) -> None:
    wavs = sorted(Path(args.clips).glob(args.pattern))
    order = fleurs_order(Path(args.fleurs or args.clips))
    wavs = sorted(wavs, key=lambda p: order.index(p.stem.rsplit("-", 1)[-1]))
    configs = {
        "tc-gguf": tc_texts(probe(args, args.gguf, wavs, "--timestamps", "none")),
        "tc-bin": tc_texts(probe(args, args.bin, wavs, "--timestamps", "none")),
        "wc-greedy": {w.stem: whisper_cpp(args, w, "-bs", "1") for w in wavs},
        "wc-greedy-sns": {w.stem: whisper_cpp(args, w, "-bs", "1", "--suppress-nst") for w in wavs},
        "wc-beam5-sns": {w.stem: whisper_cpp(args, w, "-bs", "5", "--suppress-nst") for w in wavs},
    }
    refs = {w.stem: reference(w) for w in wavs}
    save(args, args.name or "parity", {"references": refs, "texts": configs})

    names = list(configs)
    rows = []
    for w in wavs:
        k = w.stem
        cells = [f"{100.0 * word_errors(refs[k], configs[c][k])[0] / word_errors(refs[k], '')[1]:.1f}" for c in names]
        same = "yes" if configs["tc-gguf"][k] == configs["wc-greedy-sns"][k] else "no"
        same_bin = "yes" if configs["tc-gguf"][k] == configs["tc-bin"][k] else "no"
        same_beam = "yes" if configs["wc-greedy-sns"][k] == configs["wc-beam5-sns"][k] else "no"
        rows.append([k.rsplit("-", 1)[-1], *cells, same_bin, same, same_beam])
    for label, subset in (("first 10", wavs[:10]), ("all 20", wavs)):
        rows.append([f"**WER, {label}**", *[f"**{wer([(refs[w.stem], configs[c][w.stem]) for w in subset]):.1f}**" for c in names], "", "", ""])
    print(md_table(["clip", *names, "tc-gguf = tc-bin", "tc-gguf = wc-greedy-sns", "greedy = beam5"], rows))
    print()
    for w in wavs:
        k = w.stem
        if len({configs[c][k] for c in names}) > 1:
            print(f"- {k}: ref «{refs[k]}»")
            for c in names:
                print(f"  - {c}: «{configs[c][k]}»")


def fleurs_order(clips: Path) -> list[str]:
    """FLEURS sentence ids in table order, the order fetch.py takes them in."""
    tsv = clips / ".download" / "test.tsv"
    order: list[str] = []
    for line in tsv.read_text(encoding="utf-8").splitlines():
        sentence_id = line.split("\t")[0]
        if sentence_id not in order:
            order.append(sentence_id)
    return order


def step_prompt(args) -> None:
    takes = Path(args.takes)
    short = sorted(takes.glob("vocab-0*.wav"))
    long_take = takes / "vocab-long.wav"
    prompt = Path(args.prompt).read_text(encoding="utf-8").strip()
    long_prompt = Path(args.long_prompt).read_text(encoding="utf-8").strip()
    ts = ["--timestamps", args.timestamps]

    # "capped": earlier text limited so that marker + prompt + earlier text fit
    # the 224 tokens whisper.cpp allows; the library also caps the prompt
    # itself at that limit, so a prompt over 111 tokens loses its head.
    n_prompt, n_long = prompt_tokens(args, [args.prompt, args.long_prompt])
    configs = {
        "none": [],
        "prompt": ["--prompt", prompt],
        "prompt+condition": ["--prompt", prompt, "--condition"],
        "prompt+carry": ["--prompt", prompt, "--carry"],
        "prompt+carry, capped": ["--prompt", prompt, "--carry", "--max-prev", str(223 - n_prompt)],
        "long prompt+carry": ["--prompt", long_prompt, "--carry"],
        "long prompt+carry, capped": ["--prompt", long_prompt, "--carry", "--max-prev", str(max(1, 223 - n_long))],
    }
    results = {}
    for name, extra in configs.items():
        results[name] = probe(args, args.gguf, [*short, long_take], *ts, *extra)
    for name, text in (("wc prompt+carry", prompt), ("wc long prompt+carry", long_prompt)):
        wc_flags = ["-bs", "1", "--suppress-nst", "--prompt", text, "--carry-initial-prompt"]
        results[name] = [{"file": w.stem, "text": whisper_cpp(args, w, *wc_flags)} for w in [*short, long_take]]
    save(args, args.name or "prompt", results)

    # Short clips: terms found per configuration.
    names = list(results)
    rows = []
    for w in short:
        k = w.stem
        row = [k]
        for name in names:
            text = next(r["text"] for r in results[name] if r["file"] == k)
            row.append(f"{sum(term_found(t, text) for t in TERMS[k])}/{len(TERMS[k])}")
        rows.append(row)
    totals = ["**total**"]
    for name in names:
        hits = sum(
            term_found(t, next(r["text"] for r in results[name] if r["file"] == w.stem))
            for w in short
            for t in TERMS[w.stem]
        )
        totals.append(f"**{hits}/{sum(len(TERMS[w.stem]) for w in short)}**")
    rows.append(totals)
    print("Short clips, terms written right:\n")
    print(md_table(["clip", *names], rows))
    print()
    for name in names:
        print(f"- {name}:")
        for w in short:
            print(f"  - {w.stem}: «{next(r['text'] for r in results[name] if r['file'] == w.stem)}»")

    # Long take: terms found per 30-second window of the vocabulary sentence's start.
    layout = json.loads(long_take.with_suffix(".json").read_text(encoding="utf-8"))
    vocab_sentences = [s for s in layout if s["vocab"]]
    sentence_terms = []
    for s in vocab_sentences:
        clip = next(w.stem for w in short if reference(w) == s["text"])
        sentence_terms.append((int(s["start_s"] // WINDOW_S) + 1, clip, TERMS[clip]))
    print("\nLong take (97 s), terms written right, by the window the sentence starts in:\n")
    rows = []
    for window in sorted({w for w, _, _ in sentence_terms}):
        terms = [t for w, _, ts_ in sentence_terms if w == window for t in ts_]
        row = [f"window {window}", len(terms)]
        for name in names:
            text = next(r["text"] for r in results[name] if r["file"] == "vocab-long")
            row.append(sum(term_found(t, text) for t in terms))
        rows.append(row)
    wers = ["WER", ""]
    for name in names:
        text = next(r["text"] for r in results[name] if r["file"] == "vocab-long")
        wers.append(f"{wer([(reference(long_take), text)]):.1f}")
    rows.append(wers)
    print(md_table(["", "terms", *names], rows))
    print()
    for name in names:
        row = next(r for r in results[name] if r["file"] == "vocab-long")
        print(f"- {name} ({row.get('status', 'whisper.cpp')}): «{row['text']}»")
        if "windows" in row:
            print(f"  windows: {[(w['t0_ms'], w['t1_ms'], w['temperature']) for w in row['windows']]}")


def step_vad(args) -> None:
    clips = sorted(Path(args.clips).glob("fleurs-nb-*.wav"))
    takes = Path(args.takes)
    vocab = sorted(takes.glob("vocab-0*.wav")) + sorted(takes.glob("word-*.wav"))
    silent = sorted(takes.glob("silent-*.wav"))
    everything = [*clips, *vocab, *silent]
    default = {r["file"]: r for r in probe_vad(args, everything)}
    raw = {r["file"]: r for r in probe_vad(args, everything, "--min-speech-ms", "0")}
    asr = {r["file"]: r for r in probe(args, args.gguf, silent, "--timestamps", "none")}
    gate_off = ["--no-speech-thold", "inf"]
    asr_off = {r["file"]: r for r in probe(args, args.gguf, silent, "--timestamps", "none", *gate_off)}
    unsuppressed = {w.stem: whisper_cpp(args, w, "-bs", "1") for w in silent}
    save(args, "vad", {"default": default, "min_speech_0": raw, "asr_silent": asr, "asr_silent_gate_off": asr_off,
                       "whisper_cpp_unsuppressed_silent": unsuppressed})

    def row(k: str) -> list:
        d, r = default[k], raw[k]
        return [k, f"{d['audio_s']:.2f}", f"{abs(d['speech_ms']):.0f}", len(d["segments_ms"]),
                f"{abs(r['speech_ms']):.0f}", f"{d['max_prob']:.3f}"]

    header = ["take", "length s", "speech ms", "segments", "speech ms, no minimum", "max p"]
    print("Speech:\n")
    print(md_table(header, [row(w.stem) for w in [*clips, *vocab]]))
    speech = [default[w.stem]["speech_ms"] for w in [*clips, *vocab]]
    print(f"\nShortest speech found in a speech take: {min(speech):.0f} ms\n")
    print("Without speech:\n")
    rows = []
    for w in silent:
        k = w.stem
        a = asr[k]
        win = a["windows"][0] if a["windows"] else {}
        rows.append(row(k) + [
            f"«{asr_off[k]['text']}»",
            f"{win.get('no_speech_prob', float('nan')):.3f}",
            f"{win.get('avg_logprob', float('nan')):.2f}",
            f"«{a['text']}»",
            f"«{unsuppressed[k]}»",
        ])
    extra = ["NB-Whisper, gate off", "no-speech p", "avg logprob", "NB-Whisper, default gate",
             "whisper.cpp, nothing suppressed"]
    print(md_table(header + extra, rows))


ENGINE_PROCESSES = ["whisper-cli", "transcribe-cli", "transcribe-bench", "engine-probe"]


def wait_for_gpu(args) -> None:
    """GPU work other than timing waits while a timing series holds the lock."""
    if args.gpu_lock:
        while Path(args.gpu_lock).exists():
            time.sleep(5)


class GpuTimingLock:
    """The shared lock directory, taken with mkdir (atomic) for a timing series."""

    def __init__(self, path: str | None):
        self.path = Path(path) if path else None

    def __enter__(self):
        while self.path:
            try:
                self.path.mkdir()
                break
            except FileExistsError:
                time.sleep(5)
        while busy := [name for name in ENGINE_PROCESSES if subprocess.run(["pgrep", "-x", name], capture_output=True).returncode == 0]:
            print(f"waiting for {', '.join(busy)} to finish", file=sys.stderr)
            time.sleep(5)
        return self

    def __exit__(self, *_):
        if self.path:
            self.path.rmdir()


def load_average() -> str:
    return subprocess.run(["sysctl", "-n", "vm.loadavg"], capture_output=True, text=True).stdout.strip()


def step_speed(args) -> None:
    with GpuTimingLock(args.gpu_lock):
        args.holds_gpu_lock = True
        timed_series(args)


def timed_series(args) -> None:
    clip = Path(args.clip)
    load_at_start = load_average()
    started = time.strftime("%Y-%m-%d %H:%M:%S")
    results: dict[str, dict] = {}
    for label, model in (("tc greedy, GGUF F16", args.gguf), *((f"tc greedy, {Path(m).name}", m) for m in args.extra_gguf)):
        row = probe(args, model, [clip], "--timestamps", "none", "--warmup", str(args.warmup), "--repeat", str(args.repeat))[0]
        results[label] = {"runs_ms": row["runs_ms"], "median_ms": row["median_ms"], "load_ms": row.get("load_ms"),
                          "first_run_ms": row.get("first_run_ms"), "encode_ms": row["encode_ms"], "decode_ms": row["decode_ms"],
                          "text": row["text"]}
    results.update(whisper_server_times(args, clip))
    load_at_end = load_average()
    save(args, args.name or "speed", {"clip": clip.stem, "started": started, "load_average_start": load_at_start,
                                      "load_average_end": load_at_end, "results": results})
    audio_s = probe_duration(clip)
    rows = []
    for label, r in results.items():
        runs = r["runs_ms"]
        rows.append([label, f"{r['median_ms']:.0f}", f"{min(runs):.0f}–{max(runs):.0f}", f"{audio_s * 1000 / r['median_ms']:.1f}×"])
    print(f"{clip.stem}, {audio_s:.2f} s of audio, {args.repeat} runs after {args.warmup} warm-up runs, "
          f"started {started}; load average {load_at_start} at the start, {load_at_end} at the end:\n")
    print(md_table(["engine, decoding", "median ms", "range ms", "faster than real time"], rows))
    for label, r in results.items():
        print(f"- {label}: «{r['text']}»")


def probe_duration(wav: Path) -> float:
    import wave

    with wave.open(str(wav), "rb") as w:
        return w.getnframes() / w.getframerate()


def whisper_server_times(args, clip: Path) -> dict[str, dict]:
    """whisper.cpp with the model loaded: our own whisper-server on a free port."""
    port = args.port
    server = subprocess.Popen(
        [args.whisper_server, "-m", args.bin, "-l", "no", "--host", "127.0.0.1", "--port", str(port)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        deadline = time.time() + 120
        while time.time() < deadline:
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=1)
                break
            except OSError:
                time.sleep(0.5)
        audio = clip.read_bytes()
        out = {}
        for label, beam in (("wc greedy, ggml-model.bin", "1"), ("wc beam 5, ggml-model.bin", "5")):
            fields = {"beam_size": beam, "no_timestamps": "true", "response_format": "json", "temperature": "0.0"}
            runs, text = [], ""
            for i in range(args.warmup + args.repeat):
                t = time.perf_counter()
                text = post_inference(port, audio, clip.name, fields)
                if i >= args.warmup:
                    runs.append((time.perf_counter() - t) * 1000)
            out[label] = {"runs_ms": [round(r, 1) for r in runs], "median_ms": statistics.median(runs), "text": text}
        return out
    finally:
        server.send_signal(signal.SIGINT)
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()


def post_inference(port: int, audio: bytes, filename: str, fields: dict[str, str]) -> str:
    boundary = uuid.uuid4().hex
    parts = []
    for key, value in fields.items():
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'.encode())
    parts.append(
        f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{filename}"\r\n'
        f"Content-Type: audio/wav\r\n\r\n".encode() + audio + b"\r\n"
    )
    parts.append(f"--{boundary}--\r\n".encode())
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/inference",
        data=b"".join(parts),
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.loads(response.read())["text"].strip()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("step", choices=["parity", "prompt", "vad", "speed"])
    parser.add_argument("--probe", required=True)
    parser.add_argument("--gguf", required=True, help="NB-Whisper Large GGUF (converted)")
    parser.add_argument("--bin", required=True, help="NbAiLab's ggml-model.bin")
    parser.add_argument("--vad", help="Silero VAD GGUF")
    parser.add_argument("--whisper-cli", default="whisper-cli")
    parser.add_argument("--whisper-server", default="whisper-server")
    parser.add_argument("--clips", help="FLEURS clips from research/clips/fetch.py, or noisy clips")
    parser.add_argument("--pattern", default="fleurs-nb-*.wav", help="parity: clip file pattern")
    parser.add_argument("--fleurs", help="parity: FLEURS clip directory, for the table order of other clips")
    parser.add_argument("--name", help="parity: results file name (default parity)")
    parser.add_argument("--takes", help="takes from make_clips.py")
    parser.add_argument("--prompt", help="vocabulary prompt file")
    parser.add_argument("--long-prompt", help="vocabulary prompt near the 223-token limit")
    parser.add_argument("--timestamps", default="none", choices=["none", "auto"])
    parser.add_argument("--clip", help="clip to time (speed)")
    parser.add_argument("--extra-gguf", nargs="*", default=[], help="more GGUFs to time (speed)")
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--repeat", type=int, default=10)
    parser.add_argument("--port", type=int, default=8649)
    parser.add_argument("--gpu-lock", help="lock directory shared with other GPU users: timing takes it, "
                                           "other steps wait while it exists")
    parser.add_argument("--results", required=True, help="directory for the JSON results")
    args = parser.parse_args()
    {"parity": step_parity, "prompt": step_prompt, "vad": step_vad, "speed": step_speed}[args.step](args)


if __name__ == "__main__":
    main()

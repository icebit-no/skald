#!/usr/bin/env python3
"""Transcribe a clip folder with NbAiLab's own inference.py (the reference).

Loads `BokfinkASR` from the `inference.py` that ships in the model folder, so
the prompt, decoding and text clean-up are NbAiLab's, unchanged. The model is
loaded once; every `*.wav` in the clip folder is transcribed once per target
language. Writes one JSON line per run:

    {"clip": "fleurs-nb-1970", "target_lang": "nob", "keywords": "",
     "text": "...", "duration_seconds": 15.42, "truncated": false,
     "seconds": 3.1}

Usage (the model folder is a download of NbAiLab/nb-asr-qwen06b-bokfink02-beta
at a pinned revision):

    python reference.py --model <model dir> --clips <clip dir> \
        --out reference.jsonl [--langs nob,nno] [--keywords keywords.tsv [--keywords-only]]

`--keywords` names a tab-separated file `<clip>\t<keywords>`; each listed clip
is run once more per language with those keywords as SYSTEM context.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import time
from pathlib import Path


def load_inference_module(model_dir: Path):
    spec = importlib.util.spec_from_file_location("bokfink_inference", model_dir / "inference.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_keywords(path: Path | None) -> dict[str, str]:
    if path is None:
        return {}
    table = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        clip, words = line.split("\t", 1)
        table[clip.strip()] = words.strip()
    return table


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--clips", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--langs", default="nob,nno")
    parser.add_argument("--keywords", type=Path, default=None)
    parser.add_argument("--keywords-only", action="store_true",
                        help="run only the clips listed in --keywords")
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--threads", type=int, default=6)
    parser.add_argument("--float32", action="store_true",
                        help="cast the loaded model to float32 (a precision check; "
                             "NbAiLab's script runs BF16)")
    parser.add_argument("--only", default="", help="comma-separated clip names to run")
    args = parser.parse_args()

    import torch

    torch.set_num_threads(args.threads)
    inference = load_inference_module(args.model)
    asr = inference.BokfinkASR(model=str(args.model), backend="transformers", device=args.device)
    if args.float32:
        asr.wrapper.model.float()

    clips = sorted(args.clips.glob("*.wav"))
    if args.only:
        clips = [clip for clip in clips if clip.stem in args.only.split(",")]
    if not clips:
        sys.exit(f"no .wav files in {args.clips}")
    keywords = read_keywords(args.keywords)
    jobs = [] if args.keywords_only else [(clip, lang, "") for clip in clips for lang in args.langs.split(",")]
    jobs += [
        (clip, lang, keywords[clip.stem])
        for clip in clips
        if clip.stem in keywords
        for lang in args.langs.split(",")
    ]

    with args.out.open("w", encoding="utf-8") as out:
        for clip, lang, words in jobs:
            start = time.perf_counter()
            result = asr.transcribe(str(clip), target_lang=lang, keywords=words)
            row = {
                "clip": clip.stem,
                "target_lang": lang,
                "keywords": words,
                "text": result["text"],
                "duration_seconds": result["duration_seconds"],
                "truncated": result["truncated"],
                "seconds": round(time.perf_counter() - start, 3),
            }
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
            out.flush()
            print(f"{clip.stem} {lang} {'kw ' if words else ''}{row['seconds']:.1f}s  {row['text']}", file=sys.stderr)


if __name__ == "__main__":
    main()

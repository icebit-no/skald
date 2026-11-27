#!/usr/bin/env python3
"""Transcribe a clip folder with transcribe.cpp's CLI and a Bokfink GGUF.

Writes the same JSON lines as reference.py, so compare.py can join the two.
The language codes map onto the GGUF's declared languages: nob -> nb
(`language Norwegian<asr_text>`), nno -> nn (`language Norwegian Nynorsk<asr_text>`).
Keywords go in as one vocabulary entry, which transcribe.cpp writes verbatim
into the system message, where NbAiLab's prompt puts them.

Usage:

    python transcribe_cpp.py --cli <build>/bin/transcribe-cli --model bokfink.gguf \
        --clips <clip dir> --out transcribe_cpp.jsonl [--langs nob,nno] \
        [--keywords keywords.tsv] [--backend metal]

Standard library only.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

CODES = {"nob": "nb", "nno": "nn", "eng": "en"}


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


def run_batch(cli: Path, model: Path, wavs: list[Path], lang: str, backend: str,
              vocabulary_file: Path | None) -> list[dict]:
    with tempfile.NamedTemporaryFile("w", suffix=".list", delete=False) as listing:
        listing.write("".join(f"{wav}\n" for wav in wavs))
    cmd = [str(cli), "-m", str(model), "-l", CODES[lang], "--backend", backend,
           "--batch", listing.name, "--batch-jsonl"]
    if vocabulary_file is not None:
        cmd += ["--vocabulary-file", str(vocabulary_file)]
    done = subprocess.run(cmd, capture_output=True, text=True)
    Path(listing.name).unlink()
    if done.returncode != 0:
        sys.exit(f"transcribe-cli failed ({done.returncode}):\n{done.stderr[-2000:]}")
    rows = [json.loads(line) for line in done.stdout.splitlines() if line.startswith("{")]
    return [row for row in rows if "file" in row]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--clips", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--langs", default="nob,nno")
    parser.add_argument("--keywords", type=Path, default=None)
    parser.add_argument("--backend", default="metal")
    args = parser.parse_args()

    wavs = sorted(args.clips.glob("*.wav"))
    if not wavs:
        sys.exit(f"no .wav files in {args.clips}")
    keywords = read_keywords(args.keywords)

    with args.out.open("w", encoding="utf-8") as out:
        def emit(row: dict, lang: str, words: str) -> None:
            record = {
                "clip": Path(row["file"]).stem,
                "target_lang": lang,
                "keywords": words,
                "text": row.get("text", "").strip(),
                "encode_ms": row.get("encode_ms"),
                "decode_ms": row.get("decode_ms"),
            }
            out.write(json.dumps(record, ensure_ascii=False) + "\n")
            print(f"{record['clip']} {lang} {'kw ' if words else ''} {record['text']}", file=sys.stderr)

        for lang in args.langs.split(","):
            for row in run_batch(args.cli, args.model, wavs, lang, args.backend, None):
                emit(row, lang, "")
        for wav in wavs:
            words = keywords.get(wav.stem)
            if not words:
                continue
            with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as vocab:
                vocab.write(words + "\n")
            for lang in args.langs.split(","):
                for row in run_batch(args.cli, args.model, [wav], lang, args.backend, Path(vocab.name)):
                    emit(row, lang, words)
            Path(vocab.name).unlink()


if __name__ == "__main__":
    main()

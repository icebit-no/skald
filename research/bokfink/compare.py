#!/usr/bin/env python3
"""Compare transcribe.cpp output with NbAiLab's reference, clip by clip.

Joins two JSON-lines files (from reference.py and transcribe_cpp.py) on clip,
target language and keywords, and prints Markdown tables: whether the texts
match exactly, and the word differences where they do not. With `--clips`, it
also prints each engine's word error rate against the FLEURS text for the
plain bokmål runs (lower-cased, punctuation removed), as context only.

Usage:

    python compare.py reference.jsonl transcribe_cpp.jsonl [--clips <clip dir>]

Standard library only.
"""

from __future__ import annotations

import argparse
import difflib
import json
import re
from pathlib import Path

LANG_TITLES = {"nob": "Bokmål (`language Norwegian<asr_text>`)",
               "nno": "Nynorsk (`language Norwegian Nynorsk<asr_text>`)",
               "eng": "English (`language English<asr_text>`)"}


def load(path: Path) -> dict[tuple[str, str, str], dict]:
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            row = json.loads(line)
            rows[(row["clip"], row["target_lang"], row["keywords"])] = row
    return rows


def word_diff(a: str, b: str) -> str:
    wa, wb = a.split(), b.split()
    parts = []
    for op, i1, i2, j1, j2 in difflib.SequenceMatcher(a=wa, b=wb, autojunk=False).get_opcodes():
        if op == "replace":
            parts.append(f"«{' '.join(wa[i1:i2])}» → «{' '.join(wb[j1:j2])}»")
        elif op == "delete":
            parts.append(f"− «{' '.join(wa[i1:i2])}»")
        elif op == "insert":
            parts.append(f"+ «{' '.join(wb[j1:j2])}»")
    return "; ".join(parts)


def normalise(text: str) -> list[str]:
    return re.sub(r"[^\w]+", " ", text.lower()).split()


def edit_distance(a: list[str], b: list[str]) -> int:
    prev = list(range(len(b) + 1))
    for i, wa in enumerate(a, 1):
        cur = [i]
        for j, wb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (wa != wb)))
        prev = cur
    return prev[-1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("reference", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--clips", type=Path, default=None)
    args = parser.parse_args()

    ref, cand = load(args.reference), load(args.candidate)
    missing = sorted(set(ref) - set(cand))
    if missing:
        print(f"Missing from {args.candidate.name}: {missing}\n")

    keys = sorted(set(ref) & set(cand))
    total = sum(ref[k]["text"].strip() == cand[k]["text"].strip() for k in keys)
    print(f"**{total} of {len(keys)} runs match exactly.**\n")

    groups = [(lang, False) for lang in LANG_TITLES] + [(lang, True) for lang in LANG_TITLES]
    for lang, with_keywords in groups:
        group = [k for k in keys if k[1] == lang and bool(k[2]) == with_keywords]
        if not group:
            continue
        same = sum(ref[k]["text"].strip() == cand[k]["text"].strip() for k in group)
        title = LANG_TITLES[lang] + (", with keywords" if with_keywords else "")
        print(f"### {title}: {same} of {len(group)} exact\n")
        if with_keywords:
            print("| Clip | Keywords | Exact | Word differences (reference → transcribe.cpp) | Text |")
            print("|---|---|---|---|---|")
        else:
            print("| Clip | Exact | Word differences (reference → transcribe.cpp) | Text |")
            print("|---|---|---|---|")
        for k in group:
            a, b = ref[k]["text"].strip(), cand[k]["text"].strip()
            exact = "yes" if a == b else "**no**"
            diff = word_diff(a, b) if a != b else ""
            if a != b and not diff:
                diff = "whitespace only"
            clip = k[0].removeprefix("fleurs-nb-")
            if with_keywords:
                print(f"| {clip} | {k[2]} | {exact} | {diff} | {a} |")
            else:
                print(f"| {clip} | {exact} | {diff} | {a} |")
        print()

    if args.clips is not None:
        plain = [k for k in keys if k[1] == "nob" and not k[2]]
        n_words = 0
        errors = {"reference": 0, "transcribe.cpp": 0}
        for k in plain:
            truth = normalise((args.clips / f"{k[0]}.txt").read_text(encoding="utf-8"))
            n_words += len(truth)
            errors["reference"] += edit_distance(truth, normalise(ref[k]["text"]))
            errors["transcribe.cpp"] += edit_distance(truth, normalise(cand[k]["text"]))
        print(f"Word error rate against the FLEURS text, plain bokmål runs, {len(plain)} clips, "
              f"{n_words} words (lower-cased, punctuation removed; context only):\n")
        for engine, err in errors.items():
            print(f"- {engine}: {err} errors, {100.0 * err / n_words:.1f} %")
        print()


if __name__ == "__main__":
    main()

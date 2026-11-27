#!/usr/bin/env python3
"""Merge a sharded Hugging Face checkpoint into one model.safetensors.

transcribe.cpp's Whisper converter reads a single `model.safetensors`, while
NbAiLab/nb-whisper-large ships its weights in two shards listed in
`model.safetensors.index.json`. This writes a directory the converter accepts:
every small file of the source directory is linked into `<out>`, and the shards
are merged tensor for tensor, unchanged, into `<out>/model.safetensors`.

Runs in transcribe.cpp's Whisper converter environment (it needs safetensors
and torch):

    uv run --project <transcribe.cpp>/scripts/envs/whisper \
        python research/engine-check/merge_safetensors.py <hf-dir> <out-dir>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

from safetensors import safe_open
from safetensors.torch import save_file


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    source, out = Path(sys.argv[1]), Path(sys.argv[2])
    index = json.loads((source / "model.safetensors.index.json").read_text())
    shards = sorted(set(index["weight_map"].values()))
    out.mkdir(parents=True, exist_ok=True)

    for item in source.iterdir():
        if item.suffix == ".safetensors" or item.name == "model.safetensors.index.json":
            continue
        link = out / item.name
        if not link.exists():
            link.symlink_to(item.resolve())

    tensors = {}
    for shard in shards:
        with safe_open(str(source / shard), framework="pt") as f:
            for name in f.keys():
                tensors[name] = f.get_tensor(name)
    missing = set(index["weight_map"]) - set(tensors)
    if missing:
        sys.exit(f"tensors listed in the index but not in the shards: {sorted(missing)[:5]}")
    save_file(tensors, str(out / "model.safetensors"), metadata={"format": "pt"})
    print(f"{len(tensors)} tensors from {len(shards)} shards -> {out / 'model.safetensors'}")


if __name__ == "__main__":
    main()

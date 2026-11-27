#!/usr/bin/env python3
"""Run transcribe.cpp's Whisper converter on an NB-Whisper checkpoint.

The converter refuses variant slugs it does not list, and lists only OpenAI's
checkpoints and Breeze-ASR-25. NB-Whisper has the Whisper large-v3
architecture, so the converter handles it as is; this wrapper only registers
the NB-Whisper slugs and display names, then calls the converter's own main
with the remaining arguments. The GGUF's author and organization come from
`--repo-id` (NbAiLab), its licence is the converter's Apache 2.0 default.

    uv run --project <transcribe.cpp>/scripts/envs/whisper \
        python research/engine-check/convert_nb_whisper.py <transcribe.cpp> \
        <merged-hf-dir> <out.gguf> --repo-id NbAiLab/nb-whisper-large \
        --revision <commit>
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

NB_WHISPER_NAMES = {
    "nb-whisper-large": "NB-Whisper Large",
    "nb-whisper-medium": "NB-Whisper Medium",
    "nb-whisper-small": "NB-Whisper Small",
    "nb-whisper-base": "NB-Whisper Base",
}


def main() -> int:
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    transcribe_cpp = Path(sys.argv[1]).resolve()
    script = transcribe_cpp / "scripts" / "convert-whisper.py"
    spec = importlib.util.spec_from_file_location("convert_whisper", script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.VARIANT_DISPLAY_NAMES.update(NB_WHISPER_NAMES)
    return module.main([str(script), *sys.argv[2:]])


if __name__ == "__main__":
    raise SystemExit(main())

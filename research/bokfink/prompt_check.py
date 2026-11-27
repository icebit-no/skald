#!/usr/bin/env python3
"""Check that NbAiLab's prompt has the token layout transcribe.cpp builds.

Renders the prompt with `make_prompt` from the model folder's inference.py and
tokenizes it with the model's own processor, as NbAiLab's script does. Then
builds the sequence transcribe.cpp's Qwen3-ASR prompt builder assembles (role
and newline tokens, the system text encoded on its own, the audio pads, and
`language {Name}` encoded on its own followed by `<asr_text>`) and checks that
the two are identical, for both language prefixes, with and without keywords.

The verbatim style is printed as well: its prompt has `\\n<verbatim>` after
the audio, a slot transcribe.cpp's builder has no field for.

Usage:

    python prompt_check.py --model <model dir>
"""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path

LANGUAGE_NAMES = {"nob": "Norwegian", "nno": "Norwegian Nynorsk"}
KEYWORDS = ["", "mosasaurer", "Nord-Østersjøen Tromsø Nordlandssykehuset"]


def load_inference_module(model_dir: Path):
    spec = importlib.util.spec_from_file_location("bokfink_inference", model_dir / "inference.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def transcribe_cpp_layout(tok, n_audio: int, system_text: str, language: str) -> list[int]:
    """Mirror of build_prompt_tokens + encode_language_prefix (src/arch/qwen3_asr/model.cpp)."""
    piece = tok.convert_tokens_to_ids
    newline = piece("Ċ")
    ids = [piece("<|im_start|>"), piece("system"), newline]
    ids += tok.encode(system_text, add_special_tokens=False) if system_text else []
    ids += [piece("<|im_end|>"), newline, piece("<|im_start|>"), piece("user"), newline]
    ids += [piece("<|audio_start|>")] + [piece("<|audio_pad|>")] * n_audio + [piece("<|audio_end|>")]
    ids += [piece("<|im_end|>"), newline, piece("<|im_start|>"), piece("assistant"), newline]
    ids += tok.encode(f"language {language}", add_special_tokens=False) + [piece("<asr_text>")]
    return ids


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--model", type=Path, required=True)
    args = parser.parse_args()

    import numpy as np
    from qwen_asr.core.transformers_backend.processing_qwen3_asr import Qwen3ASRProcessor

    inference = load_inference_module(args.model)
    processor = Qwen3ASRProcessor.from_pretrained(str(args.model))
    tok = processor.tokenizer
    audio = np.zeros(int(16000 * 9.24), dtype=np.float32)
    audio_pad = tok.convert_tokens_to_ids("<|audio_pad|>")

    def hf_ids(lang: str, style: str, keywords: str) -> list[int]:
        prompt = inference.make_prompt(processor, lang, style, keywords)
        batch = processor(text=[prompt], audio=[audio], sampling_rate=16000, return_tensors="np",
                          padding=True, truncation=False)
        return [int(x) for x in batch["input_ids"][0]]

    failures = 0
    for lang, name in LANGUAGE_NAMES.items():
        for keywords in KEYWORDS:
            got = hf_ids(lang, "reading_optimised", keywords)
            want = transcribe_cpp_layout(tok, got.count(audio_pad), keywords, name)
            same = got == want
            failures += not same
            print(f"{'same' if same else 'DIFFERENT'}  {lang}  keywords={keywords!r}  "
                  f"{len(got)} tokens, {got.count(audio_pad)} audio")
            if not same:
                print(f"  NbAiLab:        {tok.convert_ids_to_tokens(got)}")
                print(f"  transcribe.cpp: {tok.convert_ids_to_tokens(want)}")
        prefix = tok.encode(f"language {name}", add_special_tokens=False) + [tok.convert_tokens_to_ids("<asr_text>")]
        print(f"      {lang} prefix ids {prefix} = {tok.convert_ids_to_tokens(prefix)}")

    verbatim = hf_ids("nob", "verbatim", "")
    end = verbatim.index(tok.convert_tokens_to_ids("<|audio_end|>"))
    print(f"verbatim: after <|audio_end|> come {tok.convert_ids_to_tokens(verbatim[end + 1:end + 4])} "
          f"(ids {verbatim[end + 1:end + 4]}); transcribe.cpp puts <|im_end|> right after the audio")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()

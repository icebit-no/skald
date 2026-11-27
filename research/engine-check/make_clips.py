#!/usr/bin/env python3
"""Make the synthetic test takes for the engine check (16 kHz mono WAV).

    python3 make_clips.py vocab --out DIR
        vocab-01 .. vocab-08: one sentence each, spoken by macOS `say -v Nora`,
        with words NB-Whisper breaks without a prompt («merget»,
        «pull-requesten», «PostgreSQL»). vocab-long: the eight sentences spread
        over about 100 s of plain speech, so most of them fall in the second,
        third and fourth 30-second window. word-ja, word-nei, ...: one short
        word with a second of silence on each side. Each WAV has a .txt reference;
        the long take also has vocab-long.json with each sentence's start time.

    python3 make_clips.py silence --out DIR [--fleurs DIR]
        Takes without speech: digital silence, low-level noise, mains hum, a
        breath-like noise burst, a click and typing. These are synthesized
        (seeded, so every run writes the same samples), not recorded. With
        --fleurs, also the first 1.5 s of three FLEURS clips, before the
        speaker starts: real room tone from real microphones.

    python3 make_clips.py noisy --out DIR --fleurs DIR
        Every FLEURS clip made harder, as from a distant microphone: a sparse
        synthetic room echo (40 decaying reflections over 0.4 s), then pink
        noise at 5 dB signal-to-noise ratio. Seeded, so every run writes the
        same samples. Written as noisy-<id>.wav with the clip's reference text.

Python standard library only; `vocab` needs macOS `say` with the Nora voice.
"""

from __future__ import annotations

import argparse
import array
import json
import math
import random
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

RATE = 16000

VOCAB = [
    "Jeg har merget pull-requesten og kjørt migreringen mot PostgreSQL.",
    "Kan du rebase branchen før vi merger den til main?",
    "Pull-requesten feiler fordi Kubernetes-clusteret mangler en secret.",
    "Vi bytter fra SQLite til PostgreSQL i neste release.",
    "Skald setter inn teksten der markøren står.",
    "Har du pushet commiten til GitHub ennå?",
    "Transcribe.cpp kjører NB-Whisper med Metal på Macen.",
    "Jeg merget den i går, så deployen gikk ut i natt.",
]

FILLER = [
    "God morgen, her er en kort oppdatering om hva jeg har gjort denne uken.",
    "Mandag gikk mest med til å rydde opp i gamle oppgaver og svare på e-post.",
    "Tirsdag satt jeg sammen med teamet og planla arbeidet for de neste to ukene.",
    "Vi ble enige om å prioritere stabilitet foran nye funksjoner en stund.",
    "Onsdag jobbet jeg videre med databasen.",
    "Det tok lengre tid enn ventet, fordi testene var trege og noen av dem feilet tilfeldig.",
    "Torsdag hadde vi et langt møte om hvordan vi skal rulle ut endringene.",
    "Vi landet på å gjøre det gradvis, først for noen få brukere og så for alle.",
    "Fredag brukte jeg på dokumentasjon og på å skrive ned det vi har lært.",
    "Neste uke vil jeg se nærmere på ytelsen, særlig når mange bruker systemet samtidig.",
    "Jeg trenger også hjelp til å sette opp et nytt testmiljø.",
    "Hvis noen har tid, setter jeg stor pris på en gjennomgang av koden.",
    "Ellers går det fint, og jeg gleder meg til helgen.",
    "Det var alt for nå, takk for at dere hørte på.",
]

# Order of the long take: ("F", i) is FILLER[i], ("V", i) is VOCAB[i].
LONG_TAKE = [
    ("F", 0), ("F", 1), ("V", 4), ("F", 2), ("F", 3), ("V", 1), ("F", 4), ("F", 5),
    ("V", 0), ("F", 6), ("V", 2), ("F", 7), ("V", 5),
    ("V", 3), ("F", 8), ("V", 6), ("F", 9),
    ("V", 7), ("F", 10), ("F", 11), ("F", 12), ("F", 13),
]
GAP_S = 0.4

# One-word takes, each with a second of silence on both sides: the shortest
# real dictations, for the 250 ms speech rule.
SHORT_WORDS = ["Ja.", "Nei.", "Takk.", "OK.", "Hei."]

# FLEURS clips whose first speech segment starts after 2.2 s (Silero,
# threshold 0.5); their first 1.5 s is room tone.
FLEURS_ROOM_TONE = ["1970", "1802", "2006"]
ROOM_TONE_S = 1.5


def write_wav(path: Path, samples: list[float] | array.array) -> None:
    ints = array.array("h", (max(-32768, min(32767, round(s * 32767.0))) for s in samples))
    if sys.byteorder != "little":
        ints.byteswap()
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(ints.tobytes())


def read_wav(path: Path) -> list[float]:
    with wave.open(str(path), "rb") as w:
        if w.getnchannels() != 1 or w.getframerate() != RATE or w.getsampwidth() != 2:
            sys.exit(f"{path}: expected 16 kHz mono 16-bit")
        ints = array.array("h")
        ints.frombytes(w.readframes(w.getnframes()))
    if sys.byteorder != "little":
        ints.byteswap()
    return [s / 32768.0 for s in ints]


def say(text: str, path: Path) -> None:
    subprocess.run(
        ["say", "-v", "Nora", "-o", str(path), "--file-format=WAVE", "--data-format=LEI16@16000", text],
        check=True,
    )


def vocab(out: Path) -> None:
    for i, text in enumerate(VOCAB, 1):
        say(text, out / f"vocab-{i:02d}.wav")
        (out / f"vocab-{i:02d}.txt").write_text(text + "\n", encoding="utf-8")

    samples: list[float] = []
    texts: list[str] = []
    starts: list[dict] = []
    with tempfile.TemporaryDirectory() as tmp:
        for kind, index in LONG_TAKE:
            text = VOCAB[index] if kind == "V" else FILLER[index]
            piece = Path(tmp) / "piece.wav"
            say(text, piece)
            starts.append({"start_s": round(len(samples) / RATE, 2), "vocab": kind == "V", "text": text})
            samples.extend(read_wav(piece))
            samples.extend([0.0] * int(GAP_S * RATE))
            texts.append(text)
    for word in SHORT_WORDS:
        name = f"word-{word.lower().strip('.!')}"
        piece = out / f"{name}.wav"
        say(word, piece)
        padded = [0.0] * RATE + read_wav(piece) + [0.0] * RATE
        write_wav(piece, padded)
        (out / f"{name}.txt").write_text(word + "\n", encoding="utf-8")

    write_wav(out / "vocab-long.wav", samples)
    (out / "vocab-long.txt").write_text(" ".join(texts) + "\n", encoding="utf-8")
    (out / "vocab-long.json").write_text(json.dumps(starts, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"vocab-long: {len(samples) / RATE:.1f} s")


def db(level_dbfs: float) -> float:
    return 10 ** (level_dbfs / 20)


def pink(rng: random.Random, n: int) -> list[float]:
    """Pink-ish noise (Paul Kellett's filter), normalised to RMS 1."""
    b = [0.0] * 7
    out = []
    for _ in range(n):
        w = rng.gauss(0, 1)
        b[0] = 0.99886 * b[0] + w * 0.0555179
        b[1] = 0.99332 * b[1] + w * 0.0750759
        b[2] = 0.96900 * b[2] + w * 0.1538520
        b[3] = 0.86650 * b[3] + w * 0.3104856
        b[4] = 0.55000 * b[4] + w * 0.5329522
        b[5] = -0.7616 * b[5] - w * 0.0168980
        out.append(sum(b[:6]) + b[6] + w * 0.5362)
        b[6] = w * 0.115926
    rms = math.sqrt(sum(x * x for x in out) / n)
    return [x / rms for x in out]


def band_noise(rng: random.Random, n: int, low_hz: float, high_hz: float) -> list[float]:
    """White noise through a one-pole high-pass and low-pass, normalised to peak 1."""
    a_hp = math.exp(-2 * math.pi * low_hz / RATE)
    a_lp = math.exp(-2 * math.pi * high_hz / RATE)
    prev_in = prev_hp = lp = 0.0
    out = []
    for _ in range(n):
        x = rng.gauss(0, 1)
        hp = a_hp * (prev_hp + x - prev_in)
        prev_in, prev_hp = x, hp
        lp = (1 - a_lp) * hp + a_lp * lp
        out.append(lp)
    peak = max(abs(x) for x in out) or 1.0
    return [x / peak for x in out]


def click(rng: random.Random, peak: float) -> list[float]:
    """A 6 ms decaying broadband transient."""
    n = int(0.006 * RATE)
    return [peak * rng.uniform(-1, 1) * math.exp(-i / (0.0012 * RATE)) for i in range(n)]


def add_at(base: list[float], piece: list[float], at_s: float) -> None:
    start = int(at_s * RATE)
    for i, x in enumerate(piece):
        if start + i < len(base):
            base[start + i] += x


def silence(out: Path, fleurs: Path | None) -> None:
    n = 3 * RATE
    rng = random.Random(2026_11_27)
    takes: dict[str, list[float]] = {}

    takes["silent-digital"] = [0.0] * n
    takes["silent-noise-60dB"] = [x * db(-60) for x in pink(rng, n)]
    takes["silent-noise-40dB"] = [x * db(-40) for x in pink(rng, n)]
    takes["silent-hum-50Hz"] = [
        db(-40) * (math.sin(2 * math.pi * 50 * i / RATE) + 0.5 * math.sin(2 * math.pi * 150 * i / RATE))
        for i in range(n)
    ]

    breath_len = int(0.7 * RATE)
    envelope = [math.sin(math.pi * i / breath_len) ** 2 for i in range(breath_len)]
    breath = [e * x * db(-26) for e, x in zip(envelope, band_noise(rng, breath_len, 400, 2500))]
    base = [x * db(-60) for x in pink(rng, n)]
    add_at(base, breath, 1.0)
    takes["silent-breath"] = base

    base = [x * db(-60) for x in pink(rng, n)]
    add_at(base, click(rng, db(-6)), 1.5)
    takes["silent-click"] = base

    base = [x * db(-60) for x in pink(rng, n)]
    for k in range(10):
        add_at(base, click(rng, db(-14) * rng.uniform(0.6, 1.0)), 0.3 + 0.24 * k + rng.uniform(-0.04, 0.04))
    takes["silent-typing"] = base

    if fleurs is not None:
        for sentence_id in FLEURS_ROOM_TONE:
            samples = read_wav(fleurs / f"fleurs-nb-{sentence_id}.wav")
            takes[f"silent-room-{sentence_id}"] = samples[: int(ROOM_TONE_S * RATE)]

    for name, samples in takes.items():
        write_wav(out / f"{name}.wav", samples)
        (out / f"{name}.txt").write_text("\n", encoding="utf-8")
    print(f"{len(takes)} silent takes")


def noisy(out: Path, fleurs: Path) -> None:
    rng = random.Random(5)
    reflections = sorted((rng.uniform(0.01, 0.4), rng.uniform(-1, 1)) for _ in range(40))
    taps = [(int(t * RATE), 0.6 * g * math.exp(-t / 0.12)) for t, g in reflections]
    clips = sorted(fleurs.glob("fleurs-nb-*.wav"))
    for wav in clips:
        dry = read_wav(wav)
        wet = list(dry)
        for delay, gain in taps:
            for i in range(len(dry) - delay):
                wet[i + delay] += gain * dry[i]
        signal_rms = math.sqrt(sum(x * x for x in wet) / len(wet))
        noise = pink(rng, len(wet))
        noise_gain = signal_rms / db(5)
        mixed = [s + noise_gain * n for s, n in zip(wet, noise)]
        peak = max(abs(x) for x in mixed)
        if peak > 0.99:
            mixed = [x * 0.99 / peak for x in mixed]
        name = wav.stem.replace("fleurs-nb-", "noisy-")
        write_wav(out / f"{name}.wav", mixed)
        (out / f"{name}.txt").write_text(reference_text(wav) + "\n", encoding="utf-8")
    print(f"{len(clips)} noisy clips")


def reference_text(wav: Path) -> str:
    return wav.with_suffix(".txt").read_text(encoding="utf-8").strip()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("what", choices=["vocab", "silence", "noisy"])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--fleurs", type=Path, help="FLEURS clip directory (silence only)")
    args = parser.parse_args()
    out = args.out.expanduser()
    out.mkdir(parents=True, exist_ok=True)
    fleurs = args.fleurs.expanduser() if args.fleurs else None
    if args.what == "vocab":
        vocab(out)
    elif args.what == "silence":
        silence(out, fleurs)
    elif fleurs is None:
        sys.exit("noisy needs --fleurs")
    else:
        noisy(out, fleurs)


if __name__ == "__main__":
    main()

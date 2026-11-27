#!/usr/bin/env python3
"""Fetch Norwegian Bokmål test clips from FLEURS for the engine checks.

FLEURS (google/fleurs, CC BY 4.0) is read at a pinned revision. The script
downloads the `nb_no` test split's transcript table and audio archive, checks
both against their SHA-256, and writes the first N sentences in table order:

    <out>/fleurs-nb-<id>.wav   16 kHz mono, 16-bit PCM
    <out>/fleurs-nb-<id>.txt   the reference text as FLEURS writes it
                               (original casing and punctuation), one line

FLEURS records each sentence with several speakers, so `<id>` is the sentence
id and only the first recording of each sentence is taken. The downloads are
kept in `<out>/.download/` and reused when their hash still matches.

Python standard library only:

    python3 research/clips/fetch.py --out ~/skald-clips          # 20 clips
    python3 research/clips/fetch.py --out ~/skald-clips --count 10
"""

from __future__ import annotations

import argparse
import array
import hashlib
import struct
import sys
import tarfile
import urllib.request
import wave
from pathlib import Path

REPO = "google/fleurs"
REVISION = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
SPLIT_FILES = {
    "test.tsv": (
        "data/nb_no/test.tsv",
        "d137ed2b39d3294ee72423b08352ff6184123bd25477050b8e0b953aab8e29d4",
    ),
    "test.tar.gz": (
        "data/nb_no/audio/test.tar.gz",
        "96fa705a6b30bc24548855f8dd1a2d722feb6bc78b9dfb2750dc075dc83a1ff4",
    ),
}
SAMPLE_RATE = 16000
DEFAULT_COUNT = 20

# The written clips for the default count, so a rerun can show it produced the
# same audio as the run the reports were measured on.
EXPECTED_CLIPS = {
    "1970": "5587527fbe6037e53dd85721328bfd297f83112e633cf458e57a3bf85c1d504d",
    "1994": "7f38f07fd113025d4f98ed2edc2d84206c6a8abebefc8164969c68537c47f01e",
    "2000": "b512dfdadd67c93717117d4201768c42308c0bd508e9665e7a5f81d187ae1940",
    "1981": "717f25c9ae2cb20d05030e30d6ac0549a774b01873053f23bef9da0b153c0efd",
    "1680": "6b7e372f15457eae0813408b7999207345a924000bfd650cdbd2a5e7c143ac8b",
    "1766": "6f0b1655672c8213b908d806a09f41ffd6e0cb9f9073e20c25e886a6446fda29",
    "1664": "a9570657597ac81f37e3f615c43d1fb0306a44b454861ee79bc2b449c4649284",
    "1768": "32246eb5ee93f139b3b42c1d8bb8101380a7a2b8d854c72790f56272ffce39b5",
    "1686": "0b08668b6b6846f228d8aa24c053cfc47242713d63e582765ad5c6501fbdba3e",
    "1802": "a74b1bc1650c9b68482b37ff41e634524eff2ecbf6a2a3f13d1b93faa35401c5",
    "1811": "6853f3e474a69b133ea0ceeaad94948e8165fdf518b696620717c9a8e48356b5",
    "1971": "a817c407e351142192e922105e87c706a2746ff66b9f3bffbe4e12fcb60130a3",
    "1930": "fdfa6d5d2f4601959ee5ee03d1046c61a6ff4ffe47264ac60b318e4020d374f0",
    "1661": "aec771eec2e3bfef58a7c47e5505b60c1fd19259b19d57344f8a0994386ef3c9",
    "1906": "873c920bf2fb576c87003f596d6f1dd7d4b49966f3fb1d4c90e38d22d5603bfc",
    "1898": "5ffd6652d5b55942cd27cdc002e2906d628617a00b992404848aee621dc858ec",
    "2006": "c0570181520d2c38c894b64f199d44f0d6559b29ba59e8d2e9b99cca7bd062a6",
    "2001": "37aeb3d726e5f923c4e34aec9c4a965ccd1a188bcf656ae2f72317f65d4a14f4",
    "1721": "c72bb1336598ee34c629c6c0d217c80613ae63553779779c55222cedf9bc6bee",
    "1833": "2f3c8069c72b45e8de8f4614f4249ad3c5e44a8b8d20b27cd3a69432d8ed7636",
}


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def download(name: str, cache: Path) -> Path:
    remote, expected = SPLIT_FILES[name]
    target = cache / name
    if target.exists() and sha256_of(target) == expected:
        return target
    url = f"https://huggingface.co/datasets/{REPO}/resolve/{REVISION}/{remote}"
    print(f"downloading {url}", file=sys.stderr)
    partial = target.with_suffix(target.suffix + ".part")
    with urllib.request.urlopen(url) as response, partial.open("wb") as f:
        while block := response.read(1 << 20):
            f.write(block)
    actual = sha256_of(partial)
    if actual != expected:
        partial.unlink()
        sys.exit(f"{name}: SHA-256 {actual}, expected {expected}")
    partial.replace(target)
    return target


def first_sentences(tsv: Path, count: int) -> list[tuple[str, str, str]]:
    """(sentence id, audio file name, raw transcription) in table order."""
    picked: list[tuple[str, str, str]] = []
    seen: set[str] = set()
    for line in tsv.read_text(encoding="utf-8").splitlines():
        sentence_id, file_name, raw_text = line.split("\t")[:3]
        if sentence_id in seen:
            continue
        seen.add(sentence_id)
        picked.append((sentence_id, file_name, raw_text.strip()))
        if len(picked) == count:
            break
    if len(picked) < count:
        sys.exit(f"the test split has only {len(picked)} sentences")
    return picked


def pcm16_from_wav(data: bytes, name: str) -> bytes:
    """Mono 16 kHz samples as little-endian int16, from float32 or int16 WAV."""
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        sys.exit(f"{name}: not a RIFF/WAVE file")
    fmt = None
    samples = None
    pos = 12
    while pos + 8 <= len(data):
        chunk_id, size = struct.unpack_from("<4sI", data, pos)
        body = data[pos + 8 : pos + 8 + size]
        if chunk_id == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body)
        elif chunk_id == b"data":
            samples = body
        pos += 8 + size + (size & 1)
    if fmt is None or samples is None:
        sys.exit(f"{name}: missing fmt or data chunk")
    tag, channels, rate, _, _, bits = fmt
    if channels != 1 or rate != SAMPLE_RATE:
        sys.exit(f"{name}: {channels} channels at {rate} Hz, expected mono {SAMPLE_RATE} Hz")
    if tag == 1 and bits == 16:
        return samples
    if tag == 3 and bits == 32:
        floats = array.array("f")
        floats.frombytes(samples[: len(samples) // 4 * 4])
        if sys.byteorder != "little":
            floats.byteswap()
        ints = array.array("h", (max(-32768, min(32767, round(x * 32767.0))) for x in floats))
        if sys.byteorder != "little":
            ints.byteswap()
        return ints.tobytes()
    sys.exit(f"{name}: WAV format tag {tag} with {bits} bits is not handled")


def write_clip(out: Path, sentence_id: str, pcm: bytes, text: str) -> Path:
    wav_path = out / f"fleurs-nb-{sentence_id}.wav"
    with wave.open(str(wav_path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SAMPLE_RATE)
        w.writeframes(pcm)
    (out / f"fleurs-nb-{sentence_id}.txt").write_text(text + "\n", encoding="utf-8")
    return wav_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, required=True, help="directory for the clips")
    parser.add_argument("--count", type=int, default=DEFAULT_COUNT, help="number of sentences (default 20)")
    args = parser.parse_args()

    out = args.out.expanduser()
    cache = out / ".download"
    cache.mkdir(parents=True, exist_ok=True)

    tsv = download("test.tsv", cache)
    archive = download("test.tar.gz", cache)
    wanted = first_sentences(tsv, args.count)
    by_member = {f"test/{file_name}": (sentence_id, text) for sentence_id, file_name, text in wanted}

    written: dict[str, str] = {}
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            if member.name not in by_member or not member.isfile():
                continue
            sentence_id, text = by_member[member.name]
            pcm = pcm16_from_wav(tar.extractfile(member).read(), member.name)
            wav_path = write_clip(out, sentence_id, pcm, text)
            written[sentence_id] = sha256_of(wav_path)

    missing = [sid for sid, _, _ in wanted if sid not in written]
    if missing:
        sys.exit(f"not in the archive: {', '.join(missing)}")

    mismatched = [
        sid for sid in written if sid in EXPECTED_CLIPS and EXPECTED_CLIPS[sid] != written[sid]
    ]
    for sentence_id, _, _ in wanted:
        print(f"fleurs-nb-{sentence_id}  {written[sentence_id]}")
    if mismatched:
        sys.exit(f"written audio differs from the measured run: {', '.join(mismatched)}")


if __name__ == "__main__":
    main()

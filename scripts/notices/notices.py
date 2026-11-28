#!/usr/bin/env python3
"""Writes THIRD-PARTY-NOTICES at the root of the repository, or checks that it is up to date.

    python3 scripts/notices/notices.py           write the file
    python3 scripts/notices/notices.py --check   exit 1 when the file differs from what would be written

The file credits what Skald's program carries and what it downloads: the speech models, transcribe.cpp and the
C and C++ code built with it, Silero VAD, the font, and every Rust crate compiled into the `skald` binary on
macOS, Windows or Linux. Which crates those are comes from `cargo metadata` for each platform's target, so
Cargo.lock decides; their licence texts come from the packages Cargo has downloaded. Only Python's standard
library is used, so the check runs wherever Cargo does.
"""

import argparse
import difflib
import json
import re
import subprocess
import sys
import textwrap
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUTPUT = ROOT / "THIRD-PARTY-NOTICES"
WIDTH = 100

# What each release is built for: Skald ships for Apple Silicon only on macOS (D-34).
PLATFORMS = (
    ("macOS", "aarch64-apple-darwin"),
    ("Windows", "x86_64-pc-windows-msvc"),
    ("Linux", "x86_64-unknown-linux-gnu"),
)
PLATFORM_NAMES = tuple(name for name, _ in PLATFORMS)
BINARY = "skald"

# Licence texts kept beside the script, for the crates that ship none of their own and for comparing those
# that do: Apache 2.0 from apache.org without its appendix, MIT in its usual wording without a copyright line.
STANDARD_TEXTS = {"MIT": "MIT.txt", "Apache-2.0": "Apache-2.0.txt"}
# Public domain dedications and licences that ask for no notice: a crate under one is listed without a text.
NO_NOTICE = {"CC0-1.0", "Unlicense", "0BSD", "MIT-0"}
# Skald is MIT, so a crate offered only under one of these could not be part of it.
COPYLEFT = {"GPL", "LGPL", "AGPL"}
# The order a licence is taken in when a crate offers a choice.
PREFERENCE = (
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
    "Unicode-3.0",
    "BSL-1.0",
    "Unlicense",
    "0BSD",
    "MIT-0",
    "CC0-1.0",
    "Apache-2.0 WITH LLVM-exception",
    "BlueOak-1.0.0",
    "MPL-2.0",
)
NAMES = {
    "MIT": "MIT License",
    "MIT-0": "MIT No Attribution",
    "Apache-2.0": "Apache License 2.0",
    "Apache-2.0 WITH LLVM-exception": "Apache License 2.0 with LLVM Exceptions",
    "BSD-2-Clause": "BSD 2-Clause License",
    "BSD-3-Clause": "BSD 3-Clause License",
    "ISC": "ISC License",
    "Zlib": "zlib License",
    "Unicode-3.0": "Unicode License v3",
    "Unicode-DFS-2016": "Unicode License Agreement for Data Files and Software (2016)",
    "BSL-1.0": "Boost Software License 1.0",
    "MPL-2.0": "Mozilla Public License 2.0",
    "OFL-1.1": "SIL Open Font License 1.1",
    "Unlicense": "The Unlicense",
    "0BSD": "BSD Zero Clause License",
    "CC0-1.0": "CC0 1.0 Universal",
    "BlueOak-1.0.0": "Blue Oak Model License 1.0.0",
}

MODEL_REGISTRY = ROOT / "crates/skald-models/src/registry.rs"
TRANSCRIBE = ROOT / "third_party/transcribe-cpp-sys"
SILERO_LICENCE = ROOT / "assets/models/LICENSE-silero-vad"
FONTS = ROOT / "crates/skald-overlay/assets/fonts"
FONT_SUFFIXES = {".ttf", ".otf", ".ttc", ".woff", ".woff2"}
MODEL_SUFFIXES = {".gguf", ".bin", ".onnx", ".safetensors"}


class Failure(Exception):
    pass


# Licence texts


def flat(text):
    """The text as one lowercase line, so wording can be matched across line breaks and typography."""
    text = text.replace("“", '"').replace("”", '"').replace("‘", "'").replace("’", "'")
    return " ".join(text.split()).lower()


def uncomment(text):
    """Takes off the comment markers of a licence kept as a source file's header."""
    lines = text.replace("\r\n", "\n").replace("\r", "\n").expandtabs().split("\n")
    filled = [line for line in lines if line.strip()]
    for marker in ("//", "#", "*"):
        if filled and all(line.lstrip().startswith(marker) for line in filled):
            lines = [re.sub(r"^\s*" + re.escape(marker) + r" ?", "", line) for line in lines]
            break
    return "\n".join(line.rstrip() for line in lines)


# A sentence of each licence's own text, as `flat` writes it, which a file that only names the licence lacks.
LICENCE_PHRASES = (
    (r"terms and conditions for use, reproduction, and distribution", "Apache-2.0"),
    (r'"contributor" means each individual or legal entity that creates', "MPL-2.0"),
    (r"gnu affero general public license.*?terms and conditions", "AGPL"),
    (r"gnu lesser general public license.*?terms and conditions", "LGPL"),
    (r"gnu general public license.*?terms and conditions", "GPL"),
    (r"permission is hereby granted, free of charge, to any person or organization obtaining a copy", "BSL-1.0"),
    (r"notice to user: carefully read the following legal agreement", "Unicode"),
    (r"free and unencumbered software released into the public domain", "Unlicense"),
    (r"cc0 1\.0 universal.*?statement of purpose", "CC0-1.0"),
    (r"the purpose of this license is to give everyone as much permission to work with this", "BlueOak-1.0.0"),
    (r"the goals of the open font license \(ofl\)", "OFL-1.1"),
    (r"free of charge, to any person obtaining a copy of this software", "MIT"),
    (r"redistribution and use in source and binary forms", "BSD"),
    (r"permission to use, copy, modify, (?:and/or|and) distribute this software for any purpose", "ISC"),
    (r"altered source versions must be plainly marked", "Zlib"),
)


def classify(text):
    """The SPDX identifiers of the licences a file holds, in the order it holds them, and whether it holds their
    whole texts rather than a short notice pointing at one. A file that only says which licences apply holds
    none."""
    words = flat(uncomment(text))
    found = []
    for phrase, licence in LICENCE_PHRASES:
        match = re.search(phrase, words)
        if match:
            if licence == "Apache-2.0" and "llvm exceptions" in words:
                licence = "Apache-2.0 WITH LLVM-exception"
            elif licence == "MIT" and "shall be included" not in words:
                licence = "MIT-0"
            elif licence == "BSD":
                three = "neither the name" in words or "endorse or promote" in words
                licence = "BSD-3-Clause" if three else "BSD-2-Clause"
            elif licence == "ISC" and "provided that the above copyright notice" not in words:
                licence = "0BSD"
            elif licence == "Unicode":
                licence = "Unicode-3.0" if "unicode license v3" in words else "Unicode-DFS-2016"
            found.append((match.start(), licence))
    names = {licence for _, licence in found}
    if "MPL-2.0" in names:
        # The MPL names the GNU licences as secondary licences it can be combined with.
        found = [(start, licence) for start, licence in found if licence not in COPYLEFT]
    elif names & {"LGPL", "AGPL"}:
        # Both build on the GPL and name it in their terms.
        found = [(start, licence) for start, licence in found if licence != "GPL"]
    if found:
        return tuple(licence for _, licence in sorted(found)), True
    if 'licensed under the apache license, version 2.0 (the "license")' in words:
        return ("Apache-2.0",), False
    return (), False


def label(licences):
    return " AND ".join(licences)


def is_apache(licence):
    return licence in ("Apache-2.0", "Apache-2.0 WITH LLVM-exception")


COPYRIGHT = re.compile(
    r"^(?:(?:Copyright|COPYRIGHT|copyright)\s*(?:\([cC]\)|©|\d{4}|:\s*\S)"
    r"|Copyright\s+[A-Z(]"
    r"|\([cC]\)\s*\d{4}"
    r"|©\s*\S)"
)
PLACEHOLDER = re.compile(r"[\[<{](?:yyyy|year|name|fullname|owner|copyright)", re.IGNORECASE)
RESERVED = re.compile(r"^all rights reserved\.?$", re.IGNORECASE)


def split_copyright(text):
    """The copyright lines of a licence text, and the text without them."""
    lines, body = [], []
    for line in uncomment(text).split("\n"):
        stripped = line.strip()
        if COPYRIGHT.match(stripped):
            if not PLACEHOLDER.search(stripped):
                lines.append(stripped)
        elif not RESERVED.match(stripped):
            body.append(line)
    return lines, "\n".join(body)


def terms(licence, body):
    """The part of a licence text that binds: Apache's terms without its header and appendix, any other
    licence without a title line."""
    if is_apache(licence):
        start = body.find("TERMS AND CONDITIONS FOR USE")
        end = body.find("END OF TERMS AND CONDITIONS")
        if start >= 0 and end > start:
            return body[start : end + len("END OF TERMS AND CONDITIONS")]
        return body
    lines = body.strip().split("\n")
    title = lines[0].strip(" #*_")
    short = len(title.split()) <= 8 and not title.endswith(".")
    if short and re.search(r"licen[cs]e|^mit$", title, re.IGNORECASE):
        lines = lines[1:]
    return "\n".join(lines)


def wording(licence, text):
    """What two texts of one licence must share to be printed once: their words, apart from punctuation and
    layout."""
    words = flat(terms(licence, split_copyright(text)[1])).replace("http://", "https://")
    return " ".join(re.sub(r"\W+", " ", words).split())


def printable(licence, text):
    """A licence text as the file prints it: without comment markers, runs of blank lines or Apache's appendix."""
    body = uncomment(text)
    if is_apache(licence):
        end = body.find("END OF TERMS AND CONDITIONS")
        if end >= 0:
            body = body[: end + len("END OF TERMS AND CONDITIONS")]
    body = re.sub(r"\n{3,}", "\n\n", "\n".join(line.rstrip() for line in body.split("\n")))
    return body.strip("\n") + "\n"


def read(path):
    return path.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n")


STANDARD = {licence: read(HERE / name) for licence, name in STANDARD_TEXTS.items()}
STANDARD_WORDING = {licence: wording(licence, text) for licence, text in STANDARD.items()}


# Licence expressions


def family(licence):
    for name in COPYLEFT:
        if licence.startswith(name + "-") or licence == name:
            return name
    return licence


def alternatives(expression):
    """The ways an SPDX expression can be met: each a tuple of licences that all apply."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expression.replace("/", " OR "))
    position = 0

    def peek():
        return tokens[position] if position < len(tokens) else None

    def take():
        nonlocal position
        position += 1
        return tokens[position - 1]

    def either():
        options = both()
        while peek() == "OR":
            take()
            options = options + both()
        return options

    def both():
        options = single()
        while peek() == "AND":
            take()
            right = single()
            options = [tuple(sorted(set(a) | set(b))) for a in options for b in right]
        return options

    def single():
        if peek() == "(":
            take()
            options = either()
            if take() != ")":
                raise Failure(f"unbalanced licence expression: {expression}")
            return options
        licence = take()
        if peek() == "WITH":
            take()
            licence = f"{licence} WITH {take()}"
        return [(licence,)]

    options = either()
    if position != len(tokens):
        raise Failure(f"cannot read the licence expression: {expression}")
    return options


def rank(licence):
    return PREFERENCE.index(licence) if licence in PREFERENCE else len(PREFERENCE)


def choose(name, expression, shipped):
    """The licences taken from a crate's expression: those it ships a text for, by preference; then those whose
    standard text needs no copyright line from the package."""

    def cost(option):
        missing = [licence for licence in option if licence not in shipped]
        impossible = any(family(licence) in COPYLEFT for licence in option) or any(
            licence not in STANDARD and licence not in NO_NOTICE for licence in missing
        )
        owners = sum(1 for licence in missing if licence != "Apache-2.0" and licence not in NO_NOTICE)
        return (impossible, owners, len(missing), sorted(rank(licence) for licence in option))

    options = alternatives(expression)
    best = min(options, key=cost)
    if cost(best)[0]:
        raise Failure(
            f"{name} is offered under {expression}, and Skald can take none of it: either a copyleft licence, "
            f"or one whose text the package does not ship and scripts/notices/ does not keep"
        )
    return best, {family(licence) for option in options for licence in option} - {family(c) for c in best}


# What is credited


@dataclass
class Member:
    """One thing under a licence text: a crate, a library, a font."""

    name: str
    platforms: tuple = PLATFORM_NAMES
    offered: str = ""
    copyright: list = field(default_factory=list)
    authors: str = ""
    note: str = ""

    def lines(self):
        head = self.name
        if self.platforms != PLATFORM_NAMES:
            head += f" [{', '.join(self.platforms)}]"
        if self.offered:
            head += f", offered under {self.offered}"
        out = [f"  {head}"]
        details = list(self.copyright)
        if not details and self.authors:
            details.append(f"by {self.authors}")
        if self.note:
            details.append(self.note)
        out += [f"      {line}" for line in details]
        return out


@dataclass
class Group:
    licence: str
    text: str
    members: list = field(default_factory=list)


class Groups:
    """Members under the same licence text, with one copy of the text."""

    def __init__(self):
        self.groups = {}

    def add(self, licence, text, member, extra=()):
        """Adds a member under a licence text, or under the licence's standard text when it ships none. A text
        in the standard wording is printed once, with each member's copyright lines taken out of it and listed
        with the member; any other text is printed as it is. `extra` are copyright lines found beside the
        text."""
        if licence in NO_NOTICE:
            key, printed, lines = ("no notice", licence), None, []
        elif text is None:
            key, printed, lines = ("standard", licence), STANDARD[licence], []
        elif STANDARD_WORDING.get(licence) == wording(licence, text):
            key, printed, lines = ("standard", licence), STANDARD[licence], split_copyright(text)[0]
        else:
            printed, lines = text, []
            key = ("text", licence, " ".join(re.sub(r"\W+", " ", flat(printable(licence, text))).split()))
        member.copyright = list(dict.fromkeys(lines + list(extra)))
        printed = None if printed is None else printable(licence, printed)
        group = self.groups.setdefault(key, Group(licence, printed))
        if all(existing.name != member.name for existing in group.members):
            group.members.append(member)

    def standard(self):
        """The licences whose standard text these print."""
        return {key[1] for key in self.groups if key[0] == "standard"}

    def render(self, noun, elsewhere=None):
        """The groups as the file prints them. A standard text in `elsewhere`, a {licence: section} map, is
        named rather than printed again."""

        def order(item):
            key, group = item
            kinds = {"standard": 0, "text": 1, "no notice": 2}
            return (kinds[key[0]], rank(group.licence), group.licence, sort_key(group.members[0].name))

        out = []
        for key, group in sorted(self.groups.items(), key=order):
            group.members.sort(key=lambda member: sort_key(member.name))
            count = len(group.members)
            title = " and ".join(NAMES.get(licence, licence) for licence in group.licence.split(" AND "))
            if " AND " in group.licence:
                title += ", in one text"
            elif key[0] == "text" and self.wordings(group.licence) > 1:
                title += ", in the wording these ship"
            elif key[0] == "no notice":
                title += ", which asks for no notice"
            out += [rule("-"), f"{title}: {count} {noun if count == 1 else noun + 's'}", rule("-"), ""]
            for member in group.members:
                out += member.lines()
            if key[0] == "standard" and elsewhere and group.licence in elsewhere:
                out += ["", f"The text of the {NAMES[group.licence]} is in section {elsewhere[group.licence]}."]
            elif group.text is not None:
                out += ["", group.text.rstrip("\n")]
            out.append("")
        return out

    def wordings(self, licence):
        """How many texts of a licence these print, counting its standard text."""
        return len({key for key in self.groups if key[1] == licence}) + (licence in STANDARD and not any(
            key == ("standard", licence) for key in self.groups
        ))


def sort_key(name):
    return [int(part) if part.isdigit() else part.lower() for part in re.split(r"(\d+)", name)]


# Rust crates


LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|unlicen[cs]e|notice|copyright)", re.IGNORECASE)


def licence_files(package):
    folder = Path(package["manifest_path"]).parent
    found = [path for path in folder.iterdir() if path.is_file() and LICENCE_FILE.match(path.name)]
    declared = package.get("license_file")
    if declared and (folder / declared).is_file() and (folder / declared) not in found:
        found.append(folder / declared)
    if not found and (package["source"] or "").startswith("git+"):
        # A crate in a git repository shares the licence files at the repository's root, which Cargo checks out
        # whole.
        for parent in folder.parents:
            found = [path for path in parent.iterdir() if path.is_file() and LICENCE_FILE.match(path.name)]
            if found or (parent / ".cargo-ok").exists() or (parent / ".git").exists():
                break
    return sorted(found, key=lambda path: (path.name.lower(), path.name))


def cargo_metadata(target):
    command = ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target]
    try:
        done = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
    except FileNotFoundError:
        raise Failure("cargo is not on the PATH")
    if done.returncode != 0:
        raise Failure(f"{' '.join(command)} failed:\n{done.stderr}")
    return json.loads(done.stdout)


def linked(metadata):
    """The packages compiled into the binary: its normal dependencies, followed through everything but
    procedural macros, which run while it is built."""
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [i for i in metadata["workspace_members"] if packages[i]["name"] == BINARY]
    if len(roots) != 1:
        raise Failure(f"no single workspace member named {BINARY}")
    seen, stack = set(), list(roots)
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        if any("proc-macro" in target["kind"] for target in packages[current]["targets"]):
            continue
        seen.add(current)
        for dependency in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                stack.append(dependency["pkg"])
    members = set(metadata["workspace_members"])
    return {i: packages[i] for i in seen if i not in members}


def crates():
    """Every crate in a release build, with the platforms it is built for."""
    found = {}
    for name, target in PLATFORMS:
        for package in linked(cargo_metadata(target)).values():
            key = (package["name"], package["version"], package["source"])
            found.setdefault(key, (package, []))[1].append(name)
    return [(package, tuple(platforms)) for package, platforms in found.values()]


def add_crate(groups, package, platforms):
    name = f"{package['name']} {package['version']}"
    expression = package.get("license")
    if not expression:
        raise Failure(f"{name} declares no licence")
    files = [(path, *classify(read(path))) for path in licence_files(package)]
    shipped = {licence for _, licences, whole in files if whole for licence in licences}
    chosen, rejected = choose(name, expression, shipped)
    authors = ", ".join(re.sub(r"\s*<[^>]*>", "", author).strip() for author in package.get("authors") or [])
    offered = expression if expression.strip() not in chosen else ""

    def member(note=""):
        return Member(name, platforms, offered, authors=authors, note=note)

    # Texts of the licences taken, and of any other the package names for code it carries; short Apache
    # notices and files that only say which licences apply give their copyright lines.
    texts, notices, loose = [], [], []
    for path, licences, whole in files:
        text = read(path)
        if whole:
            if any(family(licence) not in rejected | COPYLEFT for licence in licences):
                texts.append((licences, text))
        elif licences and licences[0] in chosen:
            notices += split_copyright(text)[0]
        elif not licences:
            loose += split_copyright(text)[0]
    covered = {licence for licences, _ in texts for licence in licences}
    first = True
    for licences, text in texts:
        extra = (notices if "Apache-2.0" in licences else []) + (loose if first else [])
        first = False
        groups.add(label(licences), text, member(), extra)
    for licence in chosen:
        if licence not in covered:
            extra = (notices if licence == "Apache-2.0" else []) + (loose if first else [])
            first = False
            note = "" if licence in NO_NOTICE else "the package carries no licence text; the standard text applies"
            groups.add(licence, None, member(note), extra)


# The parts written by hand


@dataclass
class Model:
    name: str
    card: str
    credit: str
    availability: str


# The attribution each model card asks for (docs/research.md, Licences). Every model in Skald's model choice is
# here: the check fails when crates/skald-models/src/registry.rs lists a model card this does not.
MODELS = (
    Model(
        "NB-Whisper Large",
        "https://huggingface.co/NbAiLab/nb-whisper-large",
        "By the National Library of Norway: a product of the NoSTram project led by Per Egil Kummervold, with "
        "Javier de la Rosa, Freddy Wetjen and Rolv-Arild Braaten as key contributors, completed with the "
        "support of NB AI-Lab under the direction of Svein Arne Brygfjeld. Trained further from OpenAI's "
        "Whisper large-v3.",
        "Skald's default model. On first run Skald downloads the National Library's own ggml-model.bin from this "
        "repository, at a pinned revision whose SHA-256 it checks.",
    ),
    Model(
        "NB-ASR Bokfink (beta)",
        "https://huggingface.co/NbAiLab/nb-asr-qwen06b-bokfink02-beta",
        "By the NB-ASR project at the National Library of Norway, with the work of its data, infrastructure and "
        "evaluation teams, on the Qwen3-ASR architecture by Alibaba's Qwen team.",
        "Listed in Skald's model choice. Skald runs it as a GGUF file, which the National Library does not "
        "publish, so it is not offered for download yet.",
    ),
    Model(
        "Nordlyd",
        "https://huggingface.co/NbAiLab/nordlyd-asr-qwen06B-nov26preview",
        "By the National Library of Norway.",
        "Shown greyed in Skald's model choice until the National Library opens it to everyone.",
    ),
)


def check_models():
    listed = set(re.findall(r'card:\s*"([^"]+)"', read(MODEL_REGISTRY)))
    credited = {model.card for model in MODELS}
    if listed != credited:
        missing = ", ".join(sorted(listed - credited)) or "none"
        gone = ", ".join(sorted(credited - listed)) or "none"
        raise Failure(
            f"the models credited here differ from {MODEL_REGISTRY.relative_to(ROOT)}: not credited: {missing}; "
            f"no longer listed: {gone}. Change MODELS in scripts/notices/notices.py."
        )


def evidence(path, line):
    """A copyright line written here by hand, checked against the source file it is copied from."""
    if line not in read(path):
        raise Failure(f"{path.relative_to(ROOT)} no longer says '{line}'; check its notice in scripts/notices/")
    return line


def engine(groups):
    ggml = TRANSCRIBE / "ggml"
    entries = (
        ("transcribe.cpp", "MIT", TRANSCRIBE / "LICENSE", [], PLATFORM_NAMES),
        ("ggml", "MIT", ggml / "LICENSE", [], PLATFORM_NAMES),
        (
            "whisper.cpp and llama.cpp, the code transcribe.cpp takes from them",
            "MIT",
            None,
            ["Copyright (c) 2023-2026 The ggml authors"],
            PLATFORM_NAMES,
        ),
        (
            "llamafile's tinyBLAS, in ggml's CPU backend",
            "MIT",
            None,
            [evidence(ggml / "src/ggml-cpu/llamafile/sgemm.cpp", "Copyright 2024 Mozilla Foundation")],
            PLATFORM_NAMES,
        ),
        (
            "YaRN's rotary scaling, in ggml's CPU and Metal backends",
            "MIT",
            None,
            [evidence(ggml / "src/ggml-cpu/ops.cpp", "Copyright (c) 2023 Jeffrey Quesnelle and Bowen Peng")],
            PLATFORM_NAMES,
        ),
        ("miniz", "MIT", TRANSCRIBE / "src/third_party/miniz/LICENSE", [], PLATFORM_NAMES),
        # As in LunarG's Vulkan SDK 1.4.350.0, which windows.yml builds against: vulkan_core.h is Apache 2.0,
        # vulkan.hpp Apache 2.0 or MIT, spirv.hpp MIT. A Linux build takes the distribution's copies of the same
        # headers.
        (
            "Vulkan-Headers, with Vulkan-Hpp, in ggml's Vulkan backend",
            "Apache-2.0",
            None,
            ["Copyright 2015-2026 The Khronos Group Inc."],
            ("Windows", "Linux"),
        ),
        (
            "SPIRV-Headers, in ggml's Vulkan backend",
            "MIT",
            None,
            ["Copyright 2014-2024 The Khronos Group Inc."],
            ("Windows", "Linux"),
        ),
    )
    for name, licence, path, copyright, platforms in entries:
        text = read(path) if path else None
        if text is not None and classify(text) != ((licence,), True):
            raise Failure(f"{path.relative_to(ROOT)} is not the {licence} text it is credited with")
        groups.add(licence, text, Member(name, platforms), copyright)


def check_bundled():
    """Every font and model file in the repository is one credited here."""
    credited = {FONTS / "AlegreyaSans-Regular.ttf", FONTS / "AlegreyaSans-Medium.ttf"}
    credited.add(ROOT / "assets/models/silero-vad-v6.2-F32.gguf")
    for top in (ROOT / "crates", ROOT / "assets"):
        for path in top.rglob("*"):
            if path.suffix.lower() in FONT_SUFFIXES | MODEL_SUFFIXES and path not in credited:
                raise Failure(f"{path.relative_to(ROOT)} is not credited in scripts/notices/notices.py")
    for path in credited:
        if not path.is_file():
            raise Failure(f"{path.relative_to(ROOT)} is credited in scripts/notices/notices.py but missing")


# The file


def rule(character="="):
    return character * WIDTH


def prose(*paragraphs, indent=""):
    out = []
    for paragraph in paragraphs:
        out += textwrap.wrap(
            paragraph,
            WIDTH,
            initial_indent=indent,
            subsequent_indent=indent,
            break_long_words=False,
            break_on_hyphens=False,
        )
        out.append("")
    return out


def heading(number, title):
    return [rule(), f"{number}. {title.upper()}", rule(), ""]


def render():
    check_models()
    check_bundled()
    sections = (
        "Speech models",
        "transcribe.cpp, ggml, whisper.cpp and llama.cpp",
        "Silero VAD",
        "Alegreya Sans",
        "Rust crates",
    )
    # The crates come first, so the other sections can point at the standard texts they print.
    rust = Groups()
    def by_name(item):
        return sort_key(f"{item[0]['name']} {item[0]['version']}")

    for package, platforms in sorted(crates(), key=by_name):
        add_crate(rust, package, platforms)
    elsewhere = {licence: len(sections) for licence in rust.standard()}

    out = ["THIRD-PARTY NOTICES FOR SKALD", ""]
    out += prose(
        "Skald is free software under the MIT License (LICENSE). It is built on the work of others, credited "
        "here: the speech models it downloads, the speech engine and the other code in its program, and the "
        "font it shows.",
        "scripts/notices/notices.py writes this file from Cargo.lock and the files it names, and the notices "
        "check in CI fails when it is out of date; change the script rather than this file.",
    )
    out += [f"  {number}. {title}" for number, title in enumerate(sections, 1)] + [""]

    out += heading(1, sections[0])
    out += prose(
        "Skald's speech models are made by the National Library of Norway (Nasjonalbiblioteket) and published "
        "under the Apache License 2.0. Skald does not include or redistribute them: it downloads a model from the "
        "National Library's own repository on Hugging Face when the user asks for it."
    )
    for model in MODELS:
        out += [model.name] + prose(model.credit, indent="  ")[:-1]
        out += [f"  {model.card}", "  Apache License 2.0."] + prose(model.availability, indent="  ")
    if "Apache-2.0" in elsewhere:
        out += [f"The text of the Apache License 2.0 is in section {elsewhere['Apache-2.0']}.", ""]
    else:
        out += [STANDARD["Apache-2.0"]]

    out += heading(2, sections[1])
    out += prose(
        "Skald transcribes with transcribe.cpp by handy-computer "
        "(https://github.com/handy-computer/transcribe.cpp), compiled into the program from the "
        "transcribe-cpp-sys crate with two patches of Skald's (third_party/transcribe-cpp-sys/SKALD.md). "
        "transcribe.cpp computes with ggml (https://github.com/ggml-org/ggml), takes its FFT from whisper.cpp "
        "(https://github.com/ggml-org/whisper.cpp) and its Unicode tables and pretokenizer from llama.cpp "
        "(https://github.com/ggml-org/llama.cpp), and carries miniz. On Windows and Linux ggml's Vulkan backend "
        "is compiled with the Khronos Group's headers; on Windows ggml's backends are modules beside skald.exe."
    )
    groups = Groups()
    engine(groups)
    out += groups.render("part", elsewhere)

    out += heading(3, sections[2])
    out += prose(
        "Skald's program carries Silero VAD v6.2 by the Silero Team (https://github.com/snakers4/silero-vad), the "
        "speech detection behind its silence check, as the GGUF file handy-computer converted "
        "(assets/models/README.md). transcribe.cpp carries the model's segmentation, ported from Silero VAD 6.2.3."
    )
    groups = Groups()
    silero = read(SILERO_LICENCE)
    groups.add("MIT", silero, Member("Silero VAD"))
    out += groups.render("part", elsewhere)

    out += heading(4, sections[3])
    out += prose(
        "The overlay's Runepinne and Smidd jern themes set their text in Alegreya Sans Regular and Medium "
        "(https://github.com/huertatipografica/Alegreya-Sans), which Skald's program carries, unchanged "
        "(crates/skald-overlay/assets/fonts/README.md)."
    )
    groups = Groups()
    ofl = read(FONTS / "OFL.txt")
    groups.add("OFL-1.1", ofl, Member("Alegreya Sans"))
    out += groups.render("font", elsewhere)

    out += heading(5, sections[4])
    out += prose(
        "The Rust crates compiled into the skald program for macOS ({}), Windows ({}) and Linux ({}), as "
        "Cargo.lock resolves them. A crate built for some of these only names them in brackets. Build "
        "dependencies, procedural macros and test-only crates run while Skald is built and are not part of the "
        "program, so they are left out; Skald's own crates are under its MIT License.".format(
            *(target for _, target in PLATFORMS)
        ),
        "Where a crate is offered under a choice of licences, Skald takes the one it is listed under. Crates "
        "under the same licence text share one copy of it; the copyright lines, which differ from crate to "
        "crate, stand with each crate, or its authors where its licence text names none.",
    )
    out += rust.render("crate")

    while out and not out[-1]:
        out.pop()
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--check", action="store_true", help="fail when THIRD-PARTY-NOTICES is out of date")
    arguments = parser.parse_args()
    try:
        written = render()
    except Failure as failure:
        print(f"notices: {failure}", file=sys.stderr)
        return 1
    if arguments.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current == written:
            return 0
        diff = difflib.unified_diff(
            current.splitlines(), written.splitlines(), "THIRD-PARTY-NOTICES", "expected", lineterm=""
        )
        print("\n".join(list(diff)[:80]), file=sys.stderr)
        print(
            "notices: THIRD-PARTY-NOTICES is out of date; run python3 scripts/notices/notices.py and commit it",
            file=sys.stderr,
        )
        return 1
    with open(OUTPUT, "w", encoding="utf-8", newline="\n") as file:
        file.write(written)
    return 0


if __name__ == "__main__":
    sys.exit(main())

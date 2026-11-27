# transcribe-cpp-sys, as Skald builds it

The `transcribe-cpp-sys` crate 0.5.1 as published on crates.io (SHA-256
`323903fad2bcebaf025314ccb5dc3e178ba8e61622fc935fe17096d5ab93244b`), built from
[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) commit
`4a4e08f99c62ae1036551c259477fe4f3127f092` (v0.5.1) by handy-computer, MIT (`LICENSE`; ggml and the other
code it bundles in `THIRD-PARTY-LICENSES.md`). The root `Cargo.toml` puts this copy in place of the
crates.io one, so the safe `transcribe-cpp` crate builds on it.

It carries two patches, in `third_party/patches/transcribe-cpp-sys/`, each explained at its top:

1. `0001-whisper-carried-prompt-budget.patch`: a prompt carried into every 30-second window shares its
   223 tokens with the text before the window, the prompt first, as in whisper.cpp. Without it a full
   ordskatt breaks long takes (docs/research/engine-check.md, section 2).
2. `0002-qwen3-asr-language-names.patch`: a Qwen3-ASR GGUF names its own languages, which Bokfink needs
   for `language Norwegian<asr_text>` and `language Norwegian Nynorsk<asr_text>`
   (docs/research/bokfink-gguf.md).

Nothing else differs from the published crate. `third_party/vendor-transcribe-cpp-sys.sh` writes this
folder from scratch: it downloads the crate, checks its SHA-256 and applies the patches. Run it on an
unchanged checkout and `git status` stays clean.

The copy goes when an upstream release carries both changes, or changes that do the same; then Skald pins
that release from crates.io again and drops the root `Cargo.toml` patch (docs/decisions.md, D-23). Until
then a new transcribe.cpp release means changing the version and hash in the script, running it, and
fixing any patch that does not apply to it.

//! `skald bench` before the `skald` binary has the command:
//!
//! ```sh
//! cargo run --release -p skald-bench --example bench -- <folder> [options]
//! ```

use std::process::ExitCode;

fn main() -> ExitCode {
    skald_bench::run(std::env::args_os().skip(1))
}

//! On Windows the executable carries Skald's icon, which Explorer, the Start menu and GPUI's windows show.

#[path = "build/windows_icon.rs"]
mod windows_icon;

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    // MSVC's linker takes a compiled resource file as it is; other Windows linkers would need a tool to
    // turn it into an object first.
    if os == "windows" && env == "msvc" {
        windows_icon::embed();
    }
}

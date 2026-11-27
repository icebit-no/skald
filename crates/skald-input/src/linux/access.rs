//! The permission macOS calls Accessibility, which Linux does not have: the hold key comes from the
//! desktop's key binding, which needs no grant, so it always reads as granted and there is nothing to ask
//! for or open.

pub fn accessibility_granted() -> bool {
    true
}

pub fn request_accessibility() -> bool {
    true
}

pub fn open_accessibility_settings() -> std::io::Result<()> {
    Ok(())
}

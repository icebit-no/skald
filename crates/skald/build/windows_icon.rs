//! The icon as a compiled resource file, made from the PNG files in `assets/icon` at every build so the icon
//! has one source, as the macOS bundle's `.icns` has (D-34). Each size is an icon image of its own holding the
//! PNG as it is, which Windows reads from Vista on; the group that names them takes id 1, the icon Windows and
//! GPUI look for first.

use std::env;
use std::fs;
use std::path::PathBuf;

const SIZES: [u32; 5] = [16, 32, 64, 128, 256];

/// A resource's type, by its number in the Windows SDK.
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// Movable, pure and discardable, as the resource compiler marks icons.
const ICON_FLAGS: u16 = 0x1010;
const GROUP_FLAGS: u16 = 0x1030;

pub(crate) fn embed() {
    let icons = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
        .join("../../assets/icon");
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| {
            let path = icons.join(format!("app-icon-{size}.png"));
            println!("cargo:rerun-if-changed={}", path.display());
            let png = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(
                png_size(&png),
                Some((size, size)),
                "{} is not {size} × {size}",
                path.display()
            );
            (size, png)
        })
        .collect();

    let mut res = Vec::new();
    // A resource file opens with an empty resource that says what it is.
    resource(&mut res, 0, 0, 0, &[]);
    for (index, (_, png)) in images.iter().enumerate() {
        resource(&mut res, RT_ICON, index as u16 + 1, ICON_FLAGS, png);
    }
    resource(&mut res, RT_GROUP_ICON, 1, GROUP_FLAGS, &group(&images));

    let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo")).join("skald.res");
    fs::write(&out, res).unwrap_or_else(|error| panic!("{}: {error}", out.display()));
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// The width and height a PNG's header gives.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.get(..8)? != b"\x89PNG\r\n\x1a\n" || png.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

/// One resource: its header, with the type and name as numbers, then its data, padded to four bytes.
fn resource(res: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    const HEADER_SIZE: u32 = 32;
    res.extend((data.len() as u32).to_le_bytes());
    res.extend(HEADER_SIZE.to_le_bytes());
    res.extend(
        [0xFFFFu16, kind, 0xFFFF, name]
            .iter()
            .flat_map(|word| word.to_le_bytes()),
    );
    res.extend(0u32.to_le_bytes()); // data version
    res.extend(flags.to_le_bytes());
    res.extend(0u16.to_le_bytes()); // language: neutral
    res.extend(0u32.to_le_bytes()); // version
    res.extend(0u32.to_le_bytes()); // characteristics
    res.extend(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

/// The group icon's directory: one entry per image, by the image's resource id. A side of 256 is written as
/// 0, since the field holds a byte.
fn group(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut group = Vec::new();
    group.extend(
        [0u16, 1, images.len() as u16]
            .iter()
            .flat_map(|word| word.to_le_bytes()),
    );
    for (index, (size, png)) in images.iter().enumerate() {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        group.extend([side, side, 0, 0]);
        group.extend(1u16.to_le_bytes()); // planes
        group.extend(32u16.to_le_bytes()); // bits per pixel
        group.extend((png.len() as u32).to_le_bytes());
        group.extend((index as u16 + 1).to_le_bytes());
    }
    group
}

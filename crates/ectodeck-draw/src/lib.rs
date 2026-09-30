//! Ectodeck's drawing of a device from its core's art (the core protocol's
//! draft 4): the pieces of one SVG, lit key by key, as one HDR picture.

pub mod hdr;

/// The Z10 core's art, from its own folder. Until the Z10 core runs as its
/// own program, Ectodeck reads that folder when it is built.
pub mod z10 {
    pub const SVG: &str = include_str!("../../../cores/evga-z10/art/z10.svg");
    pub const DEVICE: &str = include_str!("../../../cores/evga-z10/device.json");
}

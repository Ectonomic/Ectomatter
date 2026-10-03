//! Ectodeck's drawing of a device from its core's art (the core protocol's
//! draft 4): the pieces of one SVG, lit key by key, as one HDR picture.

pub mod hdr;
pub mod pieces;

/// A real device for the tests to draw: the Z10's core, read from its own
/// folder. The drawing itself knows no device; it draws what it is given.
#[cfg(test)]
pub(crate) mod fixture {
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10");

    pub fn svg() -> String {
        std::fs::read_to_string(format!("{DIR}/art/z10.svg")).expect("the Z10 core's art")
    }

    pub fn device() -> String {
        std::fs::read_to_string(format!("{DIR}/device.json")).expect("the Z10 core's device description")
    }
}

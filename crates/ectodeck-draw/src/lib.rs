//! Ectodeck's drawing of a device from its core's art and description (the
//! core protocol's draft 6): the pieces of one SVG, lit key by key, as one
//! HDR picture; and the description's own format, JSON with comments (jsonc).

pub mod hdr;
pub mod jsonc;
pub mod pieces;
pub mod print;

/// A real device for the tests to draw: the Z10's core, read from its own
/// folder. The drawing itself knows no device; it draws what it is given.
#[cfg(test)]
pub(crate) mod fixture {
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10");

    pub fn svg() -> String {
        std::fs::read_to_string(format!("{DIR}/art/z10.svg")).expect("the Z10 core's art")
    }

    /// its device description, as its file has it (JSON with comments)
    pub fn device() -> String {
        std::fs::read_to_string(format!("{DIR}/device.json")).expect("the Z10 core's device description")
    }

    /// The typefaces, the core's own among them.
    pub fn faces() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
        static ADDED: std::sync::Once = std::sync::Once::new();
        ADDED.call_once(|| crate::print::add_fonts(["BarlowSemiCondensed-SemiBold.ttf", "Z10Symbols-Bold.ttf"].map(|f| std::path::PathBuf::from(format!("{DIR}/fonts/{f}")))));
        crate::print::faces()
    }
}

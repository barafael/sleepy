//! Gives the Windows executable its hourglass; elsewhere, does nothing.
//!
//! The window icon is set at runtime from the PNG; an .exe carries its
//! icon in a resource table, which is a link-time affair - so the .ico
//! is embedded here, and only here does the build need to know it.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        // A package whose own icon cannot be embedded is broken, and
        // should fail the build rather than ship a blank tile.
        #[allow(clippy::expect_used)]
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("cannot embed assets/icon.ico");
    }
}

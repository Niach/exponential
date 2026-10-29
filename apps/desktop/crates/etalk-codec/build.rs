//! NEW-88: `libopus` links the system's libopus (pkg-config `opus`).

fn main() {
    #[cfg(feature = "libopus")]
    if let Err(err) = pkg_config::Config::new().atleast_version("1.1").probe("opus") {
        panic!("the `libopus` feature needs the system libopus (Debian/Ubuntu: libopus-dev, Fedora: opus-devel, macOS: brew install opus): {err}");
    }
}

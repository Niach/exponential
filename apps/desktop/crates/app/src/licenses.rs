//! The third-party notice, compiled into the binary (EXP-375 / EXP-376).
//!
//! The desktop app is distributed as a single executable — inside a `.app`
//! bundle, an `.AppImage`, or as a bare Windows `.exe`. `include_str!` is what
//! makes the notice travel with the executable itself rather than with the
//! container around it, which matters because the `.exe` has no container.
//!
//! It also closes the in-binary gap for the assets `assets.rs` embeds:
//! `assets/fonts/**/*.ttf` is ~3.6 MB of OFL-licensed Font Software, and
//! `assets/icons/**/*.svg` is Lucide's ISC-licensed geometry, but the
//! `LICENSE.txt` files sitting next to them are NOT in the rust-embed include
//! list. The generated `NOTICES.txt` reproduces both bodies in full, so
//! embedding this one file discharges OFL section 2 and the ISC notice without
//! widening that include list.
//!
//! The file is GENERATED — `bun run --filter @exp/licenses generate`. It is
//! byte-compared on every PR by `apps/web/src/lib/licenses.test.ts`, and the
//! `include_str!` below means a build cannot even succeed without it.

/// The full third-party notice for this build, as shipped.
pub const NOTICES: &str = include_str!("../../../assets/licenses/NOTICES.txt");

/// The Apache-2.0 text the app itself is licensed under.
pub const LICENSE: &str = include_str!("../../../../../LICENSE");

/// Our own attribution notice (Apache-2.0 section 4(d)).
pub const NOTICE: &str = include_str!("../../../../../NOTICE");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_are_embedded_and_complete() {
        assert!(
            NOTICES.len() > 100_000,
            "NOTICES.txt looks truncated ({} bytes) — regenerate with \
             `bun run --filter @exp/licenses generate`",
            NOTICES.len()
        );
        for expected in [
            "EXPONENTIAL — THIRD-PARTY NOTICES",
            "Desktop application",
            "Open-source components",
            "Licence election",
            "Trademarks",
        ] {
            assert!(NOTICES.contains(expected), "NOTICES.txt is missing {expected:?}");
        }
    }

    /// The whole reason this module exists: the fonts and icons compiled in by
    /// `assets.rs` have licence obligations that only this file discharges.
    #[test]
    fn embedded_assets_are_covered() {
        assert!(
            NOTICES.contains("SIL OPEN FONT LICENSE Version 1.1"),
            "the embedded Inter/JetBrains Mono TTFs need the OFL reproduced"
        );
        assert!(
            NOTICES.contains("Lucide Contributors 2022"),
            "the embedded Lucide SVGs need the ISC notice reproduced"
        );
    }

    #[test]
    fn our_own_licence_travels_too() {
        assert!(LICENSE.contains("Apache License"));
        assert!(LICENSE.contains("Version 2.0, January 2004"));
        assert!(NOTICE.contains("Exponential"));
    }
}

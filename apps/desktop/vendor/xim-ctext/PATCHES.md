# Vendored xim-ctext (Exponential patch, #ERR-18)

Source: the `xim-ctext` crate 0.3.0 as published on crates.io (the
COMPOUND_TEXT codec of https://github.com/Riey/xim-rs, by Riey, MIT,
`LICENSE`). The published crate's `src/main.rs` scratch binary is left out.

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`; gpui's
Linux backend reaches it through `zed-xim` 0.4.0-zed, which stays a crates.io
dependency. Drop the patch once a release decodes COMPOUND_TEXT without
errors (or zed-xim stops `expect`ing the result).

## Changes

Marked `Exponential patch (#ERR-18)` in `src/lib.rs`.

- **`compound_text_to_utf8` never fails.** Upstream decoded text without an
  escape as UTF-8 and returned an error for every escape but UTF-8
  (`ESC % G`) and JIS X 0208 (`ESC $ ( B`). zed-xim `expect`s the result for
  commits and preedit, so an input method committing "ü" as COMPOUND_TEXT
  (the single byte 0xFC: ISO 8859-1 is the format's default right half)
  panicked the desktop IDE on the main thread. Now: escape-free valid UTF-8
  still decodes as UTF-8 (unchanged); otherwise GL starts as ASCII and GR as
  ISO 8859-1, `ESC - F` designates ISO 8859-1/2/3/4/5/7/8/9 to GR, `ESC $ ( F`
  / `ESC $ ) F` GB 2312, JIS X 0208 or KS C 5601 (decoded in their EUC form),
  `ESC % G … ESC % @` is lossy UTF-8, CSI direction sequences and unknown
  escapes are skipped. A truncated sequence ends the text instead of erroring
  (upstream also panicked on a UTF-8 run shorter than its end escape).

# FINDINGS · core (taffy in Rust, macOS host)

Measured 2026-09-28 on the M-series MacBook Pro, release build, `cargo run -p vapp-spike --release --example dump_frames`.

## Layout time, fixed fake measure (no FFI, no fonts)

| surface | nodes | measure calls | taffy layout | parse + build |
|---|---|---|---|---|
| kitchen sink @ 900 | 48 | 406 | 150 µs | ~120 µs |
| bench @ 390 (1 column) | 203 | 2992 | 461 to 471 µs | 230 to 244 µs |
| bench @ 900 (3 columns) | 203 | 2596 | 436 to 453 µs | 228 to 237 µs |

Best-of-5 spread shown. The number that matters for the natives is the MEASURE CALL COUNT: taffy asks a leaf several times per pass (min-content, max-content, definite available width, then the final known-width pass) and its per-node cache only dedupes identical requests. ~15 calls per leaf on the bench tree (133 leaves) and ~13 per leaf on the kitchen sink. Every one of those becomes an FFI crossing plus a native text measurement on iOS/Android. The in-process cost is negligible (0.5 ms for 200 nodes); the FFI + text-shaping cost is what the phone numbers must show.

Mitigations if the phone numbers miss the 2 ms budget (VAPP-42 acceptance):
- host-side memo per (index, known, available) for the lifetime of a content version; the second pass at the same width then costs zero measure calls;
- explicit `width: 100%` / fixed heights on leaves inside grids (both known ⇒ no call);
- taffy's `compute_layout` on a tree whose leaf sizes were captured once ("measure snapshot"), re-laying out only on content or width change.

## RTL (taffy 0.12.2)

`Style::direction` exists and is honoured by flexbox, grid, block and absolute placement (`compute/flexbox.rs:457`, `compute/grid/placement.rs`, `compute/mod.rs:144`). It is a PER-NODE field, not inherited: the core copies the root's `direction` onto every node. Verified by `rtl_mirrors_every_frame` (390 and 900 px): every flow node's x mirrors exactly (`x' = W - x - w`).

Physical properties stay physical, exactly like CSS: `right: 8` on the absolute badge keeps it on the right edge under RTL, `marginLeft: auto` keeps pushing to the right. Only logical properties (`insetInlineEnd`, `marginInlineStart`) would flip, and those are not in taffy 0.12's `Style` at all (its inset/margin/padding are physical `Rect`s). Whitelist consequence: v1 offers PHYSICAL `top/right/bottom/left` and `margin*`; a logical-property author who needs RTL-aware insets must do it in the runtime (swap left/right when `direction` is rtl) or wait for taffy support. That is the same trade React Native made until 0.70.

## Semantics the web mapper must reproduce (or the frames diverge)

- `Layout.location` is parent-relative; the core accumulates. Absolute children are positioned against their PARENT box, not the nearest positioned ancestor: every Box gets `position: relative` on web.
- taffy defaults: `display: flex`, `flex-direction: row`, `box-sizing: border-box`, `flex-shrink: 1`, `align-items: stretch`. The web base rule emits those.
- A measured leaf reports its CONTENT box; taffy adds the leaf's padding/border. Leaf styles therefore carry no padding (native controls are border-box).
- Containers (`box`, `card`) are never measured; a childless container has size 0 unless styled (the `footer-spacer` uses `height: 1`).

## Whitelist after the core

`vapp_spike::WHITELIST` (lib.rs) is the candidate. Everything in the fixture resolves through it (`fixture_uses_only_whitelisted_keys`). Grid strings parsed by hand (`tracks.rs`): `px`, bare numbers, `%`, `fr`, `auto`, `min-content`, `max-content`, `minmax()`, `repeat(n|auto-fill|auto-fit, …)`; areas from string rows (rectangular only); `gridArea: name`, `gridColumn/gridRow`: `n`, `a / b`, `span n`, name. Not supported and not needed for v1: `calc()`, named lines in track lists, `grid-auto-flow`, `justify-items`, `order`, `z-index`, logical properties, `inset` shorthand with multiple values, `padding: "a b"` shorthands (single value or per-side only), `boxShadow` beyond one string token.

Paint order note: CSS paints positioned elements above in-flow siblings; the natives paint pre-order. The fixture rule "absolute nodes last among their siblings" makes both agree; the protocol should enforce that ordering (or sort positioned nodes last per parent) instead of adding `z-index`.

## Send-ness

taffy's `CompactLength` carries a raw pointer slot for `calc()`; `Style` is therefore `!Send`. The core never builds calc values, so `Surface` is `unsafe impl Send` for the `Mutex` in the UniFFI object. `vapp-client` should either keep that impl or ask taffy for a `Send` calc representation.

## UniFFI facade facts (uniffi 0.32.2)

- Foreign trait `#[uniffi::export(with_foreign)] trait Measure` works for Swift and Kotlin; the callback carries a small record with the node INDEX (no strings).
- `--library` bindgen mode reads the host `.dylib`; Swift module `VappSpikeFFI`, C module `VappSpikeFFIFFI` (rename `VappSpikeFFIFFI.modulemap` to `module.modulemap` for the xcframework).
- Sizes: iOS static lib 20 MB unstripped archive (the linked contribution is what counts; VAPP-3's opt-z number is 418 KB for taffy alone); Android arm64 `.so` 950 KB with LTO + strip. Kotlin needs JNA 5.17 (`@aar`).
- Licence: uniffi is MPL-2.0, already allowed in `apps/desktop/deny.toml`; the mobile facade lives in a standalone workspace (`spikes/vapp-ffi`) so uniffi never enters the desktop `Cargo.lock` that CI builds `--locked`.

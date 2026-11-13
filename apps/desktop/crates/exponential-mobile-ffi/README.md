# `exponential-mobile-ffi`: the phone apps' one Rust library

#PERF-11. The Exponential Android and iOS apps link three UniFFI facades:
genUI's core ([`exponential-ui-ffi`](../exponential-ui-ffi)), the Engine
([`eengine-ffi`](../eengine-ffi)) and the P2P node
([`exponential-net-ffi`](../exponential-net-ffi)). Shipped as three libraries,
each carried its own Rust std and its own copy of everything they share
(serde, tokio, uniffi, ...). This crate links the three into ONE library;
the facades stay separate SDK crates, with their own bindings and releases.

| | Android | iOS |
|---|---|---|
| build | `build-android.sh` (the app's `buildMobileFfi` gradle task) | `build-ios.sh` |
| output | `out/jniLibs/<abi>/libexponential_mobile_ffi.so` | `out/ExponentialMobileFFI.xcframework`, linked into ExpCore |
| bindings | the facades' committed Kotlin, pointed here by `MobileNativeLibrary` (`uniffi.component.<namespace>.libraryOverride`) | the facades' committed Swift; the symbols resolve against ExpCore |

Each facade keeps its UniFFI namespace, so their scaffolding symbols never
collide. On iOS `build-ios.sh` also writes header-only link stubs (marked
`LINK_STUB`) where the `ExponentialUI` and `ExponentialEngine` Swift packages
look for their xcframework, so the app does not link their code twice. The
facades' own `build-ios.sh` put the standalone libraries back (SDK tests, the
iOS runner).

Features are the ones both apps shipped before: genUI with its built-in
themes, the Engine without its bundled sqlite, the full P2P node with its
persistent blob store. The desktop app and the runners link the facades
directly, as before.

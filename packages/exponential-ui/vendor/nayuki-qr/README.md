# Attribution: Project Nayuki's QR Code generator library

`src/qr-code.ts` (VAPP-162, the QrCode native's matrix) is a TypeScript port
of Project Nayuki's QR Code generator library
(https://www.nayuki.io/page/qr-code-generator-library,
https://github.com/nayuki/QR-Code-generator), narrowed to one byte-mode
segment, the exact error-correction level (no `boostEcl`) and the automatic
mask. The Rust core's port (`apps/desktop/crates/exponential-ui`) is a second
port of the same algorithm. Licence: MIT, Copyright (c) Project Nayuki
(`LICENSE.txt` here). Recorded in `docs/third-party-licences.md`.

/**
 * The pure half of the web lanes' eNet step (EXP-1842): `scripts/build-enet-wasm.sh`
 * needs wasm-bindgen-cli at EXACTLY the `wasm-bindgen` version in
 * `apps/desktop/Cargo.lock`, the wasm32 target and rustup's llvm-tools. A Mac
 * whose `~/.cargo/bin/wasm-bindgen` is another version (another repo pins its
 * own) used to fail every fresh checkout's web lane; the lane now installs the
 * pinned CLI into a versioned root under the cargo target and puts it first on
 * the script's PATH, never touching the global one.
 */
import { join } from "node:path"

/** `apps/desktop/Cargo.lock`'s `wasm-bindgen` version, as the script reads it. */
export function wasmBindgenLockVersion(cargoLock: string): string | undefined {
  const match = /^name = "wasm-bindgen"\n+version = "([^"]+)"/m.exec(cargoLock)
  return match?.[1]
}

/** The version a `wasm-bindgen --version` line names (`wasm-bindgen 0.2.130`). */
export function wasmBindgenCliVersion(output: string): string | undefined {
  return /^wasm-bindgen\s+(\S+)/m.exec(output.trim())?.[1]
}

/**
 * Where the lane installs the pinned CLI (`cargo install --root`): one root per
 * version, inside the cargo target so it is shared across checkouts and goes
 * away with a `cargo clean`.
 */
export function wasmBindgenToolRoot(cargoTarget: string, version: string): string {
  return join(cargoTarget, `shots-tools`, `wasm-bindgen-${version}`)
}

/** The rustup steps the script's own checks name, run on the pinned toolchain. */
export const ENET_RUSTUP_STEPS = [
  [`rustup`, `target`, `add`, `wasm32-unknown-unknown`],
  [`rustup`, `component`, `add`, `llvm-tools`],
] as const

import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { wasmBindgenCliVersion, wasmBindgenLockVersion, wasmBindgenToolRoot } from "./enet.ts"

describe(`the pinned wasm-bindgen-cli the web lane installs`, () => {
  test(`reads the lock's wasm-bindgen, not wasm-bindgen-macro`, () => {
    const lock = [
      `[[package]]`,
      `name = "wasm-bindgen-macro"`,
      `version = "0.2.99"`,
      ``,
      `[[package]]`,
      `name = "wasm-bindgen"`,
      `version = "0.2.130"`,
      `source = "registry+https://github.com/rust-lang/crates.io-index"`,
    ].join(`\n`)
    expect(wasmBindgenLockVersion(lock)).toBe(`0.2.130`)
    expect(wasmBindgenLockVersion(`name = "serde"\nversion = "1"`)).toBeUndefined()
  })

  test(`the repo's own lock pins one`, () => {
    const lock = readFileSync(join(import.meta.dir, `../../../apps/desktop/Cargo.lock`), `utf8`)
    expect(wasmBindgenLockVersion(lock)).toMatch(/^\d+\.\d+\.\d+$/)
  })

  test(`a global CLI of another version is a mismatch`, () => {
    expect(wasmBindgenCliVersion(`wasm-bindgen 0.2.126\n`)).toBe(`0.2.126`)
    expect(wasmBindgenCliVersion(``)).toBeUndefined()
  })

  test(`one install root per version, inside the cargo target`, () => {
    expect(wasmBindgenToolRoot(`/t`, `0.2.130`)).toBe(`/t/shots-tools/wasm-bindgen-0.2.130`)
  })
})

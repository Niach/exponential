// EXP-1603: the hash of the sources the committed Motion WASM is built from
// (eui-motion + eui-motion-wasm sources and manifests). Printed by
// scripts/build-motion-wasm.sh into wasm/motion/SOURCE_HASH; the
// motion-wasm test recomputes it.
import { createHash } from "node:crypto"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"

const repo = join(import.meta.dir, `../../..`)
const roots = [`apps/desktop/crates/eui-motion/src`, `apps/desktop/crates/eui-motion/Cargo.toml`, `apps/desktop/crates/eui-motion-wasm/src`, `apps/desktop/crates/eui-motion-wasm/Cargo.toml`]

function files(path: string): string[] {
  const abs = join(repo, path)
  if (statSync(abs).isFile()) return [path]
  return readdirSync(abs)
    .sort()
    .flatMap((name) => files(join(path, name)))
}

export function motionSourceHash(): string {
  const hash = createHash(`sha256`)
  for (const file of roots.flatMap(files).sort()) {
    hash.update(relative(repo, join(repo, file)))
    hash.update(`\0`)
    hash.update(readFileSync(join(repo, file)))
    hash.update(`\0`)
  }
  return hash.digest(`hex`)
}

if (import.meta.main) console.log(motionSourceHash())

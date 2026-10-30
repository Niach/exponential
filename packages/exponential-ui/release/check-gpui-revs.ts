// EXP-1429: the gpui revision policy, gated. One zed rev and one
// gpui-component rev, written in five places that must agree:
//   apps/desktop/Cargo.toml + Cargo.lock        (the workspace the painter builds in)
//   apps/desktop/crates/app/tests/pinned_revs.rs (the cargo-side lock gate)
//   apps/desktop/crates/exponential-ui-gpui/README.md (the consumer policy table)
//   apps/marketing/ui/guides/gpui/Cargo.toml + Cargo.lock (the third-party guide app)
//   apps/marketing/ui/src/pages/Guide.tsx       (the install snippet on exponential.at/ui)
// plus the rust toolchain of both,
// and both consumers carry the ztracing [patch]. Text only, no cargo: it runs
// in the test.yml gate.
//
//   bun packages/exponential-ui/release/check-gpui-revs.ts

import { readFileSync } from "node:fs"
import { join, resolve } from "node:path"

const REPO = resolve(import.meta.dir, `..`, `..`, `..`)
const ZED = `https://github.com/zed-industries/zed`
const COMPONENT = `https://github.com/longbridge/gpui-component`
const SHA = /^[0-9a-f]{40}$/

const errors: string[] = []
const read = (path: string) => readFileSync(join(REPO, path), `utf8`)
const fail = (path: string, message: string) => errors.push(`${path}: ${message}`)

// The README's policy table is the source of truth: `| zed … | \`<sha>\` |`.
const README = `apps/desktop/crates/exponential-ui-gpui/README.md`
function tableRev(name: string): string {
  const row = read(README)
    .split(`\n`)
    .find((l) => l.startsWith(`| ${name} `))
  const rev = row?.match(/`([0-9a-f]{40})`/)?.[1]
  if (!rev) {
    fail(README, `no "| ${name} … | \`<40-hex rev>\` |" row in the gpui revision table`)
    return ``
  }
  return rev
}
const zedRev = tableRev(`zed`)
const componentRev = tableRev(`gpui-component`)

// Every git source of either upstream in a lock sits on the pin.
function checkLock(path: string) {
  let zed = 0
  let component = 0
  for (const line of read(path).split(`\n`)) {
    const source = line.match(/^source = "git\+([^"]+)"$/)?.[1]
    if (!source) continue
    if (source.startsWith(`${ZED}#`) || source.startsWith(`${ZED}?`)) {
      zed++
      if (source !== `${ZED}#${zedRev}`) fail(path, `zed source off the pin: ${source}`)
    } else if (source.startsWith(`${COMPONENT}?`) || source.startsWith(`${COMPONENT}#`)) {
      component++
      if (source !== `${COMPONENT}?rev=${componentRev}#${componentRev}`) fail(path, `gpui-component source off the pin: ${source}`)
    }
  }
  if (!zed) fail(path, `no zed git source`)
  if (!component) fail(path, `no gpui-component git source`)
}

// Manifests: every gpui-component dep names the rev, zed deps float (the lock
// pins them; a rev there splits gpui into two package ids), and the ztracing
// shims patch zed.
function checkManifest(path: string) {
  const text = read(path)
  const lines = text.split(`\n`).filter((l) => !l.trimStart().startsWith(`#`))
  let component = 0
  for (const line of lines) {
    if (line.includes(`git = "${COMPONENT}"`)) {
      component++
      const rev = line.match(/rev = "([0-9a-f]+)"/)?.[1]
      if (rev !== componentRev) fail(path, `gpui-component dep not at ${componentRev}: ${line.trim()}`)
    }
    if (line.includes(`git = "${ZED}"`) && /\b(rev|tag|branch) = /.test(line)) {
      fail(path, `zed dep pins in the manifest (the Cargo.lock is the pin): ${line.trim()}`)
    }
  }
  if (!component) fail(path, `no gpui-component dependency`)
  const patch = text.split(`[patch."${ZED}"]`)[1]?.split(/\n\[/)[0] ?? ``
  for (const shim of [`ztracing`, `ztracing_macro`]) {
    if (!new RegExp(`^${shim} = `, `m`).test(patch)) fail(path, `no ${shim} entry under [patch."${ZED}"]`)
  }
}

// Any other place that spells a rev out in full.
function checkMentions(path: string, required: { zed: boolean; component: boolean }) {
  const text = read(path)
  for (const [name, rev, needed] of [
    [`zed`, zedRev, required.zed],
    [`gpui-component`, componentRev, required.component],
  ] as const) {
    if (needed && !text.includes(rev)) fail(path, `does not name the ${name} pin ${rev}`)
  }
}

if (SHA.test(zedRev) && SHA.test(componentRev)) {
  checkManifest(`apps/desktop/Cargo.toml`)
  checkLock(`apps/desktop/Cargo.lock`)
  checkManifest(`apps/marketing/ui/guides/gpui/Cargo.toml`)
  checkLock(`apps/marketing/ui/guides/gpui/Cargo.lock`)
  checkMentions(`apps/desktop/crates/app/tests/pinned_revs.rs`, { zed: true, component: true })
  checkMentions(`apps/marketing/ui/src/pages/Guide.tsx`, { zed: true, component: true })
}

// The compiler the pinned gpui needs: `| rust | \`1.96.0\` |`.
const rust = read(README)
  .split(`\n`)
  .find((l) => l.startsWith(`| rust `))
  ?.match(/`([0-9.]+)`/)?.[1]
if (!rust) fail(README, `no "| rust | \`<version>\` |" row in the gpui revision table`)
else
  for (const path of [`apps/desktop/rust-toolchain.toml`, `apps/marketing/ui/guides/gpui/rust-toolchain.toml`]) {
    const channel = read(path).match(/^channel = "([^"]+)"/m)?.[1]
    if (channel !== rust) fail(path, `toolchain ${channel} is not the pinned rust ${rust}`)
  }

if (errors.length) {
  for (const e of errors) console.error(`::error::${e}`)
  console.error(`\ngpui revisions drifted. The policy and the bump recipe: ${README} ("gpui revisions").`)
  process.exit(1)
}
console.log(`gpui pins agree: zed ${zedRev.slice(0, 8)}, gpui-component ${componentRev.slice(0, 8)}, rust ${rust}`)

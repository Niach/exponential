// EXP-1429: the gpui version policy, gated. One gpui-pre snapshot and one
// gpui-component release (both crates.io, exact `=` requirements), written in
// the places that must agree:
//   apps/desktop/Cargo.toml + Cargo.lock        (the workspace the painter builds in)
//   apps/desktop/crates/app/tests/pinned_revs.rs (the cargo-side lock gate)
//   apps/desktop/crates/eui-gpui/README.md (the consumer policy table)
//   packages/exponential-ui/guides/gpui/Cargo.toml + Cargo.lock (the third-party guide app)
//   samples/exponential-ui/gpui/Cargo.toml + Cargo.lock (the sample app)
//   (eDE's shell, apps/desktop/crates/ede-shell, is a member of the desktop workspace since #EDE-51)
//   apps/site/src/content/docs/docs/sdk/genui/guides/gpui.mdx (the install snippet on exponential.at)
// plus the rust toolchain of the workspace and the guide app. No zed or
// gpui-kit git source may come back (a second gpui package id splits every
// gpui type). Text only, no cargo: it runs in the test.yml gate.
//
//   bun packages/exponential-ui/release/check-gpui-revs.ts

import { existsSync, readFileSync } from "node:fs"
import { join, resolve } from "node:path"

const REPO = resolve(import.meta.dir, `..`, `..`, `..`)
const VERSION = /^\d+\.\d+\.\d+$/
const GIT_UPSTREAMS = [`github.com/zed-industries/zed`, `github.com/longbridge/gpui-`]
const KIT_PACKAGES = [`gpui-component`, `gpui-base`, `gpui-kit-assets`, `gpui-component-macros`]
// The snapshot's reqwest fork keeps reqwest's own version line (=0.12.15).
const isPre = (name: string) => (name === `gpui-pre` || name.startsWith(`gpui-pre-`)) && name !== `gpui-pre-reqwest`

const errors: string[] = []
const read = (path: string) => readFileSync(join(REPO, path), `utf8`)
const fail = (path: string, message: string) => errors.push(`${path}: ${message}`)

// The README's policy table is the source of truth: `| gpui-pre | \`0.3.11\` |`.
const README = `apps/desktop/crates/eui-gpui/README.md`
function tableVersion(name: string): string {
  const row = read(README)
    .split(`\n`)
    .find((l) => l.startsWith(`| ${name} `))
  const version = row?.match(/`(\d+\.\d+\.\d+)`/)?.[1]
  if (!version) {
    fail(README, `no "| ${name} … | \`<x.y.z>\` |" row in the gpui revision table`)
    return ``
  }
  return version
}
const gpuiPre = tableVersion(`gpui-pre`)
const component = tableVersion(`gpui-component`)

// Every gpui-pre package in a lock sits on the snapshot, every gpui-kit
// package on the release, and no upstream comes from git.
function checkLock(path: string) {
  let pre = 0
  let kit = 0
  for (const block of read(path).split(`[[package]]`).slice(1)) {
    const field = (key: string) => block.match(new RegExp(`^${key} = "([^"]+)"$`, `m`))?.[1]
    const name = field(`name`)
    const version = field(`version`)
    const source = field(`source`) ?? ``
    if (!name || !version) continue
    if (GIT_UPSTREAMS.some((u) => source.includes(u))) fail(path, `${name} comes from git (${source}); gpui is pinned to crates.io`)
    if (isPre(name)) {
      pre++
      if (version !== gpuiPre) fail(path, `${name} ${version} is off the gpui-pre pin ${gpuiPre}`)
    } else if (KIT_PACKAGES.includes(name)) {
      kit++
      if (version !== component) fail(path, `${name} ${version} is off the gpui-component pin ${component}`)
    }
  }
  if (!pre) fail(path, `no gpui-pre package`)
  if (!kit) fail(path, `no gpui-component package`)
}

// Manifests: every gpui-pre requirement is `=<snapshot>`, every gpui-kit
// requirement `=<release>`, and no zed or gpui-kit git dependency.
function checkManifest(path: string) {
  const lines = read(path)
    .split(`\n`)
    .filter((l) => !l.trimStart().startsWith(`#`))
  let seen = 0
  for (const line of lines) {
    if (GIT_UPSTREAMS.some((u) => line.includes(u))) fail(path, `git dependency on a gpui upstream: ${line.trim()}`)
    const pkg = line.match(/package = "([^"]+)"/)?.[1] ?? line.match(/^([a-z][a-z0-9_-]*)\s*=/)?.[1]
    if (!pkg) continue
    const req = line.match(/version = "([^"]+)"/)?.[1] ?? line.match(/^[a-z][a-z0-9_-]*\s*=\s*"([^"]+)"/)?.[1]
    const want = isPre(pkg) ? gpuiPre : KIT_PACKAGES.includes(pkg) ? component : undefined
    if (!want || !req) continue
    seen++
    if (req !== `=${want}`) fail(path, `${pkg} requires "${req}", not the pin "=${want}"`)
  }
  if (!seen) fail(path, `no gpui-pre or gpui-component dependency`)
}

// Any other place that spells the versions out.
function checkMentions(path: string) {
  const text = read(path)
  if (!text.includes(gpuiPre)) fail(path, `does not name the gpui-pre pin ${gpuiPre}`)
  if (!text.includes(component)) fail(path, `does not name the gpui-component pin ${component}`)
}

if (VERSION.test(gpuiPre) && VERSION.test(component)) {
  checkManifest(`apps/desktop/Cargo.toml`)
  checkLock(`apps/desktop/Cargo.lock`)
  for (const app of [`packages/exponential-ui/guides/gpui`, `samples/exponential-ui/gpui`]) {
    checkManifest(`${app}/Cargo.toml`)
    if (existsSync(join(REPO, app, `Cargo.lock`))) checkLock(`${app}/Cargo.lock`)
  }
  checkMentions(`apps/desktop/crates/app/tests/pinned_revs.rs`)
  checkMentions(`apps/site/src/content/docs/docs/sdk/genui/guides/gpui.mdx`)
}

// The compiler the pinned gpui needs: `| rust | \`1.98.1\` |`.
const rust = read(README)
  .split(`\n`)
  .find((l) => l.startsWith(`| rust `))
  ?.match(/`([0-9.]+)`/)?.[1]
if (!rust) fail(README, `no "| rust | \`<version>\` |" row in the gpui revision table`)
else
  for (const path of [`apps/desktop/rust-toolchain.toml`, `packages/exponential-ui/guides/gpui/rust-toolchain.toml`]) {
    const channel = read(path).match(/^channel = "([^"]+)"/m)?.[1]
    if (channel !== rust) fail(path, `toolchain ${channel} is not the pinned rust ${rust}`)
  }

if (errors.length) {
  for (const e of errors) console.error(`::error::${e}`)
  console.error(`\ngpui versions drifted. The policy and the bump recipe: ${README} ("gpui revisions").`)
  process.exit(1)
}
console.log(`gpui pins agree: gpui-pre ${gpuiPre}, gpui-component ${component}, rust ${rust}`)

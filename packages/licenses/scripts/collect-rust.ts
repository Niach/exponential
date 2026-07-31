#!/usr/bin/env bun
// EXP-375 — collects the desktop app's Rust dependency licences.
//
// Run: `bun run --filter @exp/licenses collect:rust` (needs a cargo toolchain
// and a populated registry; CI runs it in the `desktop` job, which already has
// both). Output: the committed `inventory/rust.json`, consumed offline by
// `scripts/generate.ts`.
//
// THE TRAP THIS SCRIPT EXISTS TO AVOID: `Cargo.lock` is not the shipped graph.
// It carries 916 packages; only ~706 are reachable from the workspace over
// non-dev edges on any of our three target triples. The difference is not
// cosmetic — `libfuzzer-sys` is `(MIT OR Apache-2.0) AND NCSA`, NCSA is not on
// deny.toml's allow-list, and it sits in the lock purely as `rav1e`'s optional
// fuzzing dependency. Attributing it would assert terms over a binary that
// never links it, and would fail the cross-gate against deny.toml.
//
// So: `cargo metadata --filter-platform <triple>` once per triple in
// deny.toml's `[graph].targets`, walk `resolve.nodes` from the workspace
// members following only edges with a non-dev `dep_kind`, and union the three
// results. Build dependencies ARE kept — a proc macro's output is compiled into
// the binary — which is also why `cbindgen` shows up on Windows.

import { execFileSync } from "node:child_process"
import { readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import {
  INVENTORY_COMMENT,
  byCodepoint,
  compareComponents,
  type Component,
  type Inventory,
  type LicenceText,
} from "../src/schema"
import { electFrom } from "../src/spdx"
import {
  extractCopyright,
  normaliseBody,
  readLicenceFiles,
  spdxTemplate,
} from "../src/text"

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), "..")
const repoRoot = join(pkgRoot, "..", "..")
const desktopDir = join(repoRoot, "apps/desktop")
const textsDir = join(pkgRoot, "texts")

// ---------------------------------------------------------------------------
// Targets — read from deny.toml so the two never drift apart
// ---------------------------------------------------------------------------

function denyTargets(): string[] {
  const toml = readFileSync(join(desktopDir, "deny.toml"), `utf8`)
  const block = toml.match(/targets\s*=\s*\[([\s\S]*?)\]/)
  if (!block) throw new Error(`no [graph].targets in apps/desktop/deny.toml`)
  const targets = [...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1])
  if (targets.length === 0) throw new Error(`empty [graph].targets`)
  return targets.sort(byCodepoint)
}

// ---------------------------------------------------------------------------
// The shipped graph
// ---------------------------------------------------------------------------

interface CargoPkg {
  id: string
  name: string
  version: string
  license?: string | null
  license_file?: string | null
  repository?: string | null
  homepage?: string | null
  authors?: string[]
  manifest_path: string
}

interface Shipped {
  /** External packages reachable over non-dev edges. */
  packages: Map<string, CargoPkg>
  /** Of those, the ones reachable ONLY through a build-dependency edge. */
  buildOnly: Set<string>
  /** Reachable only through a dev edge — compiled for tests, never shipped. */
  devOnly: Set<string>
  /** Our own crates. */
  workspace: Set<string>
}

function shippedGraph(triple: string): Shipped {
  const raw = execFileSync(
    `cargo`,
    [
      `metadata`,
      `--manifest-path`,
      join(desktopDir, `Cargo.toml`),
      `--all-features`,
      `--filter-platform`,
      triple,
      `--format-version`,
      `1`,
    ],
    { encoding: `utf8`, maxBuffer: 256 * 1024 * 1024 }
  )
  const meta = JSON.parse(raw) as {
    packages: CargoPkg[]
    workspace_members: string[]
    resolve: {
      nodes: {
        id: string
        deps: { pkg: string; dep_kinds: { kind: string | null }[] }[]
      }[]
    }
  }

  const byId = new Map(meta.packages.map((p) => [p.id, p]))
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]))
  const members = new Set(meta.workspace_members)

  /**
   * @param kinds which `dep_kind`s to traverse. `null` is a normal dependency,
   *   `"build"` a build-dependency, `"dev"` never ships.
   */
  const closure = (kinds: (string | null)[]): Set<string> => {
    const seen = new Set<string>()
    const stack = [...members]
    while (stack.length > 0) {
      const id = stack.pop()!
      if (seen.has(id)) continue
      seen.add(id)
      for (const dep of nodes.get(id)?.deps ?? []) {
        if (!dep.dep_kinds.some((k) => kinds.includes(k.kind))) continue
        stack.push(dep.pkg)
      }
    }
    return seen
  }

  // Everything that ships in any form…
  const all = closure([null, `build`])
  // …versus what is actually LINKED. A build-dependency's own dependencies are
  // build-only too, which is why this is a separate walk rather than a filter.
  const linked = closure([null])
  // …versus everything cargo would compile, tests included. The difference is
  // what the coverage gate is allowed to treat as "deliberately not attributed".
  const withDev = closure([null, `build`, `dev`])

  const packages = new Map<string, CargoPkg>()
  const buildOnly = new Set<string>()
  const devOnly = new Set<string>()
  for (const id of withDev) {
    if (members.has(id)) continue // our own Apache-2.0 crates
    const pkg = byId.get(id)
    if (!pkg) continue
    const key = `${pkg.name}@${pkg.version}`
    if (!all.has(id)) {
      devOnly.add(key)
      continue
    }
    packages.set(key, pkg)
    if (!linked.has(id)) buildOnly.add(key)
  }
  const workspace = new Set(
    [...members].map((id) => {
      const pkg = byId.get(id)!
      return `${pkg.name}@${pkg.version}`
    })
  )
  return { packages, buildOnly, devOnly, workspace }
}

// ---------------------------------------------------------------------------
// Licence bodies
// ---------------------------------------------------------------------------

/**
 * Reproduce one body per elected id. A crate that ships `LICENSE-MIT` and
 * `LICENSE-APACHE` gets the file matching what we elected; a crate that ships a
 * bare `LICENSE` gets that file; a crate that ships nothing gets the canonical
 * SPDX text. `NOTICE` files are picked up too — Apache-2.0 §4(d) makes them
 * mandatory to propagate wherever they exist.
 */
function bodiesFor(
  pkg: CargoPkg,
  licenses: string[]
): { texts: LicenceText[]; derived?: string[] } {
  const dir = dirname(pkg.manifest_path)
  const found = readLicenceFiles(dir)
  const texts: LicenceText[] = []

  // Two crates in the graph (`gpui_util`, `gpui_shared_string`, both from the
  // pinned Zed checkout) declare no `license` field but DO ship a licence file
  // — a `LICENSE-APACHE` symlink to the repository root. Take the id from the
  // file that is actually there. This deliberately cannot reach Zed's
  // `LICENSE-GPL`: that file is at the repository root and is not linked into
  // any crate we compile, so electing it would be inventing an obligation.
  let derived: string[] | undefined
  if (licenses.length === 0) {
    derived = [
      ...new Set(
        found
          .filter((f) => !/^notice/i.test(f.file) && f.spdx)
          .map((f) => f.spdx!)
      ),
    ].sort(byCodepoint)
    licenses = derived
  }

  for (const spdx of licenses) {
    const base = spdx.replace(/ WITH .*$/, ``)
    const exact = found.find((f) => f.spdx === base)
    if (exact) {
      texts.push({ spdx, source: exact.file, body: exact.body })
      continue
    }
    // A single unlabelled licence file covers a single-licence crate.
    const unlabelled = found.filter((f) => !/^notice/i.test(f.file))
    if (licenses.length === 1 && unlabelled.length === 1) {
      texts.push({ spdx, source: unlabelled[0].file, body: unlabelled[0].body })
      continue
    }
    const template = spdxTemplate(textsDir, base)
    if (template) {
      texts.push({ spdx, source: `spdx-template`, body: template })
      continue
    }
    throw new Error(
      `no licence body for ${pkg.name}@${pkg.version} (${spdx}) — ` +
        `add packages/licenses/texts/spdx/${base}.txt via \`fetch:texts\``
    )
  }

  // Apache-2.0 §4(d): a NOTICE file in the source must travel with the work.
  for (const f of found) {
    if (!/^notice/i.test(f.file)) continue
    texts.push({ spdx: `NOTICE`, source: f.file, body: f.body })
  }

  // `license-file = "…"` with no SPDX id at all (2 crates in our graph).
  if (texts.length === 0 && pkg.license_file) {
    try {
      const body = normaliseBody(readFileSync(join(dir, pkg.license_file), `utf8`))
      texts.push({ spdx: `custom`, source: pkg.license_file, body })
    } catch {
      /* fall through to the throw below */
    }
  }
  if (texts.length === 0) {
    throw new Error(
      `${pkg.name}@${pkg.version} declares no licence and ships no licence file`
    )
  }
  return { texts, ...(derived && derived.length > 0 ? { derived } : {}) }
}

// ---------------------------------------------------------------------------
// Collect
// ---------------------------------------------------------------------------

const targets = denyTargets()
const platformsOf = new Map<string, string[]>()
const packages = new Map<string, CargoPkg>()
/** Build-only on EVERY target that has it — linked anywhere means linked. */
const linkedSomewhere = new Set<string>()

const devOnlyAnywhere = new Set<string>()
const workspaceCrates = new Set<string>()

for (const triple of targets) {
  console.log(`cargo metadata --filter-platform ${triple} …`)
  const { packages: found, buildOnly, devOnly, workspace } = shippedGraph(triple)
  for (const [key, pkg] of found) {
    packages.set(key, pkg)
    platformsOf.set(key, [...(platformsOf.get(key) ?? []), triple])
    if (!buildOnly.has(key)) linkedSomewhere.add(key)
  }
  for (const key of devOnly) devOnlyAnywhere.add(key)
  for (const key of workspace) workspaceCrates.add(key)
}

const components: Component[] = []
for (const [key, pkg] of packages) {
  const declared = pkg.license?.trim()
  let licenses: string[] = []
  let election: string | undefined
  if (declared) {
    const resolved = electFrom(declared)
    licenses = resolved.licenses
    election = resolved.election
  }

  const { texts, derived } = bodiesFor(pkg, licenses)
  if (derived) licenses = derived
  const copyright = [
    ...new Set(texts.flatMap((t) => extractCopyright(t.body))),
  ].sort(byCodepoint)

  const platforms = platformsOf.get(key)!.sort(byCodepoint)
  components.push({
    name: pkg.name,
    version: pkg.version,
    declared:
      declared ??
      `(no license field; taken from ${texts.map((t) => t.source).join(`, `)} in the crate source)`,
    licenses,
    ...(election ? { election } : {}),
    ...(pkg.repository ? { homepage: pkg.repository } : {}),
    ...(copyright.length > 0 ? { copyright } : {}),
    texts,
    // Recorded so the notice can scope a crate to the targets that have it,
    // rather than implying every build links it.
    ...(platforms.length < targets.length ? { platforms } : {}),
    ...(linkedSomewhere.has(key) ? {} : { buildOnly: true as const }),
  })
}

components.sort(compareComponents)

const inventory: Inventory = {
  $comment: INVENTORY_COMMENT,
  ecosystem: `rust`,
  scope: `desktop`,
  collector: `packages/licenses/scripts/collect-rust.ts`,
  components,
}

writeFileSync(
  join(pkgRoot, `inventory/rust.json`),
  `${JSON.stringify(inventory, null, 2)}\n`
)

// ---------------------------------------------------------------------------
// The exclusion ledger
// ---------------------------------------------------------------------------
//
// Cargo.lock has ~90 more packages than the shipped graph. The coverage gate in
// apps/web/src/lib/licenses.test.ts has to be able to say "every package in the
// lockfile is either attributed or deliberately excluded, with a reason" — and
// it runs in the pure-bun job with no cargo. So the reason for every dropped
// package is recorded HERE, mechanically, by the one process that actually
// knows it. A new crate that lands in neither file fails the gate.

const lockNames = new Set<string>()
{
  const lock = readFileSync(join(desktopDir, `Cargo.lock`), `utf8`)
  for (const block of lock.split(/\n\[\[package\]\]\n/).slice(1)) {
    const name = block.match(/^name = "([^"]+)"/m)?.[1]
    const version = block.match(/^version = "([^"]+)"/m)?.[1]
    if (name && version) lockNames.add(`${name}@${version}`)
  }
}

const excluded: { package: string; reason: string }[] = []
for (const key of [...lockNames].sort(byCodepoint)) {
  if (packages.has(key)) continue
  if (workspaceCrates.has(key)) {
    excluded.push({ package: key, reason: `workspace member — Exponential's own Apache-2.0 code, covered by the repository LICENSE` })
  } else if (devOnlyAnywhere.has(key)) {
    excluded.push({ package: key, reason: `dev-dependency only — compiled for tests and benchmarks, never linked into a distributed artifact` })
  } else {
    excluded.push({ package: key, reason: `not in the resolved graph for any target in deny.toml — an optional or platform-gated dependency cargo never builds for us` })
  }
}

writeFileSync(
  join(pkgRoot, `inventory/rust-excluded.json`),
  `${JSON.stringify(
    {
      $comment: INVENTORY_COMMENT,
      note: `Packages present in apps/desktop/Cargo.lock that are NOT in the shipped graph, with the reason each was dropped. Cargo.lock is not the shipped graph: libfuzzer-sys, for one, is "(MIT OR Apache-2.0) AND NCSA" and sits in the lock only as rav1e's optional fuzzing dependency.`,
      collector: `packages/licenses/scripts/collect-rust.ts`,
      excluded,
    },
    null,
    2
  )}\n`
)

console.log(
  `\ninventory/rust.json: ${components.length} crates over ${targets.length} targets`
)
console.log(
  `inventory/rust-excluded.json: ${excluded.length} lockfile packages not shipped`
)

#!/usr/bin/env bun
// EXP-375 — the npm ecosystem collector.
//
//   bun run scripts/collect-npm.ts        (from packages/licenses)
//   bun run --filter @exp/licenses collect:npm
//
// Collects BOTH npm scopes in one pass and writes `inventory/npm-web.json`
// and `inventory/npm-marketing.json`. Those files are committed and are the
// only thing `scripts/generate.ts` reads, so this script's output has to be a
// pure function of the repo contents — see "DETERMINISM" below.
//
// WHAT IS COLLECTED
// -----------------
// The PRODUCTION-only closure of one workspace: its `dependencies` (never
// `devDependencies`), walked transitively through each resolved package's own
// `dependencies` plus the `optionalDependencies` that are part of the
// installed graph. Specifiers are resolved the way node does — walk up from
// the importer directory checking `node_modules/<name>`, ending at the repo
// root `node_modules` (bun hoists, so nearly everything lands there).
//
// Our own `@exp/*` workspace packages are SKIPPED as components (they are
// Apache-2.0 code covered by the repo LICENSE) but are still traversed
// THROUGH, so e.g. `@exp/widget`'s production deps land in the web closure.
//
// DETERMINISM (acceptance criteria — a macOS run and an ubuntu run must
// produce byte-identical files, or the drift gate fails forever)
// ---------------------------------------------------------------------------
// ~55 packages per scope are `os`/`cpu`-gated prebuilt binaries — today
// `@esbuild/*`, `@oxc-minify/binding-*`, `@oxc-transform/binding-*`,
// `@rollup/rollup-*`, `@rspack/binding-*`, `@remotion/compositor-*`,
// `@tailwindcss/oxide-*`, `lightningcss-*` and `fsevents`. Only the current
// host's variant is on disk. Reading them from disk would make every run
// host-specific. They are handled by four explicit rules:
//
//   1. DETECTION — a package is "platform-gated" if its manifest (or its
//      bun.lock entry) carries `os`/`cpu`, OR if it is absent from disk but
//      present in `bun.lock`. The second half is what makes the OTHER host's
//      variants appear in the closure at all.
//   2. IDENTITY — name and version for a gated package always come from
//      `bun.lock`, never from disk. bun.lock is committed, so it reads the
//      same on every host.
//   3. LICENCE — a gated package's `declared` id is elected at the level of
//      its FAMILY (all gated packages sharing a name once platform/arch/abi
//      suffixes are stripped, e.g. every `@rollup/rollup-*` at one version),
//      using whichever family members happen to be installed on this host as
//      evidence. macOS elects from the darwin variant, ubuntu from the linux
//      variant, and they agree because variants of one family are generated
//      from a single template. Disagreement is reported loudly. A family that
//      is host-EXCLUSIVE (no member one of the hosts can install) has no such
//      evidence, so it must appear in PLATFORM_LICENCE_FALLBACK below — a
//      reviewed, committed table. The summary FAILS LOUDLY when a gated family
//      has neither cross-host coverage nor a fallback entry, which is the only
//      way this scheme can silently start drifting.
//      The licence BODY of a gated package is ALWAYS the canonical
//      `texts/spdx/<id>.txt` template, never a file inside the package: a
//      per-package file exists only on the host that installs that variant,
//      so reading one would be the drift we are avoiding. In practice these
//      packages ship no licence file anyway (the one exception, `fsevents`,
//      is reported so its copyright line can be curated).
//   4. HOMEPAGE — omitted for gated packages. Variant manifests routinely
//      point `repository.url` at their OWN subdirectory (Remotion's
//      `.../packages/compositor-darwin-arm64`), so a family-elected homepage
//      would be the host's variant URL stamped onto all seven. The family's
//      parent package (`esbuild`, `@remotion/renderer`, …) is itself a
//      component and carries the project URL.
//
// Nothing else host-shaped reaches the output: no timestamps, no absolute
// paths, no hostnames, no `process.platform` branch outside the family
// election above. All sorting uses `byCodepoint` (never `localeCompare`,
// whose result depends on the host's ICU build).

import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import {
  INVENTORY_COMMENT,
  byCodepoint,
  compareComponents,
  type Component,
  type Inventory,
  type LicenceText,
} from "../src/schema"

const PKG_ROOT = resolve(import.meta.dirname, `..`)
const REPO_ROOT = resolve(PKG_ROOT, `..`, `..`)
const TEMPLATE_DIR = join(PKG_ROOT, `texts`, `spdx`)
const INVENTORY_DIR = join(PKG_ROOT, `inventory`)
const COLLECTOR = `packages/licenses/scripts/collect-npm.ts`

const SCOPES = [
  { scope: `web` as const, dir: `apps/web` },
  { scope: `marketing` as const, dir: `apps/marketing` },
]

// ---------------------------------------------------------------------------
// Reviewed fallbacks for platform-gated families with no member installed on
// any host we collect from — either because the family has a single
// host-exclusive member (`fsevents` is darwin-only) or because the whole
// subtree hangs off a wasm fallback variant that no native host installs
// (`@tailwindcss/oxide-wasm32-wasi`'s runtime). Keyed by family base (see
// `familyBase`). Every entry is a recorded human decision, verified against
// the package's registry metadata; the collector never guesses.
// ---------------------------------------------------------------------------
const PLATFORM_LICENCE_FALLBACK: Record<string, string> = {
  // registry.npmjs.org/fsevents → MIT (darwin-only, never installed on Linux)
  [`fsevents`]: `MIT`,
  // `@electric-sql/client` optionally depends on `@rollup/rollup-darwin-arm64`
  // ALONE, so that family has no Linux member to elect from.
  // registry.npmjs.org/@rollup/rollup-linux-x64-gnu@4.60.3 → MIT, matching the
  // darwin variant on disk.
  [`@rollup/rollup`]: `MIT`,
  // emnapi / napi-rs wasm runtime — reachable only through
  // `@tailwindcss/oxide-wasm32-wasi`, which no native host installs.
  // registry.npmjs.org → MIT for all five.
  [`@emnapi/core`]: `MIT`,
  [`@emnapi/runtime`]: `MIT`,
  [`@emnapi/wasi-threads`]: `MIT`,
  [`@napi-rs/wasm-runtime`]: `MIT`,
  [`@tybys/wasm-util`]: `MIT`,
}

// Platform / architecture / ABI tokens stripped to find a family base.
const PLATFORM_TOKENS = new Set([
  `aix`,
  `android`,
  `arm`,
  `arm64`,
  `armv7`,
  `browser`,
  `darwin`,
  `eabi`,
  `freebsd`,
  `gnu`,
  `gnueabihf`,
  `ia32`,
  `linux`,
  `loong64`,
  `mips64el`,
  `msvc`,
  `musl`,
  `musleabihf`,
  `netbsd`,
  `openbsd`,
  `openharmony`,
  `ppc64`,
  `riscv64`,
  `s390x`,
  `sunos`,
  `universal`,
  `wasi`,
  `wasm`,
  `wasm32`,
  `win`,
  `win32`,
  `x64`,
  `x86`,
])

// Election preference, first match wins (spec'd by EXP-375).
const ELECTION_PREFERENCE = [
  `MIT`,
  `ISC`,
  `BSD-2-Clause`,
  `BSD-3-Clause`,
  `0BSD`,
  `Apache-2.0`,
  `MPL-2.0`,
]

// ---------------------------------------------------------------------------
// Diagnostics — everything the human running this needs to act on.
// ---------------------------------------------------------------------------
const missingTemplates = new Map<string, Set<string>>()
const seeLicenseIn: string[] = []
const unlicensed: string[] = []
const nonSpdx: string[] = []
const undetermined: string[] = []
const noLicenceAtAll: string[] = []
const gatedUndetermined: string[] = []
const hostExclusiveFamilies: string[] = []
const familyDisagreements: string[] = []
const gatedWithOnDiskFile: string[] = []
let gatedCount = 0

const warn = (line: string) => process.stderr.write(`${line}\n`)

// ---------------------------------------------------------------------------
// bun.lock (JSONC — trailing commas, no comments)
// ---------------------------------------------------------------------------

const parseJsonc = (text: string): unknown => {
  let out = ``
  let inString = false
  let escaped = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!
    if (inString) {
      out += c
      if (escaped) escaped = false
      else if (c === `\\`) escaped = true
      else if (c === `"`) inString = false
      continue
    }
    if (c === `"`) {
      inString = true
      out += c
      continue
    }
    if (c === `,`) {
      let j = i + 1
      while (j < text.length && /\s/.test(text[j]!)) j++
      if (text[j] === `}` || text[j] === `]`) continue
    }
    out += c
  }
  return JSON.parse(out)
}

interface LockEntry {
  key: string
  name: string
  version: string
  isWorkspace: boolean
  dependencies: Record<string, string>
  optionalDependencies: Record<string, string>
  os?: unknown
  cpu?: unknown
}

const lockRaw = parseJsonc(
  readFileSync(join(REPO_ROOT, `bun.lock`), `utf8`)
) as {
  workspaces: Record<string, { name?: string }>
  packages: Record<string, unknown[]>
}

const parseLockEntry = (key: string, value: unknown[]): LockEntry => {
  const ident = String(value[0] ?? key)
  const at = ident.lastIndexOf(`@`)
  const name = at > 0 ? ident.slice(0, at) : ident
  const version = at > 0 ? ident.slice(at + 1) : ``
  const meta = (
    typeof value[2] === `object` && value[2] !== null ? value[2] : {}
  ) as Record<string, unknown>
  return {
    key,
    name,
    version,
    isWorkspace: version.startsWith(`workspace:`),
    dependencies: (meta.dependencies as Record<string, string>) ?? {},
    optionalDependencies:
      (meta.optionalDependencies as Record<string, string>) ?? {},
    os: meta.os,
    cpu: meta.cpu,
  }
}

const LOCK = new Map<string, LockEntry>()
for (const [key, value] of Object.entries(lockRaw.packages)) {
  LOCK.set(key, parseLockEntry(key, value))
}

/** Split a bun.lock key into segments, honouring `@scope/name` segments. */
const splitLockKey = (key: string): string[] => {
  if (key === ``) return []
  const parts = key.split(`/`)
  const out: string[] = []
  for (let i = 0; i < parts.length; i++) {
    if (parts[i]!.startsWith(`@`) && i + 1 < parts.length)
      out.push(`${parts[i]}/${parts[++i]}`)
    else out.push(parts[i]!)
  }
  return out
}

/** Node-style lookup inside bun.lock's flattened key space. */
const lockLookup = (
  importerKey: string | null,
  name: string
): LockEntry | undefined => {
  const chain = splitLockKey(importerKey ?? ``)
  for (let i = chain.length; i >= 0; i--) {
    const candidate = [...chain.slice(0, i), name].join(`/`)
    const hit = LOCK.get(candidate)
    if (hit) return hit
  }
  return undefined
}

// ---------------------------------------------------------------------------
// Disk resolution
// ---------------------------------------------------------------------------

interface DiskPkg {
  dir: string
  manifest: Record<string, unknown>
  workspaceInternal: boolean
}

const manifestCache = new Map<string, Record<string, unknown> | null>()

const readManifest = (dir: string): Record<string, unknown> | null => {
  const cached = manifestCache.get(dir)
  if (cached !== undefined) return cached
  const file = join(dir, `package.json`)
  let parsed: Record<string, unknown> | null = null
  try {
    parsed = JSON.parse(readFileSync(file, `utf8`).replace(/^﻿/, ``)) as Record<
      string,
      unknown
    >
  } catch {
    parsed = null
  }
  manifestCache.set(dir, parsed)
  return parsed
}

/** Walk up `node_modules` from `fromDir`, node-resolution style. */
const resolveFromDisk = (fromDir: string, name: string): DiskPkg | null => {
  let dir = fromDir
  for (;;) {
    const candidate = join(dir, `node_modules`, name)
    const manifest = existsSync(join(candidate, `package.json`))
      ? readManifest(candidate)
      : null
    if (manifest) {
      let real = candidate
      try {
        real = realpathSync(candidate)
      } catch {
        /* keep the symlink path */
      }
      const inRepo = real === REPO_ROOT || real.startsWith(`${REPO_ROOT}/`)
      return {
        dir: real,
        manifest,
        workspaceInternal: inRepo && !real.includes(`/node_modules/`),
      }
    }
    if (dir === REPO_ROOT) return null
    const parent = dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

// ---------------------------------------------------------------------------
// SPDX expressions
// ---------------------------------------------------------------------------

type Expr =
  | { kind: `id`; id: string }
  | { kind: `and`; nodes: Expr[] }
  | { kind: `or`; nodes: Expr[] }

/** SPDX ids are `[A-Za-z0-9.+-]`; anything else is free text, not an id. */
const SPDX_ID_RE = /^[A-Za-z0-9][A-Za-z0-9.+-]*$/

/**
 * Tolerant SPDX parser. Handles the sloppy legacy spellings npm packages ship:
 * `MIT/Apache-2.0` (slash = OR), `(MIT OR CC0-1.0)`, lower-case operators,
 * `X WITH Y` exceptions, and trailing `+`.
 *
 * Returns `null` for anything that is NOT an SPDX expression — free-text
 * declarations like Remotion's `Remotion License https://remotion.dev/license`
 * must not be silently truncated to a fake id.
 */
const parseSpdx = (input: string): Expr | null => {
  const tokens = input
    .replace(/\//g, ` OR `)
    .replace(/([()])/g, ` $1 `)
    .split(/\s+/)
    .filter(Boolean)
  if (tokens.length === 0) return null

  let pos = 0
  const peek = () => tokens[pos]
  const upper = () => (tokens[pos] ?? ``).toUpperCase()

  const parsePrimary = (): Expr | null => {
    if (peek() === `(`) {
      pos++
      const inner = parseOr()
      if (peek() === `)`) pos++
      return inner
    }
    const token = peek()
    if (token === undefined || token === `)` || !SPDX_ID_RE.test(token))
      return null
    pos++
    let id = token
    // `Apache-2.0 WITH LLVM-exception` is one licence id, not two conjuncts.
    while (
      upper() === `WITH` &&
      tokens[pos + 1] !== undefined &&
      SPDX_ID_RE.test(tokens[pos + 1]!)
    ) {
      pos++
      id = `${id} WITH ${tokens[pos]}`
      pos++
    }
    return { kind: `id`, id }
  }

  const parseAnd = (): Expr | null => {
    const nodes: Expr[] = []
    const first = parsePrimary()
    if (!first) return null
    nodes.push(first)
    while (upper() === `AND`) {
      pos++
      const next = parsePrimary()
      if (!next) break
      nodes.push(next)
    }
    return nodes.length === 1 ? nodes[0]! : { kind: `and`, nodes }
  }

  const parseOr = (): Expr | null => {
    const nodes: Expr[] = []
    const first = parseAnd()
    if (!first) return null
    nodes.push(first)
    while (upper() === `OR`) {
      pos++
      const next = parseAnd()
      if (!next) break
      nodes.push(next)
    }
    return nodes.length === 1 ? nodes[0]! : { kind: `or`, nodes }
  }

  const expr = parseOr()
  // Leftover tokens ⇒ this was never an SPDX expression.
  return expr !== null && pos === tokens.length ? expr : null
}

const hasOr = (expr: Expr): boolean =>
  expr.kind === `or` || (expr.kind === `and` && expr.nodes.some(hasOr))

const preferenceRank = (id: string): number => {
  const index = ELECTION_PREFERENCE.indexOf(id)
  return index === -1 ? ELECTION_PREFERENCE.length : index
}

/** Resolve an expression to the ids that actually bind us. */
const electIds = (expr: Expr): string[] => {
  if (expr.kind === `id`) return [expr.id]
  if (expr.kind === `and`) {
    const set = new Set<string>()
    for (const node of expr.nodes) for (const id of electIds(node)) set.add(id)
    return [...set].sort(byCodepoint)
  }
  // OR — elect exactly one branch.
  const branches = expr.nodes.map((node) => electIds(node))
  let best = branches[0]!
  for (const branch of branches.slice(1)) {
    const bestRank = Math.min(...best.map(preferenceRank))
    const rank = Math.min(...branch.map(preferenceRank))
    if (rank < bestRank) best = branch
    else if (
      rank === bestRank &&
      byCodepoint(branch.join(` AND `), best.join(` AND `)) < 0
    )
      best = branch
  }
  return [...new Set(best)].sort(byCodepoint)
}

interface Election {
  /** How the package expressed itself — drives reporting, not the output. */
  kind: `spdx` | `see-file` | `unlicensed` | `non-spdx` | `none`
  declared: string
  licenses: string[]
  election?: string
  /** `SEE LICENSE IN <file>` target, when that is what the package declares. */
  seeFile?: string
}

/** Read the declared licence off a manifest, in every spelling npm allows. */
const declaredFrom = (manifest: Record<string, unknown>): string => {
  const license = manifest.license
  if (typeof license === `string`) return license.trim()
  if (license && typeof license === `object`) {
    const type = (license as { type?: unknown }).type
    if (typeof type === `string`) return type.trim()
  }
  const legacy = manifest.licenses
  if (Array.isArray(legacy)) {
    const types = legacy
      .map((entry) =>
        typeof entry === `string`
          ? entry
          : String((entry as { type?: unknown })?.type ?? ``)
      )
      .map((value) => value.trim())
      .filter(Boolean)
    // npm's legacy array means "dual licensed, pick one".
    if (types.length > 0) return types.join(` OR `)
  }
  if (typeof legacy === `string`) return legacy.trim()
  return ``
}

const elect = (declared: string): Election => {
  // No `license` field at all. `UNKNOWN` is the explicit marker the merge step
  // and `curated/` key off; the empty string would read as "none needed".
  if (declared === ``)
    return { kind: `none`, declared: `UNKNOWN`, licenses: [] }
  const seeMatch = /^SEE\s+LICEN[CS]E\s+IN\s+(.+)$/i.exec(declared)
  if (seeMatch)
    return {
      kind: `see-file`,
      declared,
      licenses: [],
      seeFile: seeMatch[1]!.trim(),
    }
  if (/^UNLICENSED$/i.test(declared))
    return { kind: `unlicensed`, declared, licenses: [] }

  const expr = parseSpdx(declared)
  if (!expr) return { kind: `non-spdx`, declared, licenses: [] }
  const licenses = electIds(expr)
  const result: Election = { kind: `spdx`, declared, licenses }
  if (hasOr(expr))
    result.election = `${licenses.join(` AND `)} elected from \`${declared}\``
  return result
}

// ---------------------------------------------------------------------------
// Licence bodies
// ---------------------------------------------------------------------------

const normaliseBody = (raw: string): string =>
  raw
    .replace(/^﻿/, ``)
    .replace(/\r\n?/g, `\n`)
    .split(`\n`)
    .map((line) => line.replace(/[ \t]+$/, ``))
    .join(`\n`)
    .replace(/\n+$/, ``)

const BODY_EXT = /\.(md|markdown|txt|text|rst|html)$/

interface LicenceFile {
  /** Filename as it appears in the package, used verbatim as `source`. */
  name: string
  base: `licence` | `copying` | `notice`
  /** e.g. `mit`, `apache-2.0`, or `` for a plain LICENSE. */
  qualifier: string
}

const classifyLicenceFile = (fileName: string): LicenceFile | null => {
  const stem = fileName.toLowerCase().replace(BODY_EXT, ``)
  const match = /^(licen[sc]e|copying|notice)(?:[-_.](.+))?$/.exec(stem)
  if (!match) return null
  const base =
    match[1] === `copying`
      ? (`copying` as const)
      : match[1] === `notice`
        ? (`notice` as const)
        : (`licence` as const)
  return { name: fileName, base, qualifier: match[2] ?? `` }
}

const licenceFilesIn = (dir: string): LicenceFile[] => {
  let entries: string[]
  try {
    entries = readdirSync(dir)
  } catch {
    return []
  }
  const out: LicenceFile[] = []
  for (const entry of entries.sort(byCodepoint)) {
    const classified = classifyLicenceFile(entry)
    if (!classified) continue
    try {
      if (!statSync(join(dir, entry)).isFile()) continue
    } catch {
      continue
    }
    out.push(classified)
  }
  return out
}

const normaliseId = (id: string): string =>
  id.toLowerCase().replace(/[^a-z0-9]/g, ``)

const qualifierMatchesId = (qualifier: string, id: string): boolean => {
  if (qualifier === ``) return false
  const q = normaliseId(qualifier)
  const i = normaliseId(id)
  if (q === ``) return false
  return q === i || i.startsWith(q) || q.startsWith(i)
}

const templateCache = new Map<string, string | null>()

const templateFor = (id: string): string | null => {
  const cached = templateCache.get(id)
  if (cached !== undefined) return cached
  const file = join(TEMPLATE_DIR, `${id}.txt`)
  let body: string | null = null
  try {
    body = normaliseBody(readFileSync(file, `utf8`))
  } catch {
    body = null
  }
  templateCache.set(id, body)
  return body
}

const noteMissingTemplate = (id: string, pkg: string) => {
  warn(`MISSING TEMPLATE: ${id} (needed by ${pkg})`)
  const set = missingTemplates.get(id) ?? new Set<string>()
  set.add(pkg)
  missingTemplates.set(id, set)
}

const readBody = (dir: string, fileName: string): string | null => {
  try {
    return normaliseBody(readFileSync(join(dir, fileName), `utf8`))
  } catch {
    return null
  }
}

const COPYRIGHT_RE = /^\s*(?:\(c\)\s*)?copyright\b|^\s*copyright\s*(?:\(c\)|©)/i

/** Copyright lines carried by a real licence body (never a template). */
const copyrightLines = (body: string): string[] => {
  const out: string[] = []
  for (const rawLine of body.split(`\n`)) {
    const line = rawLine
      .trim()
      .replace(/^[*#>\-\s]+/, ``)
      .trim()
    if (!COPYRIGHT_RE.test(line)) continue
    if (
      /<year>|<owner>|\[year\]|\[fullname\]|\[name of copyright owner\]/i.test(
        line
      )
    )
      continue
    if (!out.includes(line)) out.push(line)
  }
  return out
}

// ---------------------------------------------------------------------------
// Homepage
// ---------------------------------------------------------------------------

const homepageFrom = (
  manifest: Record<string, unknown>
): string | undefined => {
  const homepage = manifest.homepage
  if (typeof homepage === `string` && homepage.trim() !== ``)
    return homepage.trim()
  const repository = manifest.repository
  const url =
    typeof repository === `string`
      ? repository
      : typeof (repository as { url?: unknown })?.url === `string`
        ? (repository as { url: string }).url
        : ``
  if (!url) return undefined
  let normalised = url.trim()
  if (/^(github|gitlab|bitbucket):/.test(normalised)) {
    const [host, path] = normalised.split(`:`)
    const domain =
      host === `github`
        ? `github.com`
        : host === `gitlab`
          ? `gitlab.com`
          : `bitbucket.org`
    normalised = `https://${domain}/${path}`
  } else if (/^[\w.-]+\/[\w.-]+$/.test(normalised)) {
    normalised = `https://github.com/${normalised}`
  }
  normalised = normalised
    .replace(/^git\+/, ``)
    .replace(/^git:\/\//, `https://`)
    .replace(/^ssh:\/\/git@/, `https://`)
    .replace(/^git@([^:]+):/, `https://$1/`)
    .replace(/\.git$/, ``)
  return normalised === `` ? undefined : normalised
}

// ---------------------------------------------------------------------------
// Platform families
// ---------------------------------------------------------------------------

const familyBase = (name: string): string => {
  const slash = name.lastIndexOf(`/`)
  const scope =
    name.startsWith(`@`) && slash > 0 ? name.slice(0, slash + 1) : ``
  const bare = scope === `` ? name : name.slice(slash + 1)
  const parts = bare.split(`-`)
  while (
    parts.length > 0 &&
    PLATFORM_TOKENS.has(parts[parts.length - 1]!.toLowerCase())
  )
    parts.pop()
  return `${scope}${parts.join(`-`)}`
}

/**
 * Declarations gathered from gated packages that ARE installed on this host.
 * Keyed both by `base@version` and by bare `base`, so a family whose exact
 * version has no installed member can still inherit from a sibling version.
 */
const familyEvidence = new Map<string, Set<string>>()

const recordFamilyEvidence = (
  name: string,
  version: string,
  manifest: Record<string, unknown>
) => {
  const declared = declaredFrom(manifest)
  if (declared === ``) return
  for (const key of [`${familyBase(name)}@${version}`, familyBase(name)]) {
    const evidence = familyEvidence.get(key) ?? new Set<string>()
    evidence.add(declared)
    familyEvidence.set(key, evidence)
  }
}

const electFamilyDeclaration = (name: string, version: string): string => {
  for (const key of [`${familyBase(name)}@${version}`, familyBase(name)]) {
    const values = [...(familyEvidence.get(key) ?? [])].sort(byCodepoint)
    if (values.length === 0) continue
    // Variants of one family are generated from a single template, so this
    // should always be a one-element set. If it is not, the elected value is
    // still deterministic (codepoint-first) but a human must look.
    if (values.length > 1)
      familyDisagreements.push(
        `${key}: ${values.join(` | `)} → using ${values[0]}`
      )
    return values[0]!
  }
  return ``
}

// ---------------------------------------------------------------------------
// Closure traversal
// ---------------------------------------------------------------------------

interface RawPkg {
  name: string
  version: string
  /** `null` for platform-gated packages — their bodies come from templates. */
  dir: string | null
  manifest: Record<string, unknown> | null
  gated: boolean
  /** Diagnostics only: where a gated package happens to live on THIS host. */
  gatedHostDir: string | null
}

const collectClosure = (workspaceDir: string): Map<string, RawPkg> => {
  const rootDir = join(REPO_ROOT, workspaceDir)
  const rootManifest = readManifest(rootDir)
  if (!rootManifest) throw new Error(`no package.json at ${workspaceDir}`)

  const found = new Map<string, RawPkg>()
  const visited = new Set<string>()

  interface Task {
    dir: string | null
    manifest: Record<string, unknown> | null
    lockKey: string | null
    deps: string[]
    optionalDeps: string[]
  }

  const depNames = (
    manifest: Record<string, unknown>,
    field: string
  ): string[] =>
    Object.keys((manifest[field] as Record<string, string>) ?? {}).sort(
      byCodepoint
    )

  const queue: Task[] = [
    {
      dir: rootDir,
      manifest: rootManifest,
      lockKey: String(rootManifest.name ?? ``),
      deps: depNames(rootManifest, `dependencies`),
      optionalDeps: depNames(rootManifest, `optionalDependencies`),
    },
  ]

  while (queue.length > 0) {
    const task = queue.shift()!
    for (const name of [...task.deps, ...task.optionalDeps]) {
      const lock = lockLookup(task.lockKey, name)
      // `task.dir === null` means the importer is a platform-gated package, so
      // it has no host-independent directory to walk up from. Resolve those
      // children against the hoisted root instead, and only accept the hit if
      // its version matches the lockfile — otherwise we would be attributing a
      // different copy. Never resolving gated children relative to a host-
      // specific directory is what keeps the traversal identical everywhere.
      const fromRoot = task.dir === null
      const hit = resolveFromDisk(task.dir ?? REPO_ROOT, name)
      const disk =
        hit !== null &&
        (!fromRoot ||
          lock === undefined ||
          String(hit.manifest.version ?? ``) === lock.version)
          ? hit
          : null

      // Our own workspace packages: traverse through, never emit.
      if (disk?.workspaceInternal || lock?.isWorkspace) {
        const dir = disk?.workspaceInternal ? disk.dir : null
        const manifest = dir === null ? null : readManifest(dir)
        const id = `workspace:${lock?.name ?? name}`
        if (visited.has(id) || !manifest || dir === null) continue
        visited.add(id)
        queue.push({
          dir,
          manifest,
          lockKey: String(manifest.name ?? name),
          deps: depNames(manifest, `dependencies`),
          optionalDeps: depNames(manifest, `optionalDependencies`),
        })
        continue
      }

      const onDisk = disk !== null
      const diskGated =
        onDisk &&
        (disk!.manifest.os !== undefined || disk!.manifest.cpu !== undefined)
      const lockGated =
        lock !== undefined && (lock.os !== undefined || lock.cpu !== undefined)
      // A package absent from disk but present in bun.lock is the OTHER host's
      // variant — that is exactly the gated case we must materialise.
      const gated = diskGated || lockGated || (!onDisk && lock !== undefined)

      if (!onDisk && lock === undefined) continue // not part of the graph anywhere

      // Identity: gated packages are pinned to bun.lock, never to disk.
      const pkgName = gated
        ? (lock?.name ?? name)
        : String(disk!.manifest.name ?? name)
      const pkgVersion = gated
        ? (lock?.version ?? String(disk?.manifest.version ?? ``))
        : String(disk!.manifest.version ?? ``)
      const id = `${pkgName}@${pkgVersion}`

      if (gated && onDisk)
        recordFamilyEvidence(pkgName, pkgVersion, disk!.manifest)

      if (!found.has(id)) {
        found.set(id, {
          name: pkgName,
          version: pkgVersion,
          dir: gated ? null : disk!.dir,
          manifest: gated ? null : disk!.manifest,
          gated,
          gatedHostDir: gated && onDisk ? disk!.dir : null,
        })
      }

      if (visited.has(id)) continue
      visited.add(id)

      if (gated) {
        // Deps of a gated package come from bun.lock so that e.g.
        // `@img/sharp-linux-x64 → @img/sharp-libvips-linux-x64` is discovered
        // on a host where neither is installed. `dir: null` deliberately drops
        // the host's copy of the directory even when it exists — see above.
        queue.push({
          dir: null,
          manifest: null,
          lockKey: lock?.key ?? null,
          deps: Object.keys(lock?.dependencies ?? {}).sort(byCodepoint),
          optionalDeps: Object.keys(lock?.optionalDependencies ?? {}).sort(
            byCodepoint
          ),
        })
      } else {
        queue.push({
          dir: disk!.dir,
          manifest: disk!.manifest,
          lockKey: lock?.key ?? null,
          deps: depNames(disk!.manifest, `dependencies`),
          optionalDeps: depNames(disk!.manifest, `optionalDependencies`),
        })
      }
    }
  }

  return found
}

// ---------------------------------------------------------------------------
// Component assembly
// ---------------------------------------------------------------------------

const buildComponent = (raw: RawPkg): Component => {
  const label = `${raw.name}@${raw.version}`

  const declaredRaw = raw.gated
    ? electFamilyDeclaration(raw.name, raw.version) ||
      PLATFORM_LICENCE_FALLBACK[familyBase(raw.name)] ||
      PLATFORM_LICENCE_FALLBACK[raw.name] ||
      ``
    : declaredFrom(raw.manifest!)

  const election = elect(declaredRaw)
  const files = raw.dir === null ? [] : licenceFilesIn(raw.dir)
  const texts: LicenceText[] = []
  const copyright: string[] = []

  if (raw.gated) {
    gatedCount++
    // Bodies always come from the canonical template — a file inside a gated
    // package only exists on the host that installs that variant.
    if (
      raw.gatedHostDir !== null &&
      licenceFilesIn(raw.gatedHostDir).length > 0
    )
      gatedWithOnDiskFile.push(label)
    if (election.licenses.length === 0)
      gatedUndetermined.push(`${label} — no family evidence`)
    for (const id of election.licenses) {
      const body = templateFor(id)
      if (body === null) {
        noteMissingTemplate(id, label)
        continue
      }
      texts.push({ spdx: id, source: `spdx-template`, body })
    }
  } else if (election.seeFile !== undefined) {
    const body = raw.dir === null ? null : readBody(raw.dir, election.seeFile)
    seeLicenseIn.push(
      `${label} — ${election.declared}${body === null ? ` (FILE MISSING from the package)` : ``}`
    )
    if (body !== null && body !== ``) {
      texts.push({ spdx: election.declared, source: election.seeFile, body })
      for (const line of copyrightLines(body))
        if (!copyright.includes(line)) copyright.push(line)
    }
  } else if (election.licenses.length === 0) {
    if (election.kind === `unlicensed`) unlicensed.push(label)
    else if (election.kind === `non-spdx`)
      nonSpdx.push(`${label} — declared: ${election.declared}`)
    else undetermined.push(`${label} — no \`license\` field`)
    // Still reproduce whatever the package ships, so the notice is complete.
    const generic =
      files.find((f) => f.base === `licence` && f.qualifier === ``) ?? files[0]
    const body = generic === undefined ? null : readBody(raw.dir!, generic.name)
    if (body !== null && body !== ``) {
      texts.push({ spdx: election.declared, source: generic!.name, body })
      for (const line of copyrightLines(body))
        if (!copyright.includes(line)) copyright.push(line)
    } else if (election.kind !== `unlicensed`) {
      noLicenceAtAll.push(label)
    }
  } else {
    const used = new Set<string>()
    for (const id of election.licenses) {
      // Prefer a file whose name names this SPDX id, then a plain LICENSE /
      // COPYING (only meaningful when a single id binds us), then NOTICE.
      const specific = files.find(
        (f) => f.base !== `notice` && qualifierMatchesId(f.qualifier, id)
      )
      const generic =
        election.licenses.length === 1
          ? (files.find((f) => f.base === `licence` && f.qualifier === ``) ??
            files.find((f) => f.base === `copying` && f.qualifier === ``) ??
            files.find((f) => f.base === `notice`))
          : undefined
      const chosen = specific ?? generic
      const body =
        chosen === undefined || used.has(chosen.name)
          ? null
          : readBody(raw.dir!, chosen.name)
      if (body !== null && body !== ``) {
        used.add(chosen!.name)
        texts.push({ spdx: id, source: chosen!.name, body })
        for (const line of copyrightLines(body))
          if (!copyright.includes(line)) copyright.push(line)
        continue
      }
      const template = templateFor(id)
      if (template === null) {
        noteMissingTemplate(id, label)
        continue
      }
      texts.push({ spdx: id, source: `spdx-template`, body: template })
    }
  }

  // Gated packages get no homepage — see rule 4 in the header comment.
  const homepage = raw.gated ? undefined : homepageFrom(raw.manifest!)

  // Key order here IS the key order in the committed JSON — keep it stable.
  return {
    name: raw.name,
    version: raw.version,
    declared: election.declared,
    licenses: election.licenses,
    ...(election.election !== undefined ? { election: election.election } : {}),
    ...(homepage !== undefined ? { homepage } : {}),
    ...(copyright.length > 0 ? { copyright: copyright.sort(byCodepoint) } : {}),
    texts,
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const closures = SCOPES.map(({ scope, dir }) => ({
  scope,
  dir,
  raw: collectClosure(dir),
}))

mkdirSync(INVENTORY_DIR, { recursive: true })

const summary: string[] = []
for (const { scope, raw } of closures) {
  const components = [...raw.values()]
    .map(buildComponent)
    .sort(compareComponents)
  const inventory: Inventory = {
    $comment: INVENTORY_COMMENT,
    ecosystem: `npm`,
    scope,
    collector: COLLECTOR,
    components,
  }
  const file = join(INVENTORY_DIR, `npm-${scope}.json`)
  writeFileSync(file, `${JSON.stringify(inventory, null, 2)}\n`)
  const gated = components.filter(
    (c) => raw.get(`${c.name}@${c.version}`)?.gated
  ).length
  summary.push(
    `${scope}: ${components.length} components (${gated} platform-gated)`
  )
}

// Cross-host coverage guard. Family election only agrees between macOS and
// Linux when the family actually HAS a member each host installs. A family
// that is host-exclusive (`fsevents`; `@electric-sql/client`'s lone
// `@rollup/rollup-darwin-arm64`) MUST carry a PLATFORM_LICENCE_FALLBACK entry,
// or the two runners will disagree and the drift gate will never go green.
{
  const families = new Map<string, Set<string>>()
  for (const { raw } of closures)
    for (const pkg of raw.values()) {
      if (!pkg.gated) continue
      const key = familyBase(pkg.name)
      const members = families.get(key) ?? new Set<string>()
      members.add(pkg.name)
      families.set(key, members)
    }
  for (const [key, members] of [...families.entries()].sort((a, b) =>
    byCodepoint(a[0], b[0])
  )) {
    const names = [...members]
    const covered =
      names.some((n) => n.includes(`darwin`)) &&
      names.some((n) => n.includes(`linux`))
    const hasFallback =
      PLATFORM_LICENCE_FALLBACK[key] !== undefined ||
      names.every((n) => PLATFORM_LICENCE_FALLBACK[n] !== undefined)
    if (!covered && !hasFallback)
      hostExclusiveFamilies.push(
        `${key} — members: ${names.sort(byCodepoint).join(`, `)}`
      )
  }
}

warn(``)
warn(`--- collect-npm summary ---`)
for (const line of summary) warn(line)
warn(`platform-gated component instances: ${gatedCount}`)
if (missingTemplates.size > 0) {
  warn(`missing SPDX templates (${missingTemplates.size}):`)
  for (const id of [...missingTemplates.keys()].sort(byCodepoint))
    warn(`  ${id} — ${missingTemplates.get(id)!.size} package(s)`)
}
for (const [label, list] of [
  [`SEE LICENSE IN`, seeLicenseIn],
  [`UNLICENSED`, unlicensed],
  [`non-SPDX declaration (emitted verbatim, licenses: [])`, nonSpdx],
  [`no \`license\` field`, undetermined],
  [`NO licence determinable at all (no field, no file)`, noLicenceAtAll],
  [`platform-gated with no family evidence and no fallback`, gatedUndetermined],
  [
    `HOST-EXCLUSIVE gated family with no PLATFORM_LICENCE_FALLBACK — WILL DRIFT between macOS and Linux, add an entry`,
    hostExclusiveFamilies,
  ],
  [`family licence disagreement`, familyDisagreements],
  [
    `gated package with an on-disk licence file (deliberately not read)`,
    gatedWithOnDiskFile,
  ],
] as const) {
  if (list.length === 0) continue
  warn(`${label} (${list.length}):`)
  for (const entry of [...new Set(list)].sort(byCodepoint)) warn(`  ${entry}`)
}

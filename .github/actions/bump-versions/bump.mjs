#!/usr/bin/env node
// EXP-1430: config-driven release version bump. Dependency-free Node ESM so any
// repo (Exponential, spezl) runs it on any runner through the composite action.
//
//   node bump.mjs --config .github/release-versions.json [--root .] [--dry-run] \
//     desktop=patch android=0.14.75 ios=minor
//
// stderr: the plan (one -/+ pair per rewritten line). stdout: the resulting
// tags, one per line. In Actions it also writes `tags` + `versions` outputs.
import { readFileSync, writeFileSync, appendFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { execFileSync } from 'node:child_process'
import { pathToFileURL } from 'node:url'

const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/

export class BumpError extends Error {}
const fail = (msg) => {
  throw new BumpError(msg)
}

export const parseVersion = (v) => {
  const m = SEMVER.exec(String(v).trim())
  if (!m) fail(`not a x.y.z version: "${v}"`)
  return m.slice(1, 4).map(Number)
}

const cmp = (a, b) => {
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] - b[i]
  return 0
}

// patch|minor|major|x.y.z → the next version; an explicit one must move forward.
export const nextVersion = (current, spec) => {
  const [maj, min, pat] = parseVersion(current)
  if (spec === `patch`) return `${maj}.${min}.${pat + 1}`
  if (spec === `minor`) return `${maj}.${min + 1}.0`
  if (spec === `major`) return `${maj + 1}.0.0`
  const explicit = parseVersion(spec)
  if (cmp(explicit, [maj, min, pat]) <= 0) fail(`explicit version ${spec} is not above the current ${current}`)
  return explicit.join(`.`)
}

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)

// A field pattern carries one named group `value`; `{current}` is replaced by
// the escaped current version (for files listing many `name@version` rows).
const compile = (field, current) => {
  const src = current === undefined ? field.pattern : field.pattern.replaceAll(`{current}`, escapeRe(current))
  if (!src.includes(`(?<value>`)) fail(`pattern for ${field.file} has no (?<value>…) group: ${field.pattern}`)
  const flags = new Set(`${field.flags ?? `m`}gd`)
  return new RegExp(src, [...flags].join(``))
}

const findMatches = (text, field, current) => {
  const matches = [...text.matchAll(compile(field, current))]
  const label = `${field.file}: /${field.pattern}/`
  if (matches.length === 0) fail(`${label} matched 0 times`)
  if (matches.length > 1 && !field.all) fail(`${label} matched ${matches.length} times (expected exactly 1; set "all": true to rewrite every match)`)
  return matches
}

const lineAt = (text, index) => {
  const start = text.lastIndexOf(`\n`, index - 1) + 1
  const end = text.indexOf(`\n`, index)
  return text.slice(start, end === -1 ? text.length : end)
}

// Rewrites every match's `value` group (right to left so indices hold).
// An `all` field with many matches shows up once in the plan (count + first).
const rewrite = (text, matches, next, plan, file) => {
  let out = text
  let first
  for (const m of [...matches].reverse()) {
    const [s, e] = m.indices.groups.value
    const before = lineAt(out, s)
    out = out.slice(0, s) + next(m.groups.value) + out.slice(e)
    first = { before, after: lineAt(out, s) }
  }
  const n = matches.length > 1 ? `${matches.length} matches, first: ` : ``
  plan.push({ file, before: n + first.before.trim(), after: n + first.after.trim() })
  return out
}

// Cargo.lock: every workspace package (no `source =` line) still on the old
// workspace version moves to the new one. Cargo itself never runs.
export const bumpCargoLock = (text, from, to) => {
  const changed = []
  const out = text.replace(/^\[\[package\]\]\n[\s\S]*?(?=^\[\[package\]\]$|(?![\s\S]))/gm, (block) => {
    if (/^source = /m.test(block)) return block
    const v = /^version = "([^"]+)"$/m.exec(block)?.[1]
    if (v !== from) return block
    changed.push(/^name = "([^"]+)"$/m.exec(block)?.[1])
    return block.replace(/^version = "[^"]+"$/m, `version = "${to}"`)
  })
  if (changed.length === 0) fail(`Cargo.lock: no workspace package on version ${from}`)
  return { text: out, changed }
}

// The newest `<prefix>x.y.z` tag, for products whose version lives only in tags.
const versionFromTags = (root, prefix) => {
  const raw = execFileSync(`git`, [`tag`, `--list`, `${prefix}*`], { cwd: root, encoding: `utf8` })
  const re = new RegExp(`^${escapeRe(prefix)}(\\d+\\.\\d+\\.\\d+)$`)
  const found = raw.split(`\n`).map((t) => re.exec(t.trim())?.[1]).filter(Boolean)
  if (found.length === 0) fail(`no ${prefix}x.y.z tag found (fetch tags: actions/checkout fetch-depth: 0)`)
  return found.sort((a, b) => cmp(parseVersion(b), parseVersion(a)))[0]
}

const readCache = (root) => {
  const files = new Map()
  return {
    get: (file) => {
      if (!files.has(file)) files.set(file, readFileSync(resolve(root, file), `utf8`))
      return files.get(file)
    },
    set: (file, text) => files.set(file, text),
    entries: () => files.entries(),
  }
}

export const bump = ({ config, requests, root = process.cwd(), dryRun = false }) => {
  const products = config.products ?? fail(`config has no "products"`)
  const cache = readCache(root)
  const originals = new Map()
  const load = (file) => {
    const t = cache.get(file)
    if (!originals.has(file)) originals.set(file, t)
    return t
  }
  const plan = []
  const results = []

  for (const [name, spec] of Object.entries(requests)) {
    const p = products[name] ?? fail(`unknown product "${name}" (config has: ${Object.keys(products).join(`, `)})`)
    const versionFields = (p.fields ?? []).filter((f) => (f.kind ?? `version`) === `version`)
    const buildFields = (p.fields ?? []).filter((f) => f.kind === `build`)

    let current
    if (p.versionFromTags !== undefined) current = versionFromTags(root, p.versionFromTags)
    // `{current}` patterns can only check, not discover, the current version.
    for (const f of versionFields.filter((f) => !f.pattern.includes(`{current}`))) {
      for (const m of findMatches(load(f.file), f)) {
        const v = m.groups.value
        if (current === undefined) current = v
        else if (v !== current) fail(`${name}: ${f.file} has version ${v}, expected ${current} like the other files`)
      }
    }
    if (current === undefined) fail(`${name}: no version field and no "versionFromTags"`)
    const version = nextVersion(current, spec)

    for (const f of versionFields) {
      const text = load(f.file)
      cache.set(f.file, rewrite(text, findMatches(text, f, current), () => version, plan, f.file))
    }
    const builds = {}
    for (const f of buildFields) {
      const text = load(f.file)
      const step = f.step ?? 1
      cache.set(
        f.file,
        rewrite(text, findMatches(text, f), (v) => {
          if (!/^\d+$/.test(v)) fail(`${f.file}: build number "${v}" is not an integer`)
          builds[f.name ?? f.file] = String(Number(v) + step)
          return builds[f.name ?? f.file]
        }, plan, f.file),
      )
    }
    for (const file of p.cargoLock ? [p.cargoLock].flat() : []) {
      const { text, changed } = bumpCargoLock(load(file), current, version)
      cache.set(file, text)
      plan.push({ file, before: `${changed.length} workspace packages @ ${current}`, after: `${changed.join(`, `)} @ ${version}` })
    }
    const tags = (p.tags ?? [`${name}-v{version}`]).map((t) => t.replaceAll(`{version}`, version))
    results.push({ product: name, from: current, version, builds, tags })
  }

  if (!dryRun) {
    for (const [file, text] of cache.entries()) {
      if (originals.get(file) !== text) writeFileSync(resolve(root, file), text)
    }
  }
  return { plan, results, files: [...cache.entries()].filter(([f, t]) => originals.get(f) !== t).map(([f]) => f) }
}

// `name=spec` args; `none` and empty specs are skipped so a workflow can pass
// every product unconditionally.
export const parseArgs = (argv) => {
  const opts = { requests: {}, dryRun: false, config: undefined, root: undefined }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === `--dry-run`) opts.dryRun = true
    else if (a === `--config`) opts.config = argv[++i]
    else if (a === `--root`) opts.root = argv[++i]
    else if (a.includes(`=`)) {
      const [name, spec] = a.split(`=`, 2).map((s) => s.trim())
      if (!spec || spec === `none`) continue
      if (opts.requests[name]) fail(`product "${name}" given twice`)
      opts.requests[name] = spec
    } else if (a.trim()) fail(`unexpected argument "${a}" (want --config <file>, --root <dir>, --dry-run, product=patch|minor|major|x.y.z)`)
  }
  if (!opts.config) fail(`--config <file> is required`)
  return opts
}

const main = () => {
  const opts = parseArgs(process.argv.slice(2))
  const root = resolve(opts.root ?? process.cwd())
  const config = JSON.parse(readFileSync(resolve(root, opts.config), `utf8`))
  if (Object.keys(opts.requests).length === 0) fail(`nothing to bump: pass at least one product=patch|minor|major|x.y.z`)
  const { plan, results, files } = bump({ config, requests: opts.requests, root, dryRun: opts.dryRun })

  const log = (s) => process.stderr.write(`${s}\n`)
  log(opts.dryRun ? `dry run, nothing written:` : `bumped ${files.length} files:`)
  for (const r of results) {
    const b = Object.entries(r.builds).map(([k, v]) => ` ${k}=${v}`).join(``)
    log(`  ${r.product}: ${r.from} -> ${r.version}${b}`)
  }
  for (const { file, before, after } of plan) log(`${file}\n  - ${before.trim()}\n  + ${after.trim()}`)
  const tags = results.flatMap((r) => r.tags)
  process.stdout.write(`${tags.join(`\n`)}\n`)

  if (process.env.GITHUB_OUTPUT) {
    const versions = Object.fromEntries(results.map((r) => [r.product, { version: r.version, ...r.builds }]))
    appendFileSync(
      process.env.GITHUB_OUTPUT,
      `tags<<EXP_BUMP_EOF\n${tags.join(`\n`)}\nEXP_BUMP_EOF\nversions=${JSON.stringify(versions)}\nfiles=${files.join(` `)}\n`,
    )
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? ``).href) {
  try {
    main()
  } catch (e) {
    process.stderr.write(`${process.env.GITHUB_ACTIONS ? `::error::` : ``}bump-versions: ${e.message}\n`)
    process.exit(1)
  }
}

import { describe, expect, it } from "vitest"
import { existsSync, lstatSync, readdirSync, readFileSync } from "node:fs"
import { dirname, join, relative } from "node:path"
import { NOUN_READ_OPS } from "@exp/mcp-apps/tool-names"

// The Claude plugin bundle (integrations/claude, mirrored to the public
// Niach/exponential-claude-plugin repo and submitted to Claude's plugin
// directory): the generated skills stay byte-equal to their sources
// (`bun run claude-plugin:sync`), the manifests stay valid, and every file
// stays inside the directory's limits (< 256 KiB, no OS junk, no symlinks).

/** The repo root, walked up from the vitest cwd (apps/web). */
function repoRoot(): string {
  let dir = process.cwd()
  while (!existsSync(join(dir, `integrations`, `claude`)) && dirname(dir) !== dir) dir = dirname(dir)
  return dir
}

const ROOT = repoRoot()
const PLUGIN = join(ROOT, `integrations`, `claude`)
const GENERATED = join(ROOT, `packages`, `exponential-ui`, `skills`)
const SYNCED_SKILLS = [`exponential-genui`, `exponential-apps`, `exponential-motion`]

/** Every entry under `dir`, relative, sorted (symlinks reported, not followed). */
function walk(dir: string): string[] {
  const out: string[] = []
  const visit = (current: string) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const full = join(current, entry.name)
      out.push(relative(dir, full))
      if (entry.isDirectory()) visit(full)
    }
  }
  visit(dir)
  return out.sort()
}

const filesOf = (dir: string) => walk(dir).filter((path) => lstatSync(join(dir, path)).isFile())

const readJson = (path: string) => JSON.parse(readFileSync(join(PLUGIN, path), `utf8`)) as Record<string, unknown>

/** The frontmatter's top-level `key: value` lines (a single-line scalar each). */
function frontmatter(markdown: string): Record<string, string> | null {
  const match = /^---\n([\s\S]*?)\n---\n/.exec(markdown)
  if (!match) return null
  const fields: Record<string, string> = {}
  for (const line of match[1]!.split(`\n`)) {
    const field = /^([a-zA-Z][\w-]*):\s*(.*)$/.exec(line)
    if (field) fields[field[1]!] = field[2]!
  }
  return fields
}

/** A YAML single-line scalar that is a string (not a list, map or block). */
function isStringScalar(raw: string): boolean {
  if (raw.startsWith(`"`)) {
    try {
      return typeof JSON.parse(raw) === `string`
    } catch {
      return false
    }
  }
  if (raw.startsWith(`'`)) return raw.endsWith(`'`) && raw.length > 1
  return raw.length > 0 && !/^[-[{|>&*!%@`]/.test(raw)
}

describe(`Claude plugin bundle`, () => {
  it(`carries the generated skills byte-for-byte`, () => {
    for (const name of SYNCED_SKILLS) {
      const source = join(GENERATED, name)
      const copy = join(PLUGIN, `skills`, name)
      const files = filesOf(source)
      expect(walk(copy), `${name}: run bun run claude-plugin:sync`).toEqual(walk(source))
      for (const file of files) {
        expect(readFileSync(join(copy, file)).equals(readFileSync(join(source, file))), `${name}/${file} is stale: run bun run claude-plugin:sync`).toBe(true)
      }
    }
  })

  it(`has a valid manifest, marketplace and connector`, () => {
    const plugin = readJson(`.claude-plugin/plugin.json`)
    expect(plugin.name).toBe(`exponential`)
    expect(plugin.version).toMatch(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/)
    expect(plugin.license).toBe(`Apache-2.0`)
    const marketplace = readJson(`.claude-plugin/marketplace.json`)
    expect(marketplace.plugins).toEqual([expect.objectContaining({ name: `exponential`, source: `./` })])
    const mcp = readJson(`.mcp.json`) as { mcpServers: Record<string, { type: string; url: string }> }
    expect(mcp.mcpServers).toEqual({ exponential: { type: `http`, url: `https://app.exponential.at/api/mcp` } })
  })

  it(`has a README of at least 40 words outside code`, () => {
    const readme = readFileSync(join(PLUGIN, `README.md`), `utf8`)
    const prose = readme.replace(/```[\s\S]*?```/g, ` `)
    expect(prose.split(/\s+/).filter((word) => /[A-Za-z0-9]/.test(word)).length).toBeGreaterThanOrEqual(40)
    expect(readFileSync(join(PLUGIN, `LICENSE`), `utf8`)).toBe(readFileSync(join(ROOT, `LICENSE`), `utf8`))
  })

  it(`keeps every file inside the directory's limits`, () => {
    const entries = walk(PLUGIN)
    expect(entries.length).toBeLessThanOrEqual(512)
    for (const path of entries) {
      const stat = lstatSync(join(PLUGIN, path))
      expect(stat.isSymbolicLink(), `${path} is a symlink`).toBe(false)
      expect(path, `${path} is OS junk`).not.toMatch(/(^|\/)(\.DS_Store|Thumbs\.db|desktop\.ini|__MACOSX)$/)
      expect(path, `${path}: unsafe name`).toMatch(/^[A-Za-z0-9._/-]+$/)
      if (stat.isFile()) {
        expect(stat.size, `${path} is over 256 KiB`).toBeLessThan(256 * 1024)
        expect(path, `${path}: text, SVG or PNG only`).toMatch(/(\.(md|json|svg|png)|LICENSE)$/)
      }
    }
  })

  it(`gives every skill a name and a single-string description`, () => {
    const skills = readdirSync(join(PLUGIN, `skills`))
    expect(skills.sort()).toEqual([`exponential`, ...SYNCED_SKILLS].sort())
    for (const name of skills) {
      const fields = frontmatter(readFileSync(join(PLUGIN, `skills`, name, `SKILL.md`), `utf8`))
      expect(fields, `${name}/SKILL.md has no frontmatter`).not.toBeNull()
      expect(fields!.name).toBe(name)
      expect(isStringScalar(fields!.description ?? ``), `${name}: description must be one string`).toBe(true)
    }
  })

  it(`names only tools the connector registers in the core skill`, () => {
    // #VAPP-293: a noun with reads = its read tool + its `_write` twin;
    // `session_self` has only writes and keeps its name; two verbs.
    const known = new Set([
      ...Object.keys(NOUN_READ_OPS).flatMap((noun) => [`exponential_${noun}`, `exponential_${noun}_write`]),
      `exponential_session_self`,
      `exponential_sessions_send`,
      `exponential_show`,
    ])
    expect(known.size).toBe(41)
    const skill = readFileSync(join(PLUGIN, `skills`, `exponential`, `SKILL.md`), `utf8`)
    const named = new Set(skill.match(/\bexponential_[a-z_]+/g) ?? [])
    expect(named.size).toBeGreaterThan(10)
    expect([...named].filter((name) => !known.has(name))).toEqual([])
    expect(Buffer.byteLength(skill)).toBeLessThan(12 * 1024)
  })
})

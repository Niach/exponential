// #NEW-145 / #NEW-143 D2-D3: Skills over MCP (`io.modelcontextprotocol/skills`):
// the team skills a credential reaches plus the built-ins, as `skill://`
// URIs with complete manifests, digests over the served bytes and the
// cache hints. Pure (node:crypto only) and locked by `fixtures/skills.json`
// (its `$comment` is the spec).

import { createHash } from "node:crypto"

import fixture from "../fixtures/skills.json" with { type: "json" }

export const SKILLS_EXTENSION: string = fixture.extension
export const SKILLS_TTL_MS: number = fixture.ttlMs
export const BUILTIN_ROOT: string = fixture.builtinRoot
export const MCP_BUILTIN_NAMES: readonly string[] = fixture.builtins

export type CacheScope = `private` | `public`

/** `sha256:` + the lowercase hex SHA-256 of the UTF-8 bytes served. */
export function skillDigest(content: string): string {
  return `sha256:${createHash(`sha256`).update(content, `utf8`).digest(`hex`)}`
}

export const byteSize = (content: string) => Buffer.byteLength(content, `utf8`)

/** A team skill's SKILL.md: our frontmatter (name, description; nothing else) + the body. */
export function teamSkillMd(name: string, description: string, body: string): string {
  return `---\nname: ${name}\ndescription: ${JSON.stringify(description)}\n---\n\n${body.trim()}\n`
}

export const skillUri = (root: string, name: string, path = `SKILL.md`) => `${fixture.scheme}${root}/${name}${path ? `/${path}` : ``}`

const SEGMENT = new RegExp(fixture.segmentPattern)

/** `skill://<root>/<name>[/<path>]` → its parts, or null. `path` "" = the skill's directory. */
export function parseSkillUri(uri: unknown): { root: string; name: string; path: string } | null {
  if (typeof uri !== `string` || !uri.startsWith(fixture.scheme) || uri.length > fixture.uriMax) return null
  const rest = uri.slice(fixture.scheme.length)
  if (/[?#\\\u0000]/.test(rest) || rest.endsWith(`/`)) return null
  const parts = rest.split(`/`)
  if (parts.length < 2) return null
  const [root, name, ...path] = parts
  if (!SEGMENT.test(root!) || !SEGMENT.test(name!)) return null
  if (path.some((segment) => segment === `` || segment === `.` || segment === `..`)) return null
  return { root: root!, name: name!, path: path.join(`/`) }
}

export interface TeamSkillRow {
  name: string
  description: string
  body: string
  /** A built-in's per-team row: never served as a team skill. */
  builtin: boolean
}

export interface ReachableTeam {
  slug: string
  skills: TeamSkillRow[]
}

export interface BuiltinSource {
  name: string
  description: string
  /** The whole SKILL.md. */
  skillMd: string
  files: { path: string; content: string }[]
}

export interface ServedSkill {
  root: string
  name: string
  scope: CacheScope
  frontmatter: { name: string; description: string }
  /** path → content; `SKILL.md` first, then the files by path. */
  files: Map<string, string>
}

const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

/** Every skill a caller is served: its reachable teams' own skills (by slug,
 *  then name), then the MCP built-ins (fixture order). A team skill at a
 *  built-in's URI is shadowed by the built-in. */
export function servedSkills(teams: readonly ReachableTeam[], builtins: readonly BuiltinSource[]): ServedSkill[] {
  const shipped = MCP_BUILTIN_NAMES.map((name) => builtins.find((b) => b.name === name)).filter((b): b is BuiltinSource => !!b)
  const builtinUris = new Set(shipped.map((b) => skillUri(BUILTIN_ROOT, b.name)))
  const out: ServedSkill[] = []
  for (const team of [...teams].sort((a, b) => compare(a.slug, b.slug))) {
    for (const row of [...team.skills].filter((s) => !s.builtin).sort((a, b) => compare(a.name, b.name))) {
      if (builtinUris.has(skillUri(team.slug, row.name))) continue
      out.push({
        root: team.slug,
        name: row.name,
        scope: `private`,
        frontmatter: { name: row.name, description: row.description },
        files: new Map([[`SKILL.md`, teamSkillMd(row.name, row.description, row.body)]]),
      })
    }
  }
  for (const b of shipped) {
    const files = new Map<string, string>([[`SKILL.md`, b.skillMd]])
    for (const f of [...b.files].sort((x, y) => compare(x.path, y.path))) files.set(f.path, f.content)
    out.push({ root: BUILTIN_ROOT, name: b.name, scope: `public`, frontmatter: { name: b.name, description: b.description }, files })
  }
  return out
}

export interface SkillEntry {
  uri: string
  frontmatter: { name: string; description: string }
  resources: { uri: string; digest: string; size: number }[]
}

function entry(skill: ServedSkill): SkillEntry {
  return {
    uri: skillUri(skill.root, skill.name),
    frontmatter: { name: skill.frontmatter.name, description: skill.frontmatter.description },
    resources: [...skill.files].map(([path, content]) => ({ uri: skillUri(skill.root, skill.name, path), digest: skillDigest(content), size: byteSize(content) })),
  }
}

/** A listing's scope: private as soon as it carries one team skill. */
const scopeOf = (skills: readonly ServedSkill[]): CacheScope => (skills.some((s) => s.scope === `private`) ? `private` : `public`)

export interface McpError {
  error: { code: number; message: string }
}

const err = (key: keyof typeof fixture.errors, uri: unknown): McpError => ({
  error: { code: fixture.errorCode, message: fixture.errors[key].replace(`{uri}`, typeof uri === `string` ? uri : String(uri)) },
})

/** `skills/list` (one page: a `cursor` is refused). */
export function skillsList(served: readonly ServedSkill[], params: { cursor?: unknown } = {}) {
  if (params.cursor !== undefined) return err(`badCursor`, params.cursor)
  return { resultType: `complete` as const, skills: served.map(entry), ttlMs: SKILLS_TTL_MS, cacheScope: scopeOf(served) }
}

const find = (served: readonly ServedSkill[], root: string, name: string) => served.find((s) => s.root === root && s.name === name)

/** `skills/get {uri}`: only a SKILL.md URI names a skill. */
export function skillsGet(served: readonly ServedSkill[], uri: unknown) {
  if (typeof uri !== `string` || !uri) return err(`missingUri`, uri)
  const parsed = parseSkillUri(uri)
  const skill = parsed && parsed.path === `SKILL.md` ? find(served, parsed.root, parsed.name) : undefined
  if (!skill) return err(`notServed`, uri)
  return { resultType: `complete` as const, skill: entry(skill), ttlMs: SKILLS_TTL_MS, cacheScope: skill.scope }
}

/** The mime type of a served path. */
export function mimeTypeOf(path: string): string {
  const types = fixture.mimeTypes as Record<string, string>
  const ext = /\.[a-z0-9]+$/i.exec(path)?.[0]?.toLowerCase() ?? ``
  return types[ext] ?? types.default!
}

/** `resources/read {uri}` of one served skill file. */
export function skillResourceRead(served: readonly ServedSkill[], uri: unknown) {
  if (typeof uri !== `string` || !uri) return err(`missingUri`, uri)
  const parsed = parseSkillUri(uri)
  const skill = parsed ? find(served, parsed.root, parsed.name) : undefined
  const text = skill && parsed!.path ? skill.files.get(parsed!.path) : undefined
  if (text === undefined) return err(`notServed`, uri)
  return { resultType: `complete` as const, contents: [{ uri, mimeType: mimeTypeOf(parsed!.path), text }], ttlMs: SKILLS_TTL_MS, cacheScope: skill!.scope }
}

/** `resources/directory/read {uri}`: the direct children of a skill directory. */
export function skillDirectoryRead(served: readonly ServedSkill[], uri: unknown) {
  if (typeof uri !== `string` || !uri) return err(`missingUri`, uri)
  const parsed = parseSkillUri(uri)
  const skill = parsed ? find(served, parsed.root, parsed.name) : undefined
  if (!skill) return err(`notServed`, uri)
  if (skill.files.has(parsed!.path)) return err(`notDirectory`, uri)
  const prefix = parsed!.path ? `${parsed!.path}/` : ``
  const children = new Map<string, boolean>()
  for (const path of skill.files.keys()) {
    if (!path.startsWith(prefix)) continue
    const [head, ...more] = path.slice(prefix.length).split(`/`)
    children.set(head!, (children.get(head!) ?? false) || more.length > 0)
  }
  if (children.size === 0) return err(`notServed`, uri)
  const resources = [...children]
    .sort(([a], [b]) => (a === `SKILL.md` ? -1 : b === `SKILL.md` ? 1 : compare(a, b)))
    .map(([name, dir]) => ({ uri: skillUri(skill.root, skill.name, `${prefix}${name}`), name, mimeType: dir ? fixture.directoryMimeType : mimeTypeOf(name) }))
  return { resultType: `complete` as const, resources, ttlMs: SKILLS_TTL_MS, cacheScope: skill.scope }
}

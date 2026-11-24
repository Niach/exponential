import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join, relative } from "node:path"
import { fileURLToPath } from "node:url"
import { claudeMcpJson, openclawServers } from "./mcp-entry"
import { renderPlaybook } from "./playbook"
import { renderOpenClawTheme } from "./theme/openclaw"
import { renderToneColors } from "./theme/colors"

// #NEW-23: everything a harness adapter carries that has ONE source here.
// `bun run --filter @exp/plugin-kit generate` writes the files; the drift
// gate (`generate.test.ts`, and apps/web/src/lib/plugin-kit.test.ts in the PR
// gate) fails when one was edited by hand or left behind.

export const REPO_ROOT = fileURLToPath(new URL(`../../../`, import.meta.url))

export interface Output {
  /** Repo-relative. */
  path: string
  content: string
}

/** A folder whose every file is generated: a stale extra file is drift. */
export interface OutputFolder {
  path: string
  files: Output[]
}

const read = (path: string) => readFileSync(join(REPO_ROOT, path), `utf8`)

/** `openclaw.plugin.json` with its server block taken from the entry; the
 *  rest of the manifest (version, theme listing) is the adapter's own. */
function openclawManifest(): string {
  const manifest = JSON.parse(read(`integrations/openclaw/openclaw.plugin.json`)) as Record<string, unknown>
  return `${JSON.stringify({ ...manifest, mcpServers: openclawServers() }, null, 2)}\n`
}

export function outputs(): Output[] {
  return [
    { path: `packages/plugin-kit/src/model/core/colors.ts`, content: renderToneColors() },
    { path: `integrations/claude/.mcp.json`, content: claudeMcpJson() },
    { path: `integrations/claude/skills/exponential/SKILL.md`, content: renderPlaybook(`claude`) },
    { path: `integrations/openclaw/openclaw.plugin.json`, content: openclawManifest() },
    { path: `integrations/openclaw/skills/exponential/SKILL.md`, content: renderPlaybook(`openclaw`) },
    { path: `integrations/openclaw/themes/exponential.json`, content: renderOpenClawTheme() },
  ]
}

export function outputFolders(): OutputFolder[] {
  return []
}

/** Every file under `dir`, relative, sorted. */
function walk(dir: string): string[] {
  if (!existsSync(dir)) return []
  const out: string[] = []
  const visit = (current: string) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const full = join(current, entry.name)
      if (entry.isDirectory()) visit(full)
      else out.push(relative(dir, full))
    }
  }
  visit(dir)
  return out.sort()
}

/** The repo-relative paths that differ from what the sources render. */
export function drift(): string[] {
  const stale: string[] = []
  const check = (output: Output) => {
    const full = join(REPO_ROOT, output.path)
    if (!existsSync(full) || readFileSync(full, `utf8`) !== output.content) stale.push(output.path)
  }
  outputs().forEach(check)
  for (const folder of outputFolders()) {
    folder.files.forEach(check)
    const expected = new Set(folder.files.map((file) => relative(folder.path, file.path)))
    for (const file of walk(join(REPO_ROOT, folder.path))) {
      if (!expected.has(file)) stale.push(join(folder.path, file))
    }
  }
  return stale
}

export function write(): string[] {
  const written: string[] = []
  const put = (output: Output) => {
    const full = join(REPO_ROOT, output.path)
    if (existsSync(full) && readFileSync(full, `utf8`) === output.content) return
    mkdirSync(dirname(full), { recursive: true })
    writeFileSync(full, output.content)
    written.push(output.path)
  }
  for (const folder of outputFolders()) {
    const expected = new Set(folder.files.map((file) => relative(folder.path, file.path)))
    for (const file of walk(join(REPO_ROOT, folder.path))) {
      if (expected.has(file)) continue
      rmSync(join(REPO_ROOT, folder.path, file))
      written.push(join(folder.path, file))
    }
    folder.files.forEach(put)
  }
  // `colors.ts` first: later outputs may copy it.
  outputs().forEach(put)
  return written
}

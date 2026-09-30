// EXP-1153: build the ZIP the ChatGPT/Codex plugin directory takes.
//
//   bun run --filter @exp/chatgpt-plugin pack
//
// Copies plugin.json, mcp.json, skills/ and assets/ into
// dist/exponential/ and zips that folder to dist/exponential-<version>.zip.
// The archive root IS the plugin root (the portal accepts either that or a
// single top-level directory). Nothing else belongs in it: no package.json,
// no tests, no lockfile — "keep private credentials and secrets out of the
// ZIP" starts with keeping the repo out of it.

import { cpSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

const root = resolve(import.meta.dir, `..`)
const manifest = JSON.parse(
  readFileSync(join(root, `plugin.json`), `utf8`)
) as { name: string; version: string }

const dist = join(root, `dist`)
const stage = join(dist, manifest.name)
const zipName = `${manifest.name}-${manifest.version}.zip`
const zipPath = join(dist, zipName)

rmSync(stage, { recursive: true, force: true })
rmSync(zipPath, { force: true })
mkdirSync(stage, { recursive: true })

for (const entry of [`plugin.json`, `mcp.json`, `skills`, `assets`]) {
  const from = join(root, entry)
  if (!existsSync(from)) throw new Error(`missing ${entry}`)
  cpSync(from, join(stage, entry), { recursive: true })
}

// `zip -r -X` from INSIDE the stage so entries are plugin-root relative;
// -X drops the macOS extended attributes that would otherwise ride along.
const proc = Bun.spawnSync([`zip`, `-r`, `-X`, `-q`, zipPath, `.`], {
  cwd: stage,
  stdout: `inherit`,
  stderr: `inherit`,
})
if (proc.exitCode !== 0) {
  throw new Error(`zip exited with ${proc.exitCode}`)
}

console.log(`wrote ${zipPath}`)

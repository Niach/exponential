// The Claude plugin (integrations/claude, mirrored to the public
// Niach/exponential-claude-plugin repo) ships three skills generated from the
// SDK docs: packages/exponential-ui/skills/<name>. This copies them in
// (delete, then copy), so a regeneration is one command:
//   bun run claude-plugin:sync
// apps/web/src/lib/claude-plugin.test.ts fails while the copies are stale.
import { cpSync, existsSync, rmSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

export const CLAUDE_PLUGIN_SYNCED_SKILLS = [`exponential-genui`, `exponential-apps`, `exponential-motion`] as const

const root = join(dirname(fileURLToPath(import.meta.url)), `..`)
const source = join(root, `packages`, `exponential-ui`, `skills`)
const target = join(root, `integrations`, `claude`, `skills`)

for (const name of CLAUDE_PLUGIN_SYNCED_SKILLS) {
  const from = join(source, name)
  const to = join(target, name)
  if (!existsSync(join(from, `SKILL.md`))) throw new Error(`Missing generated skill: ${from}/SKILL.md`)
  rmSync(to, { recursive: true, force: true })
  // dereference: the plugin repo carries regular files, never symlinks.
  cpSync(from, to, { recursive: true, dereference: true, filter: (path) => !/(^|\/)(\.DS_Store|Thumbs\.db)$/.test(path) })
  console.log(`synced ${name}`)
}

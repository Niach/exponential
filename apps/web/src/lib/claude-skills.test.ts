// @vitest-environment node
// #NEW-149: the Claude plugin (integrations/claude) ships copies of the
// built-in skills (packages/exponential-ui/skills, generated from the genUI
// spec and served over MCP, skills.json); this gate fails when a copy
// drifts from its source, a built-in is left out, or a stray folder appears.
// The plugin's own `exponential` playbook (plugin-kit, generate.test.ts) is
// the one folder that is not a built-in; it stands in for the built-in
// `exponential-workflow`, which the plugin does not ship.
import { existsSync, readdirSync, readFileSync } from "node:fs"
import { join, relative } from "node:path"
import { describe, expect, it } from "vitest"

const ROOT = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const PLUGIN = join(ROOT, `integrations`, `claude`, `skills`)
const BUILTINS = join(ROOT, `packages`, `exponential-ui`, `skills`)
const shipped = JSON.parse(readFileSync(join(BUILTINS, `index.json`), `utf8`)) as { skills: { name: string }[] }
/** The plugin's own folder (plugin-kit) and the built-in it replaces there. */
const PLAYBOOK = `exponential`
const NOT_SHIPPED = [`exponential-workflow`]

function files(dir: string): string[] {
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

describe(`integrations/claude/skills vs the built-in skills`, () => {
  const builtins = shipped.skills.map((skill) => skill.name)

  it(`ships every built-in but the workflow, plus its playbook, and nothing else`, () => {
    const folders = readdirSync(PLUGIN, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort()
    expect(folders).toEqual([PLAYBOOK, ...builtins.filter((name) => !NOT_SHIPPED.includes(name))].sort())
  })

  for (const name of builtins.filter((n) => !NOT_SHIPPED.includes(n))) {
    it(`${name} is byte for byte the built-in (copy packages/exponential-ui/skills/${name} over it)`, () => {
      const plugin = join(PLUGIN, name)
      const source = join(BUILTINS, name)
      expect(existsSync(plugin)).toBe(true)
      expect(files(plugin)).toEqual(files(source))
      for (const path of files(source)) expect(readFileSync(join(plugin, path), `utf8`), path).toBe(readFileSync(join(source, path), `utf8`))
    })
  }
})

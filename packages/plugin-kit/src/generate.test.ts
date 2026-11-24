import { drift, outputs } from "./generate"
import { buildOpenClawTheme } from "./theme/openclaw"
import { PLAYBOOKS, renderPlaybook, type PlaybookHost } from "./playbook"

// #NEW-23: every adapter output is what the sources render. A token, entry
// or playbook change that leaves an adapter behind fails here, like the
// other generated cross-client outputs.
describe(`adapter outputs`, () => {
  it(`are current (bun run --filter @exp/plugin-kit generate)`, () => {
    expect(drift()).toEqual([])
  })

  it(`never write outside the adapters and this package`, () => {
    for (const output of outputs()) {
      expect(output.path).toMatch(/^(integrations\/|packages\/plugin-kit\/src\/model\/core\/)/)
    }
  })
})

// EXP-1183: the OpenClaw plugin's "Exponential" theme is generated from the
// styleguide (the design-tokens glass system + the contract's working verbs).
describe(`OpenClaw theme`, () => {
  it(`fits OpenClaw's theme limits`, () => {
    const theme = buildOpenClawTheme()
    expect(new TextEncoder().encode(JSON.stringify(theme)).length).toBeLessThan(4096)
    expect(theme.workingPhrases.length).toBeLessThanOrEqual(24)
    for (const phrase of theme.workingPhrases) expect(phrase.length).toBeLessThanOrEqual(24)
    // Dark only, like every Exponential client.
    expect(Object.keys(theme)).not.toContain(`light`)
    for (const value of Object.values(theme.dark)) {
      expect(value.length).toBeLessThanOrEqual(120)
    }
    // OpenClaw mixes its colours, so the surfaces are opaque.
    for (const key of [`background`, `card`, `popover`, `secondary`, `muted`, `accent`, `border`, `input`]) {
      expect(theme.dark[key]).toMatch(/^#[0-9a-f]{6}$/)
    }
  })
})

describe(`playbook`, () => {
  it(`renders one skill per host with nothing unfilled`, () => {
    for (const host of Object.keys(PLAYBOOKS) as PlaybookHost[]) {
      const skill = renderPlaybook(host)
      expect(skill.startsWith(`---\nname: exponential\ndescription: "`)).toBe(true)
      expect(skill).not.toMatch(/\{\{/)
      // A skill is read whole on every use.
      expect(Buffer.byteLength(skill)).toBeLessThan(12 * 1024)
    }
  })

  it(`names the host, never another one`, () => {
    expect(renderPlaybook(`openclaw`)).not.toMatch(/\bClaude\b(?! Code, Codex)/)
    expect(renderPlaybook(`openclaw`)).not.toContain(`exponential-genui`)
    expect(renderPlaybook(`claude`)).toContain(`the \`exponential-genui\` skill covers the format`)
  })
})

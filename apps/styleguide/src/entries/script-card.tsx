import { scriptCard, type ScriptCardInput } from "@exp/domain-contract/script-card"
import fixture from "@exp/domain-contract/fixtures/script-card.json"
import { ScriptCard } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1279 (EXP-1259 D13): the Script card a code-mode `exec` call renders
// as — the REAL `ScriptCard` over the contract model (`scriptCard()`), fed
// the fixture's grouped case (the IDE entry's too), then a running script.

/** A short script, the shape an agent writes. */
const SOURCE = [
  `const ids = ["EXP-1253", "EXP-1254", "EXP-1255"]`,
  `const issues = await Promise.all(ids.map((id) => tools.exponential.exponential_issues_get({ id })))`,
  `return issues.map((issue) => issue.title)`,
].join(`\n`)

const GROUPED = fixture.cases[1].input as ScriptCardInput
const RUNNING: ScriptCardInput = { running: true }

export const entry: StyleguideEntry = {
  id: `script-card`,
  section: `special`,
  owner: `EXP-1279`,
  title: `Script card`,
  blurb: `A code-mode call (\`mcp__codemode__exec\`, contract toolKind \`script\`) is ONE card, never a member of a tool fold: the \`{ }\` glyph, \`Ran a script\` and the muted \`59 calls · 3.0 s\`, the script itself behind the trailing \`Script\` disclosure (a mono block). Then one row per tool the script called, in first-call order: our mark on an Exponential tool (its \`expToolDisplay\` verb), the generic tool glyph otherwise, the first subject + \`+n\`, trailing \`×count\` or \`1 failed\` in rose; a capped log ends in \`201 more calls\`. While it runs: the title alone with a spinner. Without a log (an older code mode) it reads \`Ran a script\`. Model = \`scriptCard()\` (fixture \`script-card.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ScriptCard`,
      file: `packages/ui/src/script-card.tsx`,
      note: `model: @exp/domain-contract/script-card; row: apps/web/src/components/agent-session.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `script_card`,
      file: `apps/desktop/crates/ui/src/script_card.rs`,
      note: `styleguide/entries/script_card.rs`,
    },
    ios: { state: `n/a`, note: `EXP-1259 wave B (I2) ports it.` },
    android: { state: `n/a`, note: `EXP-1259 wave B (A2) ports it.` },
  },
  island: () => (
    <div className="flex max-w-2xl flex-col gap-3 text-sm">
      <ScriptCard card={scriptCard(GROUPED)} source={SOURCE} />
      <ScriptCard card={scriptCard(RUNNING)} running />
    </div>
  ),
}

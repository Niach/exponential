import { describe, expect, it } from "vitest"

import { agentMarkIcon } from "@/components/launch-dialog/launch-options-pane"
import { ClaudeIcon, CodexIcon } from "@/components/icons/brand-icons"
import { conceptIcon } from "@/lib/icons.generated"

// EXP-849: the brand marks are hand-drawn, one per shipped agent. An id from
// outside that set — a retired `pi` row, a future agent, an external ACP
// binary — used to render NOTHING in the agent strip; it now draws the
// neutral agent concept, the fallback glyph desktop, iOS and Android pick.
describe(`agentMarkIcon`, () => {
  it(`draws each shipped agent's own mark`, () => {
    expect(agentMarkIcon(`claude`)).toBe(ClaudeIcon)
    expect(agentMarkIcon(`codex`)).toBe(CodexIcon)
  })

  it(`falls back to the neutral agent concept for an unknown id`, () => {
    const neutral = conceptIcon(`settings-agents`)
    for (const id of [`pi`, `external`, ``]) {
      expect(agentMarkIcon(id)).toBe(neutral)
    }
  })
})

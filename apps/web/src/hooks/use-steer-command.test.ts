import { describe, expect, it } from "vitest"
import { btwQuestion } from "@/hooks/use-steer-command"

// EXP-1323 (#EXP-1330): `/btw <question>` is the only steer command the web
// dispatches itself (an aside child); everything else stays message text.
describe(`btwQuestion`, () => {
  it(`reads the question of a claude session's /btw`, () => {
    expect(btwQuestion(`/btw what does useCart return?`, `claude`)).toBe(
      `what does useCart return?`
    )
    expect(btwQuestion(`  /BTW  why?  `, null)).toBe(`why?`)
  })

  it(`leaves other commands, prose, a bare /btw and other agents alone`, () => {
    expect(btwQuestion(`/compact`, `claude`)).toBeNull()
    expect(btwQuestion(`by the way /btw`, `claude`)).toBeNull()
    expect(btwQuestion(`/btw`, `claude`)).toBeNull()
    expect(btwQuestion(`/btw why?`, `codex`)).toBeNull()
  })
})

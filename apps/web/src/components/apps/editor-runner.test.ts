import { describe, expect, it } from "vitest"
import type { AccountOption } from "@/lib/accounts/account-option"
import { endedRootRunner } from "./editor-runner"

// #EXP-1729 (apps.json `editorHeader`): an ENDED editor root revives on its
// own agent: its agent's logins and models, reading its own account/model
// until a pick, and only a pick rides the next send.

const option = (agent: `claude` | `codex`, id: string, email: string): AccountOption => ({
  id,
  agent,
  email,
  isLastUsed: false,
  health: `unknown`,
})

const options = [option(`claude`, `p1`, `a@example.com`), option(`claude`, `p2`, `b@example.com`), option(`codex`, `p3`, `c@example.com`)]

describe(`endedRootRunner (#EXP-1729)`, () => {
  it(`reads the root's own login and model; nothing picked sends nothing`, () => {
    const held = endedRootRunner({ cli: `claude`, account: `p1`, model: `sonnet` }, options, {})
    expect(held.options.map((o) => o.id)).toEqual([`p1`, `p2`])
    expect(held.accountKey).toBe(`claude:p1`)
    expect(held.model).toBe(`sonnet`)
    expect(held.send).toEqual({})
  })

  it(`a pick rides the send`, () => {
    const held = endedRootRunner({ cli: `claude`, account: `p1`, model: `sonnet` }, options, { account: `claude:p2`, model: `opus` })
    expect(held.accountKey).toBe(`claude:p2`)
    expect(held.send).toEqual({ account: `p2`, model: `opus` })
  })

  it(`another agent's login is never offered or sent`, () => {
    const held = endedRootRunner({ cli: `claude`, account: null, model: null }, options, { account: `codex:p3` })
    expect(held.accountKey).toBeNull()
    expect(held.send).toEqual({})
    expect(held.model).toBeTruthy()
  })
})

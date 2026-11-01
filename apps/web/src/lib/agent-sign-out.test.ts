import { describe, expect, it } from "vitest"
import type { DeviceAgentAccounts } from "@exp/db-schema"
import { agentSignOutTitle, detectAgentSignOuts } from "./agent-sign-out"

const claude = (health: string | undefined, signedIn = true): DeviceAgentAccounts =>
  ({
    claude: {
      signedIn,
      email: `danny@acme.dev`,
      profiles: [
        { id: `work`, signedIn, email: `danny@acme.dev`, ...(health ? { health } : {}) },
        { id: `home`, signedIn: true, email: `d@home.dev`, health: `ok` },
      ],
    },
  }) as DeviceAgentAccounts

describe(`detectAgentSignOuts (#EXP-1534)`, () => {
  it(`fires once on ok → signed_out and remembers the login`, () => {
    const first = detectAgentSignOuts(claude(`ok`), claude(`signed_out`), [])
    expect(first.signOuts).toEqual([
      { key: `claude/work`, agent: `claude`, profileId: `work`, email: `danny@acme.dev`, health: `signed_out` },
    ])
    expect(first.notified).toEqual([`claude/work`])
  })

  it(`fires on unknown → needs_relogin`, () => {
    const { signOuts } = detectAgentSignOuts(claude(undefined), claude(`needs_relogin`), [])
    expect(signOuts.map((s) => [s.key, s.health])).toEqual([[`claude/work`, `needs_relogin`]])
  })

  it(`stays quiet on every later heartbeat and on a re-register`, () => {
    const { notified } = detectAgentSignOuts(claude(`ok`), claude(`signed_out`), [])
    // A repeated heartbeat with the same report.
    expect(detectAgentSignOuts(claude(`signed_out`), claude(`signed_out`), notified).signOuts).toEqual([])
    // A restart: the device re-registers `unknown` before its probe, then dead again.
    const restart = detectAgentSignOuts(claude(`signed_out`), claude(undefined), notified)
    expect(restart.notified).toEqual(notified)
    expect(detectAgentSignOuts(claude(undefined), claude(`signed_out`), restart.notified).signOuts).toEqual([])
    // needs_relogin ↔ signed_out flaps stay one notification.
    expect(detectAgentSignOuts(claude(`signed_out`), claude(`needs_relogin`), notified).signOuts).toEqual([])
  })

  it(`re-arms only once the login reads ok again`, () => {
    const dead = detectAgentSignOuts(claude(`ok`), claude(`signed_out`), [])
    const back = detectAgentSignOuts(claude(`signed_out`), claude(`ok`), dead.notified)
    expect(back).toEqual({ signOuts: [], notified: [] })
    expect(detectAgentSignOuts(claude(`ok`), claude(`signed_out`), back.notified).signOuts).toHaveLength(1)
  })

  it(`never fires for a login it never saw healthy`, () => {
    // First report ever, already dead.
    expect(detectAgentSignOuts(null, claude(`signed_out`), []).signOuts).toEqual([])
    // An agent installed but never signed in.
    const codex = (health: string) =>
      ({ codex: { signedIn: false, health } }) as unknown as DeviceAgentAccounts
    expect(detectAgentSignOuts(codex(`unknown`), codex(`signed_out`), []).signOuts).toEqual([])
  })

  it(`watches an agent row without profiles`, () => {
    const codex = (health: string) =>
      ({ codex: { signedIn: true, email: `c@acme.dev`, health } }) as unknown as DeviceAgentAccounts
    const { signOuts, notified } = detectAgentSignOuts(codex(`ok`), codex(`needs_relogin`), [])
    expect(signOuts.map((s) => s.key)).toEqual([`codex`])
    expect(notified).toEqual([`codex`])
  })

  it(`keeps the email the last report named`, () => {
    const gone = {
      claude: { signedIn: false, profiles: [{ id: `work`, signedIn: false, health: `signed_out` }] },
    } as unknown as DeviceAgentAccounts
    const [signOut] = detectAgentSignOuts(claude(`ok`), gone, []).signOuts
    expect(agentSignOutTitle(signOut!, `mint`)).toBe(`Claude Code account danny@acme.dev is signed out on mint`)
  })
})

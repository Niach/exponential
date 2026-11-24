import { describe, expect, it } from "vitest"
import { agentVoicePatchSchema, agentWakeClash, mergeAgentVoice, reconcileAgentAccount } from "./agents"
import type { DeviceAgentAccounts } from "@/db/schema"

// S-3: an agent's login is re-checked against its device on every save, so
// the definition and its conversation sessions never name a profile the
// machine does not have (Big Brother ran on one login while its definition
// named another that only existed on a different device).
describe(`reconcileAgentAccount`, () => {
  const accounts: DeviceAgentAccounts = {
    claude: {
      signedIn: true,
      profiles: [
        { id: `work`, signedIn: true },
        { id: `home`, signedIn: true, active: true },
      ],
    },
  }

  it(`keeps a profile the device has`, () => {
    expect(reconcileAgentAccount(`work`, `claude`, accounts)).toBe(`work`)
    expect(reconcileAgentAccount(`work`, null, accounts)).toBe(`work`)
  })

  it(`falls back to the device's active profile for a missing one`, () => {
    expect(reconcileAgentAccount(`gone`, `claude`, accounts)).toBe(`home`)
  })

  it(`falls back to none when the device has no active profile`, () => {
    const inactive: DeviceAgentAccounts = {
      claude: { signedIn: true, profiles: [{ id: `work`, signedIn: true }] },
    }
    expect(reconcileAgentAccount(`gone`, `claude`, inactive)).toBeNull()
  })

  it(`keeps the name when the device reported nothing to check against`, () => {
    expect(reconcileAgentAccount(`gone`, `codex`, accounts)).toBe(`gone`)
    expect(reconcileAgentAccount(`gone`, `claude`, null)).toBe(`gone`)
    expect(reconcileAgentAccount(null, `claude`, accounts)).toBeNull()
  })
})

// #EXP-1847 (agent-settings.json `voice`): `agents.update {voice}` merges a
// patch into the stored voice; null (or empty) clears a key.
describe(`mergeAgentVoice`, () => {
  it(`sets, keeps and clears keys`, () => {
    const stored = { language: `de` as const, clipId: `c-1`, clipName: `me.wav`, wakePhrase: `Hey Sloppy` }
    expect(mergeAgentVoice(stored, { language: `en`, wakePhrase: null })).toEqual({
      language: `en`,
      clipId: `c-1`,
      clipName: `me.wav`,
    })
    expect(mergeAgentVoice({}, { transcript: `` })).toEqual({})
  })
  it(`refuses a wake phrase with too few letters and an unknown language`, () => {
    expect(agentVoicePatchSchema.safeParse({ wakePhrase: `Hey` }).success).toBe(false)
    expect(agentVoicePatchSchema.safeParse({ wakePhrase: `Hey Sloppy` }).success).toBe(true)
    expect(agentVoicePatchSchema.safeParse({ language: `fr` }).success).toBe(false)
    expect(agentVoicePatchSchema.safeParse({ clipId: `x` }).success).toBe(false)
  })
})

// #EXP-1859 (agent-settings.json `voice.wake`, OWNERS' PHRASE): a team
// agent's changed phrase may not sound like another team agent's.
describe(`agentWakeClash`, () => {
  const ops = { id: `a-ops`, role: `agent`, name: `Ops`, voice: {} }
  const tri = { id: `a-tri`, role: `agent`, name: `Triage`, voice: { wakePhrase: `Hey Triage` } }
  const bro = { id: `a-bro`, role: `assistant`, name: `Bro`, voice: {} }
  const team = [ops, tri, bro]

  it(`refuses a phrase that sounds like another team agent's, naming it`, () => {
    expect(agentWakeClash(tri, { voice: { wakePhrase: `Hey Oops` } }, team)).toBe(`Sounds like Ops's wake phrase`)
  })
  it(`checks a cleared phrase as Hey {name}`, () => {
    const oops = { id: `a-oops`, role: `agent`, name: `Oops`, voice: { wakePhrase: `Hey Studio` } }
    expect(agentWakeClash(oops, { voice: {} }, [...team, oops])).toBe(`Sounds like Ops's wake phrase`)
  })
  it(`lets an unchanged phrase, an Assistant and a distinct phrase through`, () => {
    expect(agentWakeClash(ops, { voice: {} }, team)).toBeNull()
    expect(agentWakeClash(bro, { voice: { wakePhrase: `Hey Opps` } }, team)).toBeNull()
    expect(agentWakeClash(ops, { voice: { wakePhrase: `Hey Bros` } }, team)).toBeNull()
    expect(agentWakeClash(ops, { voice: { wakePhrase: `Hey Studio` } }, team)).toBeNull()
  })
})

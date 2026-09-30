import { describe, expect, it } from "vitest"
import { apiKeyKindOf, isAgentApiKeySession } from "@/lib/auth/api-key-kind"
import { createFakeDb } from "@/lib/mcp-oauth/test-db"

// EXP-1140: the launcher's hidden agent key is told apart from a person's by
// the `kind` the mint stamped into the plugin's JSON `metadata`. Untagged and
// unparsable metadata (every key minted before the tag) reads as a person's.
describe(`apiKeyKindOf`, () => {
  it(`reads the mint's kind and defaults to personal`, () => {
    expect(apiKeyKindOf(JSON.stringify({ kind: `agent` }))).toBe(`agent`)
    expect(apiKeyKindOf(JSON.stringify({ kind: `personal` }))).toBe(`personal`)
    expect(apiKeyKindOf(JSON.stringify({}))).toBe(`personal`)
    expect(apiKeyKindOf(null)).toBe(`personal`)
    expect(apiKeyKindOf(undefined)).toBe(`personal`)
    expect(apiKeyKindOf(`not json`)).toBe(`personal`)
    expect(apiKeyKindOf(`"agent"`)).toBe(`personal`)
  })
})

describe(`isAgentApiKeySession`, () => {
  const db = createFakeDb({
    apikeys: [
      { id: `key-agent`, referenceId: `actor`, metadata: JSON.stringify({ kind: `agent` }) },
      { id: `key-person`, referenceId: `actor`, metadata: JSON.stringify({ kind: `personal` }) },
    ],
  })
  const session = (id: string | undefined, userId = `actor`) =>
    ({ user: { id: userId }, session: id ? { id } : undefined }) as never

  it(`is true only for the caller's own agent-kind key row`, async () => {
    expect(await isAgentApiKeySession(db, session(`key-agent`))).toBe(true)
    expect(await isAgentApiKeySession(db, session(`key-person`))).toBe(false)
    // A cookie/bearer session id matches no key row.
    expect(await isAgentApiKeySession(db, session(`sess-1`))).toBe(false)
    expect(await isAgentApiKeySession(db, session(undefined))).toBe(false)
    // Another user's key id never classifies this user's request.
    expect(await isAgentApiKeySession(db, session(`key-agent`, `other`))).toBe(false)
  })
})

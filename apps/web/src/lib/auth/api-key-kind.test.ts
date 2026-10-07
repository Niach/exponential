import { describe, expect, it } from "vitest"
import {
  apiKeyKindOf,
  apiKeySessionKind,
  apiKeySessionMetadata,
  parseApiKeyMetadata,
  assertNotAgentApiKeySession,
  assertNotApiKeySession,
  isAgentApiKeySession,
  isApiKeySession,
} from "@/lib/auth/api-key-kind"
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

// FEED-76: the same metadata carries a scoped key's team/board selection.
describe(`parseApiKeyMetadata`, () => {
  it(`reads kind and scope, unscoped when absent`, () => {
    expect(parseApiKeyMetadata(null)).toEqual({ kind: `personal`, scope: null })
    expect(parseApiKeyMetadata(`not json`)).toEqual({ kind: `personal`, scope: null })
    expect(parseApiKeyMetadata(JSON.stringify({ kind: `agent` }))).toEqual({
      kind: `agent`,
      scope: null,
    })
    expect(
      parseApiKeyMetadata(
        JSON.stringify({
          kind: `personal`,
          scope: { allTeams: false, teamIds: [`t-1`], boardIds: [`b-1`, `b-2`] },
        })
      )
    ).toEqual({
      kind: `personal`,
      scope: { allTeams: false, teamIds: [`t-1`], boardIds: [`b-1`, `b-2`] },
    })
  })

  it(`an "everything" scope is no scope`, () => {
    expect(
      parseApiKeyMetadata(
        JSON.stringify({ scope: { allTeams: true, teamIds: [`t-1`], boardIds: [] } })
      ).scope
    ).toBeNull()
  })

  it(`a present but malformed scope fails closed`, () => {
    const closed = { allTeams: false, teamIds: [], boardIds: [] }
    expect(parseApiKeyMetadata(JSON.stringify({ scope: `x` })).scope).toEqual(closed)
    expect(
      parseApiKeyMetadata(JSON.stringify({ scope: { teamIds: `t-1`, boardIds: [] } })).scope
    ).toEqual(closed)
    expect(
      parseApiKeyMetadata(JSON.stringify({ scope: { teamIds: [`t-1`] } })).scope
    ).toEqual(closed)
    expect(
      parseApiKeyMetadata(JSON.stringify({ scope: { teamIds: [1], boardIds: [] } })).scope
    ).toEqual(closed)
  })
})

describe(`apiKeySessionMetadata`, () => {
  const db = createFakeDb({
    apikeys: [
      {
        id: `key-scoped`,
        referenceId: `actor`,
        metadata: JSON.stringify({
          kind: `personal`,
          scope: { allTeams: false, teamIds: [`t-1`], boardIds: [] },
        }),
      },
      { id: `key-plain`, referenceId: `actor`, metadata: null },
    ],
  })
  const session = (id: string | undefined, userId = `actor`) =>
    ({ user: { id: userId }, session: id ? { id } : undefined }) as never

  it(`returns the caller's own key row parsed, null for a real session`, async () => {
    expect(await apiKeySessionMetadata(db, session(`key-scoped`))).toEqual({
      kind: `personal`,
      scope: { allTeams: false, teamIds: [`t-1`], boardIds: [] },
    })
    expect(await apiKeySessionMetadata(db, session(`key-plain`))).toEqual({
      kind: `personal`,
      scope: null,
    })
    expect(await apiKeySessionMetadata(db, session(`sess-1`))).toBeNull()
    expect(await apiKeySessionMetadata(db, session(undefined))).toBeNull()
    expect(await apiKeySessionMetadata(db, session(`key-scoped`, `other`))).toBeNull()
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

describe(`apiKeySessionKind / isApiKeySession / the assert helpers`, () => {
  const db = createFakeDb({
    apikeys: [
      { id: `key-agent`, referenceId: `actor`, metadata: JSON.stringify({ kind: `agent` }) },
      { id: `key-person`, referenceId: `actor`, metadata: null },
    ],
  })
  const session = (id: string | undefined) =>
    ({ user: { id: `actor` }, session: id ? { id } : undefined }) as never

  it(`tells a key session (either kind) from a real one`, async () => {
    expect(await apiKeySessionKind(db, session(`key-agent`))).toBe(`agent`)
    expect(await apiKeySessionKind(db, session(`key-person`))).toBe(`personal`)
    expect(await apiKeySessionKind(db, session(`sess-1`))).toBeNull()
    expect(await isApiKeySession(db, session(`key-person`))).toBe(true)
    expect(await isApiKeySession(db, session(`sess-1`))).toBe(false)
  })

  it(`assertNotApiKeySession refuses ANY key, assertNotAgentApiKeySession only the agent's`, async () => {
    await expect(assertNotApiKeySession(db, session(`key-person`))).rejects.toMatchObject({
      code: `UNAUTHORIZED`,
    })
    await expect(assertNotApiKeySession(db, session(`key-agent`))).rejects.toMatchObject({
      code: `UNAUTHORIZED`,
    })
    await expect(assertNotApiKeySession(db, session(`sess-1`))).resolves.toBeUndefined()

    await expect(
      assertNotAgentApiKeySession(db, session(`key-agent`), `nope`)
    ).rejects.toMatchObject({ code: `FORBIDDEN`, message: `nope` })
    await expect(assertNotAgentApiKeySession(db, session(`key-person`), `nope`)).resolves.toBeUndefined()
    await expect(assertNotAgentApiKeySession(db, session(`sess-1`), `nope`)).resolves.toBeUndefined()
  })
})

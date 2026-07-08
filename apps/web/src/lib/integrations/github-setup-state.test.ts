import { beforeEach, describe, expect, it } from "vitest"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
  mintGithubClaimTicket,
  mintGithubSetupState,
  readGithubClaimTicket,
} from "@/lib/integrations/github-setup-state"

describe(`github setup state token`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
  })

  it(`round-trips for the minting user`, () => {
    const state = mintGithubSetupState(`user-1`)
    expect(state).toBeTruthy()
    expect(consumeGithubSetupState(state!, `user-1`)).toEqual({
      userId: `user-1`,
      workspaceId: null,
    })
  })

  it(`refuses a token minted for a different user`, () => {
    const state = mintGithubSetupState(`victim`)
    expect(consumeGithubSetupState(state!, `attacker`)).toBeNull()
  })

  it(`refuses when the callback carries no session`, () => {
    const state = mintGithubSetupState(`user-1`)
    expect(consumeGithubSetupState(state!, null)).toBeNull()
  })

  it(`refuses a missing or unsigned state`, () => {
    expect(consumeGithubSetupState(null, `user-1`)).toBeNull()
    expect(consumeGithubSetupState(`dialog`, `user-1`)).toBeNull()
  })

  it(`refuses a payload swapped under a valid signature`, () => {
    const state = mintGithubSetupState(`user-1`)!
    const sig = state.slice(state.lastIndexOf(`.`) + 1)
    const forgedBody = Buffer.from(
      JSON.stringify({ u: `attacker`, n: `nonce`, exp: Date.now() + 60_000 })
    ).toString(`base64url`)
    expect(consumeGithubSetupState(`${forgedBody}.${sig}`, `attacker`)).toBeNull()
  })

  it(`refuses an expired token`, () => {
    const twoHoursAgo = Date.now() - 2 * 60 * 60 * 1000
    const state = mintGithubSetupState(`user-1`, undefined, twoHoursAgo)
    expect(consumeGithubSetupState(state!, `user-1`)).toBeNull()
  })

  it(`is single-use`, () => {
    const state = mintGithubSetupState(`user-1`)!
    expect(consumeGithubSetupState(state, `user-1`)).toEqual({
      userId: `user-1`,
      workspaceId: null,
    })
    expect(consumeGithubSetupState(state, `user-1`)).toBeNull()
  })

  it(`mints nothing without a secret`, () => {
    delete process.env.BETTER_AUTH_SECRET
    expect(mintGithubSetupState(`user-1`)).toBeUndefined()
  })

  it(`carries the dialog flag readably without verification`, () => {
    const dialogState = mintGithubSetupState(`user-1`, { dialog: true })!
    const plainState = mintGithubSetupState(`user-1`)!
    expect(githubSetupStateWantsDialog(dialogState)).toBe(true)
    expect(githubSetupStateWantsDialog(plainState)).toBe(false)
    expect(githubSetupStateWantsDialog(`dialog`)).toBe(false)
    expect(githubSetupStateWantsDialog(null)).toBe(false)
  })

  it(`carries the mobile flag readably without verification`, () => {
    const mobileState = mintGithubSetupState(`user-1`, {
      dialog: true,
      mobile: true,
    })!
    const dialogState = mintGithubSetupState(`user-1`, { dialog: true })!
    const plainState = mintGithubSetupState(`user-1`)!
    expect(githubSetupStateWantsMobile(mobileState)).toBe(true)
    expect(githubSetupStateWantsMobile(dialogState)).toBe(false)
    expect(githubSetupStateWantsMobile(plainState)).toBe(false)
    expect(githubSetupStateWantsMobile(`mobile`)).toBe(false)
    expect(githubSetupStateWantsMobile(null)).toBe(false)
    // The marker never weakens the flags it rides alongside.
    expect(githubSetupStateWantsDialog(mobileState)).toBe(true)
  })

  it(`reads the mobile flag even from an expired token`, () => {
    const twoHoursAgo = Date.now() - 2 * 60 * 60 * 1000
    const state = mintGithubSetupState(`user-1`, { mobile: true }, twoHoursAgo)!
    expect(consumeGithubSetupState(state, `user-1`)).toBeNull()
    expect(githubSetupStateWantsMobile(state)).toBe(true)
  })

  it(`still round-trips consumption with the mobile flag set`, () => {
    const state = mintGithubSetupState(`user-1`, {
      dialog: true,
      mobile: true,
    })!
    expect(consumeGithubSetupState(state, `user-1`)).toEqual({
      userId: `user-1`,
      workspaceId: null,
    })
    // Single-use holds regardless of markers.
    expect(consumeGithubSetupState(state, `user-1`)).toBeNull()
  })

  it(`round-trips the target workspace id`, () => {
    const state = mintGithubSetupState(`user-1`, { workspaceId: `ws-9` })!
    expect(consumeGithubSetupState(state, `user-1`)).toEqual({
      userId: `user-1`,
      workspaceId: `ws-9`,
    })
  })

  it(`pins the token purpose: an install state never consumes as an OAuth state (and vice versa)`, () => {
    const installState = mintGithubSetupState(`user-1`, { workspaceId: `ws-9` })!
    const oauthState = mintGithubSetupState(`user-1`, {
      workspaceId: `ws-9`,
      oauth: true,
    })!
    // Cross-purpose replays refuse without burning the nonce…
    expect(
      consumeGithubSetupState(installState, `user-1`, { expectOauth: true })
    ).toBeNull()
    expect(consumeGithubSetupState(oauthState, `user-1`)).toBeNull()
    // …so the right callback can still consume each one.
    expect(consumeGithubSetupState(installState, `user-1`)).toEqual({
      userId: `user-1`,
      workspaceId: `ws-9`,
    })
    expect(
      consumeGithubSetupState(oauthState, `user-1`, { expectOauth: true })
    ).toEqual({ userId: `user-1`, workspaceId: `ws-9` })
  })
})

describe(`github claim ticket`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
  })

  const payload = { u: `user-1`, w: `ws-9`, ids: [1, 2, 3] }

  it(`round-trips for the minting user`, () => {
    const ticket = mintGithubClaimTicket(payload)!
    const read = readGithubClaimTicket(ticket, `user-1`)
    expect(read).toMatchObject(payload)
  })

  it(`is NOT single-use (linking is idempotent) but stays user-bound`, () => {
    const ticket = mintGithubClaimTicket(payload)!
    expect(readGithubClaimTicket(ticket, `user-1`)).toMatchObject(payload)
    expect(readGithubClaimTicket(ticket, `user-1`)).toMatchObject(payload)
    expect(readGithubClaimTicket(ticket, `attacker`)).toBeNull()
    expect(readGithubClaimTicket(ticket, null)).toBeNull()
  })

  it(`refuses a tampered installation-id set`, () => {
    const ticket = mintGithubClaimTicket(payload)!
    const sig = ticket.slice(ticket.lastIndexOf(`.`) + 1)
    const forged = Buffer.from(
      JSON.stringify({ ...payload, ids: [999], exp: Date.now() + 60_000 })
    ).toString(`base64url`)
    expect(readGithubClaimTicket(`${forged}.${sig}`, `user-1`)).toBeNull()
  })

  it(`refuses an expired ticket`, () => {
    const past = Date.now() - 60 * 60 * 1000
    const ticket = mintGithubClaimTicket(payload, past)!
    expect(readGithubClaimTicket(ticket, `user-1`)).toBeNull()
  })

  it(`refuses malformed payloads under a valid signature`, () => {
    const ticket = mintGithubClaimTicket(payload)!
    expect(readGithubClaimTicket(ticket.replaceAll(`.`, `,`), `user-1`)).toBeNull()
    expect(readGithubClaimTicket(``, `user-1`)).toBeNull()
    expect(readGithubClaimTicket(null, `user-1`)).toBeNull()
  })

  it(`carries the mobile/dialog markers`, () => {
    const ticket = mintGithubClaimTicket({ ...payload, m: true, d: true })!
    expect(readGithubClaimTicket(ticket, `user-1`)).toMatchObject({
      m: true,
      d: true,
    })
  })
})

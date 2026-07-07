import { beforeEach, describe, expect, it } from "vitest"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
  mintGithubSetupState,
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
    })
    // Single-use holds regardless of markers.
    expect(consumeGithubSetupState(state, `user-1`)).toBeNull()
  })
})

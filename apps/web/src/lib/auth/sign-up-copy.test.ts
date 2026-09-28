import { describe, expect, it } from "vitest"
import {
  ASK_NAME_HEADER,
  NAME_REQUIRED_CODE,
  SIGN_UP_NAME_COPY,
} from "./sign-up-copy"

// EXP-1026: the natives (iOS, Android, desktop) mirror these literals byte for
// byte and build against the header + code. Changing one here means changing
// all four clients in the same release.
describe(`sign-up name step copy`, () => {
  it(`is locked`, () => {
    expect(SIGN_UP_NAME_COPY).toEqual({
      title: `What should we call you?`,
      body: `This is how your teammates see you.`,
      fieldLabel: `Name`,
      placeholder: `Your name`,
      button: `Create account`,
      emptyNameError: `Enter your name to continue.`,
    })
  })

  it(`pins the wire contract`, () => {
    expect(ASK_NAME_HEADER).toBe(`X-Exp-Ask-Name`)
    expect(NAME_REQUIRED_CODE).toBe(`NAME_REQUIRED`)
  })
})

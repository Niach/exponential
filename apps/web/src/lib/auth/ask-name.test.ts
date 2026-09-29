// @vitest-environment node
import { beforeEach, describe, expect, it } from "vitest"
import { betterAuth } from "better-auth"
import { memoryAdapter } from "better-auth/adapters/memory"
import { emailOTP } from "better-auth/plugins"
import {
  askNameBeforeHook,
  fallbackUserName,
  hashOtp,
  nameRequiredForOtpSignIn,
  normalizeSignUpName,
  SIGN_UP_NAME_MAX,
} from "./ask-name"

// EXP-1026: the gate runs against a REAL Better Auth instance (memory
// adapter, the email-otp plugin configured as production configures it), so
// "the code is still valid after NAME_REQUIRED" is proven by the plugin
// itself accepting the resubmit.

const BASE = `http://localhost:3000`
const ALLOWED_ATTEMPTS = 5

function makeAuth() {
  const db: Record<string, Record<string, unknown>[]> = {
    user: [],
    session: [],
    account: [],
    verification: [],
  }
  const sent = new Map<string, string>()
  const auth = betterAuth({
    baseURL: BASE,
    secret: `test-secret-test-secret-test-secret-0123`,
    database: memoryAdapter(db),
    rateLimit: { enabled: false },
    logger: { disabled: true },
    databaseHooks: {
      user: {
        create: {
          before: async (user) => {
            const name = fallbackUserName(user)
            if (name === null) return
            return { data: { ...user, name } }
          },
        },
      },
    },
    hooks: {
      before: askNameBeforeHook({
        signUpDisabled: false,
        allowedAttempts: ALLOWED_ATTEMPTS,
      }),
    },
    plugins: [
      emailOTP({
        otpLength: 6,
        expiresIn: 600,
        allowedAttempts: ALLOWED_ATTEMPTS,
        storeOTP: `hashed`,
        sendVerificationOTP: async ({ email, otp }) => {
          sent.set(email, otp)
        },
      }),
    ],
  })
  return { auth, db, sent }
}

type Harness = ReturnType<typeof makeAuth>

async function post(
  h: Harness,
  path: string,
  body: Record<string, unknown>,
  headers: Record<string, string> = {}
) {
  const response = await h.auth.handler(
    new Request(`${BASE}/api/auth${path}`, {
      method: `POST`,
      headers: { "content-type": `application/json`, origin: BASE, ...headers },
      body: JSON.stringify(body),
    })
  )
  const text = await response.text()
  return { status: response.status, json: text ? JSON.parse(text) : null }
}

async function requestCode(h: Harness, email: string): Promise<string> {
  const res = await post(h, `/email-otp/send-verification-otp`, {
    email,
    type: `sign-in`,
  })
  expect(res.status).toBe(200)
  return h.sent.get(email)!
}

const ASK = { "X-Exp-Ask-Name": `1` }

describe(`email-code sign-up name step (EXP-1026)`, () => {
  let h: Harness
  beforeEach(() => {
    h = makeAuth()
  })

  it(`a new address without a name answers NAME_REQUIRED and keeps the code`, async () => {
    const otp = await requestCode(h, `new@example.com`)
    const first = await post(
      h,
      `/sign-in/email-otp`,
      { email: `new@example.com`, otp },
      ASK
    )
    expect(first.status).toBe(400)
    expect(first.json).toMatchObject({ code: `NAME_REQUIRED` })
    expect(typeof first.json.message).toBe(`string`)
    expect(h.db.user).toHaveLength(0)

    // A blank name is no name.
    const blank = await post(
      h,
      `/sign-in/email-otp`,
      { email: `new@example.com`, otp, name: `   ` },
      ASK
    )
    expect(blank.json).toMatchObject({ code: `NAME_REQUIRED` })

    // The same code, now with a name: created under it.
    const second = await post(
      h,
      `/sign-in/email-otp`,
      { email: `new@example.com`, otp, name: `Ada Lovelace` },
      ASK
    )
    expect(second.status).toBe(200)
    expect(second.json.user).toMatchObject({
      email: `new@example.com`,
      name: `Ada Lovelace`,
    })
    expect(typeof second.json.token).toBe(`string`)
  })

  it(`an invalid code falls through to the plugin and counts as an attempt`, async () => {
    const otp = await requestCode(h, `new@example.com`)
    const wrong = otp === `000000` ? `111111` : `000000`
    const res = await post(
      h,
      `/sign-in/email-otp`,
      { email: `new@example.com`, otp: wrong },
      ASK
    )
    expect(res.status).toBe(400)
    expect(res.json.code).toBe(`INVALID_OTP`)
    const row = h.db.verification[0] as { value: string }
    expect(row.value).toBe(`${hashOtp(otp)}:1`)
  })

  it(`an existing account signs straight in, never asked`, async () => {
    const first = await requestCode(h, `old@example.com`)
    await post(h, `/sign-in/email-otp`, {
      email: `old@example.com`,
      otp: first,
      name: `Old Timer`,
    })
    const otp = await requestCode(h, `old@example.com`)
    const res = await post(
      h,
      `/sign-in/email-otp`,
      { email: `old@example.com`, otp },
      ASK
    )
    expect(res.status).toBe(200)
    expect(res.json.user.name).toBe(`Old Timer`)
  })

  it(`without the header a new address keeps the mailbox fallback`, async () => {
    const otp = await requestCode(h, `grace.hopper@example.com`)
    const res = await post(h, `/sign-in/email-otp`, {
      email: `grace.hopper@example.com`,
      otp,
    })
    expect(res.status).toBe(200)
    expect(res.json.user.name).toBe(`grace.hopper`)
  })

  it(`stores the name trimmed and capped`, async () => {
    const otp = await requestCode(h, `new@example.com`)
    const res = await post(
      h,
      `/sign-in/email-otp`,
      { email: `new@example.com`, otp, name: `  ${`A`.repeat(150)}  ` },
      ASK
    )
    expect(res.status).toBe(200)
    expect(res.json.user.name).toBe(`A`.repeat(SIGN_UP_NAME_MAX))

    const other = await requestCode(h, `ada@example.com`)
    const trimmed = await post(h, `/sign-in/email-otp`, {
      email: `ada@example.com`,
      otp: other,
      name: `  Ada Lovelace \n`,
    })
    expect(trimmed.json.user.name).toBe(`Ada Lovelace`)
  })

  it(`without the header an all-whitespace name keeps the mailbox fallback`, async () => {
    const otp = await requestCode(h, `grace.hopper@example.com`)
    const res = await post(h, `/sign-in/email-otp`, {
      email: `grace.hopper@example.com`,
      otp,
      name: `   `,
    })
    expect(res.status).toBe(200)
    expect(res.json.user.name).toBe(`grace.hopper`)
  })
})

describe(`nameRequiredForOtpSignIn`, () => {
  const live = (otp: string, attempts = 0) => ({
    value: `${hashOtp(otp)}:${attempts}`,
    expiresAt: new Date(Date.now() + 60_000),
  })
  const input = (over: Record<string, unknown> = {}) => ({
    headers: new Headers({ "x-exp-ask-name": `1` }),
    body: { email: `New@Example.com`, otp: `123456` },
    signUpDisabled: false,
    allowedAttempts: 5,
    ...over,
  })
  const adapter = (
    stored: ReturnType<typeof live> | null,
    user: unknown = null
  ) => ({
    findUserByEmail: async () => user,
    findVerificationValue: async () => stored,
  })

  it(`asks only for a valid live code of an unknown address`, async () => {
    expect(await nameRequiredForOtpSignIn(input(), adapter(live(`123456`)))).toBe(true)
    expect(
      await nameRequiredForOtpSignIn(input(), adapter(live(`123456`), { id: `u` }))
    ).toBe(false)
    expect(await nameRequiredForOtpSignIn(input(), adapter(live(`654321`)))).toBe(false)
    expect(await nameRequiredForOtpSignIn(input(), adapter(live(`123456`, 5)))).toBe(false)
    expect(await nameRequiredForOtpSignIn(input(), adapter(null))).toBe(false)
    expect(
      await nameRequiredForOtpSignIn(
        input(),
        adapter({ ...live(`123456`), expiresAt: new Date(Date.now() - 1) })
      )
    ).toBe(false)
  })

  it(`never asks without the header, with a name, or when sign-up is closed`, async () => {
    const ok = adapter(live(`123456`))
    expect(await nameRequiredForOtpSignIn(input({ headers: new Headers() }), ok)).toBe(false)
    expect(
      await nameRequiredForOtpSignIn(
        input({ body: { email: `a@b.c`, otp: `123456`, name: `Ada` } }),
        ok
      )
    ).toBe(false)
    expect(await nameRequiredForOtpSignIn(input({ signUpDisabled: true }), ok)).toBe(false)
  })

  it(`normalizeSignUpName trims, caps by code point and empties a blank one`, () => {
    expect(normalizeSignUpName(`  Ada  `)).toBe(`Ada`)
    expect(normalizeSignUpName(` \t `)).toBe(``)
    expect(normalizeSignUpName(`x`.repeat(99) + `  y`)).toBe(`x`.repeat(99))
    const emoji = `\u{1F600}`.repeat(SIGN_UP_NAME_MAX + 5)
    expect(Array.from(normalizeSignUpName(emoji))).toHaveLength(SIGN_UP_NAME_MAX)
  })

  it(`fallbackUserName keeps a real name and defaults a blank one`, () => {
    expect(fallbackUserName({ name: `Ada`, email: `a@b.c` })).toBeNull()
    expect(fallbackUserName({ name: ` `, email: `ada@b.c` })).toBe(`ada`)
    expect(fallbackUserName({ name: ``, email: `` })).toBe(``)
  })
})

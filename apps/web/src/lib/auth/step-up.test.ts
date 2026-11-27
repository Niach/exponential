// @vitest-environment node
import { beforeEach, describe, expect, it } from "vitest"
import { betterAuth } from "better-auth"
import { memoryAdapter } from "better-auth/adapters/memory"
import { passkey } from "@better-auth/passkey"
import fixture from "@exp/domain-contract/fixtures/step-up.json"
import { stepUpPlugin, type StepUpInstance } from "./step-up"

// #EXP-2143 follow-up: the step-up plugin against a REAL Better Auth instance
// (memory adapter, the passkey plugin's fresh-session gate, the cookie cache
// on as production has it). The resumed action is the gate itself: after the
// proof, `/passkey/generate-register-options` answers 200 on the SAME session.

const BASE = `http://localhost:3000`
const DAY_MS = 24 * 60 * 60 * 1000

function makeAuth(instance: StepUpInstance = { passkey: true, email: true, password: true }) {
  const db: Record<string, Record<string, unknown>[]> = {
    user: [],
    session: [],
    account: [],
    verification: [],
    passkey: [],
  }
  const codes: string[] = []
  const auth = betterAuth({
    baseURL: BASE,
    secret: `test-secret-test-secret-test-secret-0123`,
    database: memoryAdapter(db),
    rateLimit: { enabled: false },
    logger: { disabled: true },
    emailAndPassword: { enabled: true },
    session: { cookieCache: { enabled: true, maxAge: 60 * 5 } },
    plugins: [
      passkey({ rpID: `localhost`, rpName: `Exponential`, origin: BASE }),
      stepUpPlugin({
        instance: () => instance,
        passkey: () => ({ rpID: `localhost`, origin: BASE }),
        sendCode: async ({ code }) => {
          codes.push(code)
        },
      }),
    ],
  })
  return { auth, db, codes }
}

type Harness = ReturnType<typeof makeAuth>

/** A cookie jar: every response's Set-Cookie replaces the pair it names. */
class Jar {
  pairs = new Map<string, string>()
  take(response: Response) {
    for (const line of response.headers.getSetCookie()) {
      const [pair] = line.split(`;`)
      const at = pair!.indexOf(`=`)
      const name = pair!.slice(0, at).trim()
      const value = pair!.slice(at + 1).trim()
      if (value === `` || /max-age=0/i.test(line)) this.pairs.delete(name)
      else this.pairs.set(name, value)
    }
  }
  header() {
    return [...this.pairs].map(([k, v]) => `${k}=${v}`).join(`; `)
  }
}

async function call(
  harness: Harness,
  jar: Jar,
  method: `GET` | `POST`,
  path: string,
  body?: Record<string, unknown>
) {
  const response = await harness.auth.handler(
    new Request(`${BASE}/api/auth${path}`, {
      method,
      headers: {
        origin: BASE,
        cookie: jar.header(),
        ...(body ? { "content-type": `application/json` } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    })
  )
  jar.take(response)
  const text = await response.text()
  let json: Record<string, unknown> = {}
  try {
    json = JSON.parse(text) as Record<string, unknown>
  } catch {
    json = { raw: text }
  }
  return { status: response.status, json }
}

/** Signed up a day and a bit ago: the session row is stale and the cookie
 * cache that still claimed otherwise is gone (it lives 5 minutes). */
async function staleSession(harness: Harness, email = `person@example.com`) {
  const jar = new Jar()
  const up = await call(harness, jar, `POST`, `/sign-up/email`, {
    email,
    password: `password-1234`,
    name: `Person`,
  })
  expect(up.status).toBe(200)
  const userId = (up.json.user as { id: string }).id
  const row = harness.db.session!.find((s) => s.userId === userId)!
  row.createdAt = new Date(Date.now() - DAY_MS - 60_000)
  for (const name of [...jar.pairs.keys()]) if (name.includes(`session_data`)) jar.pairs.delete(name)
  // The browser reads its session once more: the cookie cache now carries
  // the STALE createdAt, so a proof that only wrote the row would still meet
  // the gate's refusal for the cache's 5 minutes.
  await call(harness, jar, `GET`, `/get-session`)
  expect([...jar.pairs.keys()].some((name) => name.includes(`session_data`))).toBe(true)
  return { jar, userId, sessionId: row.id as string, token: row.token as string }
}

let harness: Harness

beforeEach(() => {
  harness = makeAuth()
})

describe(`step-up (fixtures/step-up.json)`, () => {
  it(`the fixture's gates are the passkey registration pair, and they refuse a stale session`, async () => {
    expect(fixture.gates.map((g) => g.path)).toEqual([
      `/passkey/generate-register-options`,
      `/passkey/verify-registration`,
    ])
    const { jar } = await staleSession(harness)
    const refused = await call(harness, jar, `GET`, `/passkey/generate-register-options`)
    expect(refused.status).toBe(403)
    expect(refused.json.code).toBe(`SESSION_NOT_FRESH`)
  })

  it(`methods: the account's ways in, in the fixture's order`, async () => {
    const { jar, userId } = await staleSession(harness)
    let methods = await call(harness, jar, `GET`, `/step-up/methods`)
    expect(methods.json).toEqual({ methods: [`email`, `password`], email: `person@example.com` })
    harness.db.passkey!.push({ id: `pk-1`, userId, credentialID: `cred-1`, publicKey: ``, counter: 0, createdAt: new Date() })
    methods = await call(harness, jar, `GET`, `/step-up/methods`)
    expect(methods.json.methods).toEqual([`passkey`, `email`, `password`])
  })

  it(`email code: a wrong code is refused, the right one makes THIS session fresh and the add resumes`, async () => {
    const { jar, sessionId, token } = await staleSession(harness)
    expect((await call(harness, jar, `POST`, `/step-up/email/send`, {})).json).toEqual({ sent: true })
    expect(harness.codes).toHaveLength(1)
    const code = harness.codes[0]!
    const wrong = await call(harness, jar, `POST`, `/step-up/email/verify`, {
      code: code === `000000` ? `111111` : `000000`,
    })
    expect(wrong.json.code).toBe(`INVALID_OTP`)
    expect((await call(harness, jar, `GET`, `/passkey/generate-register-options`)).status).toBe(403)

    const right = await call(harness, jar, `POST`, `/step-up/email/verify`, { code })
    expect(right.status).toBe(200)
    expect(right.json).toEqual({ fresh: true })
    // The same session (no new one, no sign-out), now fresh.
    expect(harness.db.session).toHaveLength(1)
    expect(harness.db.session![0]!.id).toBe(sessionId)
    expect(harness.db.session![0]!.token).toBe(token)
    expect(Date.now() - new Date(harness.db.session![0]!.createdAt as Date).getTime()).toBeLessThan(60_000)
    // The resumed action: the gate opens on the jar the proof answered.
    const resumed = await call(harness, jar, `GET`, `/passkey/generate-register-options`)
    expect(resumed.status).toBe(200)
    expect(resumed.json.challenge).toBeTypeOf(`string`)
    // The code is spent.
    expect((await call(harness, jar, `POST`, `/step-up/email/verify`, { code })).json.code).toBe(`OTP_EXPIRED`)
  })

  it(`email code: five wrong tries burn it`, async () => {
    const { jar } = await staleSession(harness)
    await call(harness, jar, `POST`, `/step-up/email/send`, {})
    const code = harness.codes[0]!
    const wrongCode = code === `000000` ? `111111` : `000000`
    for (let i = 0; i < 5; i++) {
      expect((await call(harness, jar, `POST`, `/step-up/email/verify`, { code: wrongCode })).json.code).toBe(`INVALID_OTP`)
    }
    expect((await call(harness, jar, `POST`, `/step-up/email/verify`, { code })).json.code).toBe(`TOO_MANY_ATTEMPTS`)
    expect((await call(harness, jar, `GET`, `/passkey/generate-register-options`)).status).toBe(403)
  })

  it(`email code: another session's code does not prove this one`, async () => {
    const first = await staleSession(harness)
    await call(harness, first.jar, `POST`, `/step-up/email/send`, {})
    const code = harness.codes[0]!
    const other = new Jar()
    await call(harness, other, `POST`, `/sign-in/email`, { email: `person@example.com`, password: `password-1234` })
    const second = harness.db.session!.find((s) => s.id !== first.sessionId)!
    second.createdAt = new Date(Date.now() - 2 * DAY_MS)
    for (const name of [...other.pairs.keys()]) if (name.includes(`session_data`)) other.pairs.delete(name)
    expect((await call(harness, other, `POST`, `/step-up/email/verify`, { code })).json.code).toBe(`OTP_EXPIRED`)
  })

  it(`password: wrong is refused, right makes the session fresh`, async () => {
    const { jar } = await staleSession(harness)
    const wrong = await call(harness, jar, `POST`, `/step-up/password`, { password: `nope-nope-nope` })
    expect(wrong.status).toBe(401)
    expect(wrong.json.code).toBe(`INVALID_PASSWORD`)
    const right = await call(harness, jar, `POST`, `/step-up/password`, { password: `password-1234` })
    expect(right.json).toEqual({ fresh: true })
    expect((await call(harness, jar, `GET`, `/passkey/generate-register-options`)).status).toBe(200)
  })

  it(`a method the account lacks is refused (no password, no mail)`, async () => {
    harness = makeAuth({ passkey: true, email: false, password: false })
    const { jar } = await staleSession(harness)
    expect((await call(harness, jar, `GET`, `/step-up/methods`)).json.methods).toEqual([])
    expect((await call(harness, jar, `POST`, `/step-up/email/send`, {})).json.code).toBe(`METHOD_NOT_AVAILABLE`)
    expect(harness.codes).toHaveLength(0)
    expect((await call(harness, jar, `POST`, `/step-up/password`, { password: `password-1234` })).json.code).toBe(
      `METHOD_NOT_AVAILABLE`
    )
  })

  it(`passkey: another account's passkey never proves this person`, async () => {
    const { jar, userId } = await staleSession(harness)
    harness.db.passkey!.push(
      { id: `pk-mine`, userId, credentialID: `cred-mine`, publicKey: ``, counter: 0, createdAt: new Date() },
      { id: `pk-theirs`, userId: `someone-else`, credentialID: `cred-theirs`, publicKey: ``, counter: 0, createdAt: new Date() }
    )
    const options = await call(harness, jar, `GET`, `/passkey/generate-authenticate-options`)
    expect(options.status).toBe(200)
    expect((options.json.allowCredentials as Array<{ id: string }>).map((c) => c.id)).toEqual([`cred-mine`])
    const theirs = await call(harness, jar, `POST`, `/step-up/passkey`, {
      response: { id: `cred-theirs`, rawId: `cred-theirs`, type: `public-key`, response: {} },
    })
    expect(theirs.status).toBe(401)
    expect(theirs.json.code).toBe(`PASSKEY_FAILED`)
    // A forged assertion for the right credential fails verification too.
    await call(harness, jar, `GET`, `/passkey/generate-authenticate-options`)
    const forged = await call(harness, jar, `POST`, `/step-up/passkey`, {
      response: { id: `cred-mine`, rawId: `cred-mine`, type: `public-key`, response: {} },
    })
    expect(forged.json.code).toBe(`PASSKEY_FAILED`)
    expect((await call(harness, jar, `GET`, `/passkey/generate-register-options`)).status).toBe(403)
  })

  it(`no session, no step-up`, async () => {
    const jar = new Jar()
    expect((await call(harness, jar, `GET`, `/step-up/methods`)).status).toBe(401)
    expect((await call(harness, jar, `POST`, `/step-up/password`, { password: `x` })).status).toBe(401)
  })
})

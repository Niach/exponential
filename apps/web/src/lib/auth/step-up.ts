// #EXP-2143 follow-up: "Confirm it's you" — the server half of the step-up
// sheet (packages/domain-contract/fixtures/step-up.json is the spec).
//
// Better Auth's fresh-session gate (`freshSessionMiddleware`, used by the
// passkey registration pair) refuses a session whose `createdAt` is older
// than `session.freshAge` (a day) with SESSION_NOT_FRESH. Better Auth offers
// no re-authentication of an existing session (`/update-session` only takes
// the additional fields), so this plugin does it: the person proves it is
// them with a way in the account already has (a passkey of THIS user, a code
// mailed to the account's own address, or the account's password), and the
// CURRENT session's `createdAt` moves to now, the one value the gate reads.
// The cookie cache is rewritten with it (a stale cached `createdAt` would
// keep the gate shut for its 5 minutes). No new session is ever minted: the
// native clients keep their bearer token, the web its cookie.
//
// API keys are refused in front of every path here (sign-in-methods.ts
// `isIdentityPath` covers `/step-up/`): a key must never turn itself fresh.
import { createHash, randomInt, timingSafeEqual } from "node:crypto"
import type { BetterAuthPlugin } from "better-auth"
import { APIError, createAuthEndpoint, sessionMiddleware } from "better-auth/api"
import { setSessionCookie } from "better-auth/cookies"
import { verifyAuthenticationResponse } from "@simplewebauthn/server"
import * as z from "zod"
import { stepUpMethods, type StepUpMethod } from "@exp/domain-contract/step-up"

export type StepUpInstance = {
  passkey: boolean
  email: boolean
  password: boolean
}

export type StepUpOptions = {
  /** What the instance offers, read per request (env-derived in production). */
  instance: () => StepUpInstance
  /** Mails the 6-digit code to the account's own address. */
  sendCode: (args: { email: string; code: string; userId: string }) => Promise<void>
  /** The passkey plugin's relying party: rpID and the accepted origins. */
  passkey?: () => { rpID: string; origin: string | string[] }
  /** The passkey plugin's challenge cookie (its default). */
  challengeCookie?: string
}

export const STEP_UP_CODE_TTL_MS = 10 * 60 * 1000
export const STEP_UP_CODE_ATTEMPTS = 5

/** Any endpoint's context (Better Auth's GenericEndpointContext). */
type Ctx = Parameters<typeof setSessionCookie>[0]

type SessionCtx = {
  session: { id: string; token: string; createdAt: Date | string }
  user: { id: string; email: string }
}

function hashCode(code: string): string {
  return createHash(`sha256`).update(code).digest(`hex`)
}

function sameHash(a: string, b: string): boolean {
  const left = Buffer.from(a, `hex`)
  const right = Buffer.from(b, `hex`)
  return left.length === right.length && timingSafeEqual(left, right)
}

/** The verification row of a session's mailed code. */
export function stepUpCodeIdentifier(sessionId: string): string {
  return `step-up-email:${sessionId}`
}

function fail(status: `BAD_REQUEST` | `UNAUTHORIZED` | `FORBIDDEN`, code: string, message: string): never {
  throw new APIError(status, { code, message })
}

export function stepUpPlugin(options: StepUpOptions): BetterAuthPlugin {
  const challengeCookie = options.challengeCookie ?? `better-auth-passkey`

  /** The account's ways to prove it is them, in the fixture's order. */
  async function methodsFor(ctx: Ctx, userId: string): Promise<StepUpMethod[]> {
    const instance = options.instance()
    let passkeys = 0
    if (instance.passkey) {
      passkeys = await ctx.context.adapter.count({
        model: `passkey`,
        where: [{ field: `userId`, value: userId }],
      })
    }
    const accounts = await ctx.context.internalAdapter.findAccounts(userId)
    const password = accounts.some((a) => a.providerId === `credential` && Boolean(a.password))
    return stepUpMethods({ passkeys, password }, instance)
  }

  /** The proof held: the CURRENT session becomes fresh, nothing else. */
  async function markFresh(ctx: Ctx, current: SessionCtx) {
    const updated = await ctx.context.internalAdapter.updateSession(current.session.token, {
      createdAt: new Date(),
    })
    if (!updated) fail(`UNAUTHORIZED`, `UNAUTHORIZED`, `Unauthorized`)
    await setSessionCookie(ctx, { session: updated, user: current.user as never })
    return ctx.json({ fresh: true })
  }

  async function requireMethod(ctx: Ctx, current: SessionCtx, method: StepUpMethod) {
    const methods = await methodsFor(ctx, current.user.id)
    if (!methods.includes(method)) {
      fail(`BAD_REQUEST`, `METHOD_NOT_AVAILABLE`, `That way to confirm it's you is not available.`)
    }
  }

  return {
    id: `exp-step-up`,
    endpoints: {
      stepUpMethods: createAuthEndpoint(
        `/step-up/methods`,
        { method: `GET`, use: [sessionMiddleware] },
        async (ctx) => {
          const current = ctx.context.session as unknown as SessionCtx
          return ctx.json({
            methods: await methodsFor(ctx, current.user.id),
            email: current.user.email,
          })
        }
      ),

      stepUpEmailSend: createAuthEndpoint(
        `/step-up/email/send`,
        { method: `POST`, use: [sessionMiddleware] },
        async (ctx) => {
          const current = ctx.context.session as unknown as SessionCtx
          await requireMethod(ctx, current, `email`)
          const identifier = stepUpCodeIdentifier(current.session.id)
          const code = String(randomInt(0, 1_000_000)).padStart(6, `0`)
          await ctx.context.internalAdapter.deleteVerificationByIdentifier(identifier)
          await ctx.context.internalAdapter.createVerificationValue({
            identifier,
            value: `${hashCode(code)}:0`,
            expiresAt: new Date(Date.now() + STEP_UP_CODE_TTL_MS),
          })
          await options.sendCode({ email: current.user.email, code, userId: current.user.id })
          return ctx.json({ sent: true })
        }
      ),

      stepUpEmailVerify: createAuthEndpoint(
        `/step-up/email/verify`,
        {
          method: `POST`,
          use: [sessionMiddleware],
          body: z.object({ code: z.string().trim().min(1).max(20) }),
        },
        async (ctx) => {
          const current = ctx.context.session as unknown as SessionCtx
          const identifier = stepUpCodeIdentifier(current.session.id)
          const row = await ctx.context.internalAdapter.findVerificationValue(identifier)
          if (!row) fail(`BAD_REQUEST`, `OTP_EXPIRED`, `That code expired. Request a new one.`)
          if (new Date(row.expiresAt).getTime() < Date.now()) {
            await ctx.context.internalAdapter.deleteVerificationByIdentifier(identifier)
            fail(`BAD_REQUEST`, `OTP_EXPIRED`, `That code expired. Request a new one.`)
          }
          const [hash = ``, triesText = `0`] = row.value.split(`:`)
          const tries = Number(triesText) || 0
          if (tries >= STEP_UP_CODE_ATTEMPTS) {
            await ctx.context.internalAdapter.deleteVerificationByIdentifier(identifier)
            fail(`FORBIDDEN`, `TOO_MANY_ATTEMPTS`, `Too many attempts. Request a new code.`)
          }
          if (!sameHash(hashCode(ctx.body.code), hash)) {
            await ctx.context.internalAdapter.updateVerificationByIdentifier(identifier, {
              value: `${hash}:${tries + 1}`,
            })
            fail(`BAD_REQUEST`, `INVALID_OTP`, `That code is not right.`)
          }
          await ctx.context.internalAdapter.deleteVerificationByIdentifier(identifier)
          return markFresh(ctx, current)
        }
      ),

      stepUpPassword: createAuthEndpoint(
        `/step-up/password`,
        {
          method: `POST`,
          use: [sessionMiddleware],
          body: z.object({ password: z.string().min(1).max(512) }),
        },
        async (ctx) => {
          const current = ctx.context.session as unknown as SessionCtx
          await requireMethod(ctx, current, `password`)
          const accounts = await ctx.context.internalAdapter.findAccounts(current.user.id)
          const credential = accounts.find((a) => a.providerId === `credential` && a.password)
          const ok =
            credential?.password &&
            (await ctx.context.password.verify({ hash: credential.password, password: ctx.body.password }))
          if (!ok) fail(`UNAUTHORIZED`, `INVALID_PASSWORD`, `That password is not right.`)
          return markFresh(ctx, current)
        }
      ),

      // The ceremony's options come from the passkey plugin's own
      // `/passkey/generate-authenticate-options` (with a session it lists
      // exactly this account's passkeys and sets its signed challenge
      // cookie); this endpoint consumes that challenge like the plugin's
      // verify-authentication does, but the passkey must be the SESSION
      // USER's and no session is created.
      stepUpPasskey: createAuthEndpoint(
        `/step-up/passkey`,
        {
          method: `POST`,
          use: [sessionMiddleware],
          body: z.object({ response: z.record(z.string(), z.any()) }),
        },
        async (ctx) => {
          const current = ctx.context.session as unknown as SessionCtx
          await requireMethod(ctx, current, `passkey`)
          const relying = options.passkey?.()
          if (!relying) fail(`BAD_REQUEST`, `METHOD_NOT_AVAILABLE`, `Passkeys are not available.`)
          const cookie = ctx.context.createAuthCookie(challengeCookie)
          const token = await ctx.getSignedCookie(cookie.name, ctx.context.secret)
          if (!token) fail(`BAD_REQUEST`, `PASSKEY_FAILED`, `The passkey challenge expired. Try again.`)
          const challenge = await ctx.context.internalAdapter.consumeVerificationValue(token)
          if (!challenge) fail(`BAD_REQUEST`, `PASSKEY_FAILED`, `The passkey challenge expired. Try again.`)
          const { type, expectedChallenge } = JSON.parse(challenge.value) as {
            type?: string
            expectedChallenge?: string
          }
          if (type !== `authentication` || !expectedChallenge) {
            fail(`BAD_REQUEST`, `PASSKEY_FAILED`, `The passkey challenge expired. Try again.`)
          }
          const response = ctx.body.response as { id?: unknown }
          const passkey =
            typeof response.id === `string`
              ? await ctx.context.adapter.findOne<{
                  id: string
                  userId: string
                  credentialID: string
                  publicKey: string
                  counter: number
                  transports?: string | null
                }>({
                  model: `passkey`,
                  where: [{ field: `credentialID`, value: response.id }],
                })
              : null
          // Another account's passkey proves another person.
          if (!passkey || passkey.userId !== current.user.id) {
            fail(`UNAUTHORIZED`, `PASSKEY_FAILED`, `Couldn't confirm it with that passkey.`)
          }
          let verified = false
          let newCounter = passkey.counter
          try {
            const verification = await verifyAuthenticationResponse({
              response: ctx.body.response as never,
              expectedChallenge,
              expectedOrigin: relying.origin,
              expectedRPID: relying.rpID,
              credential: {
                id: passkey.credentialID,
                publicKey: new Uint8Array(Buffer.from(passkey.publicKey, `base64`)),
                counter: passkey.counter,
                transports: passkey.transports?.split(`,`) as never,
              },
              requireUserVerification: false,
            })
            verified = verification.verified
            newCounter = verification.authenticationInfo.newCounter
          } catch {
            verified = false
          }
          if (!verified) fail(`UNAUTHORIZED`, `PASSKEY_FAILED`, `Couldn't confirm it with that passkey.`)
          await ctx.context.adapter.update({
            model: `passkey`,
            where: [{ field: `id`, value: passkey.id }],
            update: { counter: newCounter },
          })
          return markFresh(ctx, current)
        }
      ),
    },
    rateLimit: [
      // One code mail per 20 s per address-hop, like the sign-in code.
      { pathMatcher: (path) => path === `/step-up/email/send`, window: 60, max: 3 },
      { pathMatcher: (path) => path.startsWith(`/step-up/`) && path !== `/step-up/methods`, window: 60, max: 10 },
    ],
  }
}

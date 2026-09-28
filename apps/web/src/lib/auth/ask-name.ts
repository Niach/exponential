// EXP-1026: an email-code sign-up asks for a name. Better Auth's
// `POST /sign-in/email-otp` takes an optional `name`, used only when it
// CREATES the user — but it consumes the code before it creates anyone, so a
// client cannot learn "this is a new account" and then resubmit. This gate
// runs in `hooks.before` on that path, ahead of the plugin:
//
//   header `X-Exp-Ask-Name: 1` + no users row for the address + blank `name`
//   + a VALID live code → 400 `{ code: "NAME_REQUIRED" }`, code left intact.
//
// The code is only READ here (never consumed, never counted): an invalid,
// expired or exhausted code falls through to the plugin, which answers and
// counts it exactly as it always has. Existing accounts — unclaimed EXP-630
// placeholders included, they HAVE a users row — never see the step. A
// request without the header keeps today's behaviour, including the
// mailbox-local-part name default (`fallbackUserName`) for API-driven and old
// clients.
import { createHash, timingSafeEqual } from "node:crypto"
import { APIError, createAuthMiddleware } from "better-auth/api"
import { ASK_NAME_HEADER, NAME_REQUIRED_CODE } from "./sign-up-copy"

export const EMAIL_OTP_SIGN_IN_PATH = `/sign-in/email-otp`

export interface AskNameAdapter {
  findUserByEmail: (email: string) => Promise<unknown>
  findVerificationValue: (
    identifier: string
  ) => Promise<{ value: string; expiresAt: Date } | null | undefined>
}

/** The plugin's identifier for a sign-in code (`toOTPIdentifier`). */
export function signInOtpIdentifier(email: string): string {
  return `sign-in-otp-${email}`
}

/** The plugin's `storeOTP: "hashed"` digest: sha256, base64url, unpadded. */
export function hashOtp(otp: string): string {
  return createHash(`sha256`).update(otp).digest(`base64url`)
}

function sameString(a: string, b: string): boolean {
  const left = Buffer.from(a)
  const right = Buffer.from(b)
  return left.length === right.length && timingSafeEqual(left, right)
}

function hasText(value: unknown): boolean {
  return typeof value === `string` && value.trim().length > 0
}

/**
 * True when this sign-in must pause on the name step. Every "no" lets the
 * request through to the plugin untouched.
 */
export async function nameRequiredForOtpSignIn(
  request: {
    headers: Headers | null | undefined
    body: unknown
    /** The plugin's `disableSignUp`: a closed instance creates nobody, so
     *  there is no name to ask for (the plugin answers INVALID_OTP). */
    signUpDisabled: boolean
    allowedAttempts: number
  },
  adapter: AskNameAdapter
): Promise<boolean> {
  if (request.headers?.get(ASK_NAME_HEADER) !== `1`) return false
  if (request.signUpDisabled) return false
  const body = (request.body ?? {}) as {
    email?: unknown
    otp?: unknown
    name?: unknown
  }
  if (typeof body.email !== `string` || typeof body.otp !== `string`) {
    return false
  }
  if (hasText(body.name)) return false
  const email = body.email.toLowerCase()
  if (await adapter.findUserByEmail(email)) return false

  const stored = await adapter.findVerificationValue(signInOtpIdentifier(email))
  if (!stored || stored.expiresAt < new Date()) return false
  const colon = stored.value.lastIndexOf(`:`)
  const digest = colon === -1 ? stored.value : stored.value.slice(0, colon)
  const attempts = colon === -1 ? `` : stored.value.slice(colon + 1)
  if (attempts && Number.parseInt(attempts, 10) >= request.allowedAttempts) {
    return false
  }
  return sameString(hashOtp(body.otp), digest)
}

/** The `hooks.before` middleware (`lib/auth/index.ts`): throws the plugin's
 *  own error shape, `{ code, message }`, as a 400. */
export function askNameBeforeHook(options: {
  signUpDisabled: boolean
  allowedAttempts: number
}) {
  return createAuthMiddleware(async (ctx) => {
    if (ctx.path !== EMAIL_OTP_SIGN_IN_PATH) return
    const required = await nameRequiredForOtpSignIn(
      {
        headers: ctx.headers ?? ctx.request?.headers,
        body: ctx.body,
        signUpDisabled: options.signUpDisabled,
        allowedAttempts: options.allowedAttempts,
      },
      ctx.context.internalAdapter
    )
    if (!required) return
    throw new APIError(`BAD_REQUEST`, {
      code: NAME_REQUIRED_CODE,
      message: `A name is required to create this account.`,
    })
  })
}

/** EXP-857's default: an account created without a name (a code sign-in
 *  from a client that never asks) takes its mailbox's local part, so the
 *  chrome never shows an empty identity. Null = keep the given name. */
export function fallbackUserName(user: {
  name?: string | null
  email?: string | null
}): string | null {
  if (hasText(user.name)) return null
  const local = user.email?.split(`@`)[0] ?? ``
  return local || user.email || ``
}

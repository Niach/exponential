import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import {
  bearer,
  customSession,
  deviceAuthorization,
  emailOTP,
  genericOAuth,
  mcp,
} from "better-auth/plugins"
import { apiKey } from "@better-auth/api-key"
import { passkey } from "@better-auth/passkey"
import { creem } from "@creem_io/better-auth"
import { createAuthMiddleware } from "better-auth/api"
import { tanstackStartCookies } from "better-auth/tanstack-start"
import { eq, and } from "drizzle-orm"
import { db } from "@/db/connection"
import * as schema from "@/db/auth-schema"
import {
  parseOidcProviders,
  type OidcProviderConfig,
} from "@/lib/oidc-providers"
import { isCloudInstance, maybePromoteNewUser } from "@/lib/bootstrap-cloud"
import {
  isAuthRateLimitEnabled,
  isEmailOtpEnabled,
  isPasskeyEnabled,
  isPasswordSignupDisabled,
  passkeyRelyingParty,
} from "@/lib/auth/config"
import { androidPasskeyOrigins, parseFingerprints } from "@/lib/app-links"
import {
  recordEmailDelivery,
  sendEmailChangeCodeEmail,
  sendEmailChangedNoticeEmail,
  sendPasswordResetEmail,
  sendSignInCodeEmail,
  sendVerificationEmail,
} from "@/lib/email"
import { emailEnabled } from "@/lib/email-enabled"
import { dailyAnonymousIdFromHeaders } from "@/lib/conversion/anonymous"
import { recordConversionEvent } from "@/lib/conversion/events"
import { recordSubscriptionLifecycleEvent } from "@/lib/conversion/subscription-events"
import { isAdminUser } from "./app-user"
import {
  adoptProviderProfile,
  claimPlaceholder,
  providerProfileFromClaims,
} from "@/lib/placeholder-members"
import { mintAppleClientSecret } from "./apple"
import { githubOAuthClient } from "@/lib/integrations/github-app"
import { withAuthDbFailureSignal } from "./db-failure-signal"
import { askNameBeforeHook, fallbackUserName } from "./ask-name"
import {
  DISABLED_AUTH_PATHS,
  emailChangeNotice,
  signInMethodsGuardPlugin,
} from "./sign-in-methods"
import {
  fetchGithubUserInfo,
  refreshGithubAccessToken,
} from "@/lib/integrations/github-user"
import {
  resolveDesktopCardDismissal,
  resolveOnboardingCompletedAt,
} from "./onboarding"
import {
  bindSubscriptionToTeam,
  bindingInputFromCheckout,
  bindingInputFromSubscription,
} from "@/lib/billing/creem-binding"

export { parseOidcProviders, type OidcProviderConfig }

function extractGroups(profile: unknown, claimPath: string): string[] {
  if (!profile || typeof profile !== `object`) return []
  // Support dotted paths like `realm_access.roles`
  let value: unknown = profile
  for (const segment of claimPath.split(`.`)) {
    if (value && typeof value === `object` && segment in value) {
      value = (value as Record<string, unknown>)[segment]
    } else {
      return []
    }
  }
  if (Array.isArray(value)) {
    return value.filter((g): g is string => typeof g === `string`)
  }
  if (typeof value === `string`) {
    return [value]
  }
  return []
}

function isAdminFromProfile(
  profile: unknown,
  provider: OidcProviderConfig
): boolean {
  if (!provider.adminGroups || provider.adminGroups.length === 0) return false
  const groups = extractGroups(profile, provider.groupsClaim ?? `groups`)
  return provider.adminGroups.some((g) => groups.includes(g))
}

function decodeJwtPayload(token: string): unknown {
  const parts = token.split(`.`)
  if (parts.length !== 3) return null
  try {
    // base64url decode the payload (parts[1])
    const payload = parts[1].replace(/-/g, `+`).replace(/_/g, `/`)
    const padded = payload + `=`.repeat((4 - (payload.length % 4)) % 4)
    return JSON.parse(Buffer.from(padded, `base64`).toString(`utf-8`))
  } catch {
    return null
  }
}

const oidcProviders = parseOidcProviders()
const googleClientConfigured = Boolean(
  process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
)
const googleLoginEnabled =
  googleClientConfigured && process.env.GOOGLE_LOGIN_ENABLED === `true`
const googleSocialEnabled = googleLoginEnabled
// SLOP-7: the GitHub App's OAuth client is a `github` social provider. It is
// registered whenever the credentials exist — `linkSocial` is how a
// Google/Apple/code account connects GitHub for repositories — while the
// LOGIN button is a separate switch (GITHUB_LOGIN_ENABLED, lib/auth/config.ts).
const githubOAuth = githubOAuthClient()

// Sign in with Apple — required by App Store guideline 4.8 whenever the iOS
// app offers Google login. clientId is the Apple *Services ID* (web flow);
// the client secret is an ES256 JWT minted from the SIWA .p8 key at boot (see
// mintAppleClientSecret in lib/auth/apple.ts — an explicit APPLE_CLIENT_SECRET
// still wins). Apple hard-caps that JWT at 6 months, so a container left
// running that long without a restart will see Apple logins fail until it is
// restarted.
const appleClientSecret =
  process.env.APPLE_CLIENT_SECRET || mintAppleClientSecret()
const appleClientConfigured = Boolean(
  process.env.APPLE_CLIENT_ID && appleClientSecret
)
const appleLoginEnabled =
  appleClientConfigured && process.env.APPLE_LOGIN_ENABLED === `true`

// REV2-66: the Creem plugin registers its webhook endpoint ONLY when a secret
// is present, so a half-configured billing env fails silently in the worst
// place: checkout still works, the customer pays, and Creem's delivery hits a
// route that does not exist (404) — no creem_subscriptions row, no team/seat
// binding, no entitlements. The plugin's own signal is a debug-level log, so
// say it loudly at boot; the only other evidence is failed deliveries in
// Creem's dashboard.
if (
  isCloudInstance() &&
  process.env.CREEM_API_KEY &&
  !process.env.CREEM_WEBHOOK_SECRET
) {
  process.stderr.write(
    `[creem] CREEM_API_KEY is set but CREEM_WEBHOOK_SECRET is not — the webhook endpoint is NOT registered, so subscription events 404 and paid subscriptions never activate. Set CREEM_WEBHOOK_SECRET.\n`
  )
}

// EXP-857: a code burns after this many wrong tries (the plugin's
// `allowedAttempts`; the EXP-1026 name gate reads the same bound).
const EMAIL_OTP_ALLOWED_ATTEMPTS = 5

export const auth = betterAuth({
  // The token endpoints are server-only (see DISABLED_AUTH_PATHS).
  disabledPaths: DISABLED_AUTH_PATHS,
  database: withAuthDbFailureSignal(
    drizzleAdapter(db, {
      provider: `pg`,
      usePlural: true,
      schema,
    })
  ),
  emailAndPassword: {
    enabled: process.env.AUTH_PASSWORD_ENABLED !== `false`,
    // Public password sign-up: historically OFF in production (invite/OAuth
    // only). AUTH_SIGNUP_ENABLED overrides in either direction so the cloud
    // instance can open registration for launch (shared helper — also feeds
    // buildAuthConfig's signupEnabled).
    disableSignUp: isPasswordSignupDisabled(),
    // Constant on purpose (REV-5): this used to be NODE_ENV-derived, but the
    // shipped image never set NODE_ENV, so production accepted 1-char
    // passwords while reset-password.tsx and error-messages.ts promised 8.
    minPasswordLength: 8,
    sendResetPassword: async ({ user, url }) => {
      const result = await sendPasswordResetEmail({ to: user.email, url })
      await recordEmailDelivery({
        userId: user.id,
        toEmail: user.email,
        kind: `password_reset`,
        result,
      })
    },
  },
  emailVerification: {
    // Verification emails are sent but logging in is NOT blocked on them
    // (requireEmailVerification stays off) — low-friction launch posture.
    // OAuth/OIDC users arrive pre-verified by their provider. Privileged
    // side effects ARE gated on verification: initial-admin promotion waits
    // until the mailbox is proven (see maybePromoteNewUser).
    sendOnSignUp: emailEnabled,
    sendVerificationEmail: async ({ user, url }) => {
      const result = await sendVerificationEmail({ to: user.email, url })
      await recordEmailDelivery({
        userId: user.id,
        toEmail: user.email,
        kind: `email_verification`,
        result,
      })
    },
    afterEmailVerification: async (user) => {
      try {
        await maybePromoteNewUser(user.id, user.email, true)
      } catch (err) {
        console.error(
          `[auth] maybePromoteNewUser after verification failed for ${user.email}:`,
          err
        )
      }
    },
  },
  // EXP-1132: an OAuth callback that fails before its state is readable (a
  // state-cookie mismatch from a mail app's in-app browser, an expired flow)
  // knows no errorCallbackURL; Better Auth's default error route then bounces
  // to `/` in production, which read as "sent back to login for no reason"
  // and dropped the invite. Land on login instead: it renders `?error=` and
  // a remembered invite (lib/pending-invite.ts) resumes after sign-in.
  onAPIError: {
    errorURL: `/auth/login`,
  },
  session: {
    expiresIn: 60 * 60 * 24 * 60,
    updateAge: 60 * 60 * 24,
    cookieCache: {
      enabled: true,
      maxAge: 60 * 5,
    },
  },
  user: {
    additionalFields: {
      isAdmin: {
        type: `boolean`,
        defaultValue: false,
        input: false,
      },
      onboardingCompletedAt: {
        type: `date`,
        defaultValue: null,
        required: false,
        input: false,
      },
      // When the user dismissed the "Get the desktop app" card in the Agents
      // view (users.dismissDesktopAppCard). Surfaced read-only on the session
      // so the card stays hidden on later loads; never client-settable.
      desktopAppCardDismissedAt: {
        type: `date`,
        defaultValue: null,
        required: false,
        input: false,
      },
      // EXP-1126: stamped by the user.update hook below when the primary
      // email changes; INITIAL_ADMIN_EMAILS promotion skips stamped rows.
      // Declared so the adapter writes it; never client-settable.
      emailChangedAt: {
        type: `date`,
        defaultValue: null,
        required: false,
        input: false,
      },
    },
  },
  trustedOrigins: [
    ...(process.env.BETTER_AUTH_TRUSTED_ORIGINS || ``)
      .split(`,`)
      .filter(Boolean),
    // Apple's OAuth callback is a cross-origin form_post from appleid.apple.com;
    // without this Better Auth rejects the callback as a CSRF attempt.
    ...(appleLoginEnabled ? [`https://appleid.apple.com`] : []),
  ],
  rateLimit: {
    // Build-derived with an env override, NEVER NODE_ENV (REV-5) — with the
    // limiter off, Better Auth's per-path defaults (3/10s sign-in + sign-up,
    // 3/60s forget-password) never fire and nothing upstream compensates.
    enabled: isAuthRateLimitEnabled(),
    window: 60,
    max: 200,
    customRules: {
      "/get-session": { window: 60, max: 600 },
      // EXP-503: now that the buckets key on the proxy-attested last
      // x-forwarded-for hop (routes/api/auth/$.ts), one office NAT is one
      // bucket — better-auth's 3-per-10s default 429s the third coworker
      // signing in at 9am. 10/60s is burst-friendlier AND a lower sustained
      // rate (10/min vs 18/min) against online password guessing.
      "/sign-in/*": { window: 60, max: 10 },
      "/sign-up/*": { window: 60, max: 10 },
      // EXP-857: one code request per 20s per address-hop is plenty for a
      // human, and it bounds the mail a NAT can make us send. The plugin's
      // own 3/60s default on the same path stays underneath as the floor.
      "/email-otp/send-verification-otp": { window: 60, max: 3 },
      // EXP-1126: the change-email code is mail too; provider linking starts
      // are bounded like sign-ins.
      "/email-otp/request-email-change": { window: 60, max: 3 },
      "/link-social": { window: 60, max: 10 },
      "/oauth2/link": { window: 60, max: 10 },
      "/passkey/*": { window: 60, max: 30 },
    },
  },
  socialProviders: {
    ...(googleSocialEnabled
      ? {
          google: {
            clientId: process.env.GOOGLE_CLIENT_ID!,
            clientSecret: process.env.GOOGLE_CLIENT_SECRET!,
            accessType: `offline`,
            prompt: `select_account`,
          },
        }
      : {}),
    ...(githubOAuth
      ? {
          github: {
            clientId: githubOAuth.clientId,
            clientSecret: githubOAuth.clientSecret,
            // A GitHub App ignores OAuth scopes (its permissions are fixed on
            // the App); the defaults (`read:user user:email`) only matter for
            // a classic OAuth App and are harmless here.
            // Sign-up through GitHub follows the public-signup switch; the
            // login itself is gated by GITHUB_LOGIN_ENABLED in the guard
            // plugin (lib/auth/sign-in-methods.ts).
            disableSignUp: isPasswordSignupDisabled(),
            // Only a verified primary email may come through: `github` is a
            // trusted provider, so Better Auth would link by any address.
            getUserInfo: (token) => fetchGithubUserInfo(token),
            // GitHub answers a used refresh token with HTTP 200 + `error`;
            // this refresh throws on it instead of storing the dead token.
            refreshAccessToken: (refreshToken) =>
              refreshGithubAccessToken(refreshToken, githubOAuth),
          },
        }
      : {}),
    ...(appleLoginEnabled
      ? {
          apple: {
            clientId: process.env.APPLE_CLIENT_ID!,
            clientSecret: appleClientSecret!,
            // Lets the native iOS app exchange an ASAuthorization idToken
            // directly (audience = the app bundle id instead of the Services
            // ID). Harmless when unset — the web/ASWebAuthenticationSession
            // flow doesn't use it.
            ...(process.env.APPLE_APP_BUNDLE_IDENTIFIER
              ? {
                  appBundleIdentifier: process.env.APPLE_APP_BUNDLE_IDENTIFIER,
                }
              : {}),
          },
        }
      : {}),
  },
  // Without this, Better Auth refuses to attach a Google account to a
  // user that signed in via genericOAuth — the OAuth flow completes on
  // Google's side but the accounts row never lands.
  account: {
    accountLinking: {
      enabled: true,
      trustedProviders: [
        ...oidcProviders.map((p) => p.id),
        ...(googleSocialEnabled ? [`google`] : []),
        ...(appleLoginEnabled ? [`apple`] : []),
        ...(githubOAuth ? [`github`] : []),
      ],
      // Logged-in user's email (from an OIDC provider) likely differs from
      // their Google account email — without this, Better Auth refuses to link.
      allowDifferentEmails: true,
      // EXP-1126: Better Auth's last-account check counts `accounts` rows only
      // and would refuse to drop the sole Google row of an account that also
      // signs in by code or passkey. The real rule ("at least one way in
      // remains") lives in lib/auth/sign-in-methods.ts: the tRPC mutations
      // and the guard plugin below enforce it.
      allowUnlinkingAll: true,
    },
  },
  logger: {
    level: `debug`,
    log: (level, message, ...args) => {
      // stderr is unbuffered in Bun; console.log goes to stdout which can
      // get held in a buffer when not attached to a TTY (e.g. inside docker).
      const payload =
        args.length > 0
          ? `[better-auth][${level}] ${message} ${JSON.stringify(args)}\n`
          : `[better-auth][${level}] ${message}\n`
      process.stderr.write(payload)
    },
  },
  databaseHooks: {
    // EXP-630 placeholder members: a sign-in THROUGH a placeholder's email
    // (Google/Apple/OIDC link, sign-in code, password reset) makes the row a
    // real account. The linked account's id token carries the provider's
    // name and picture — those replace the invited (Linear/typed) name, but
    // only on an unclaimed placeholder; a real account's chosen name is never
    // overwritten by a later link. The session hook then clears the flag and
    // marks the pending invites accepted. Both are idempotent no-ops for
    // ordinary accounts.
    account: {
      create: {
        after: async (account) => {
          if (!account.idToken) return
          try {
            await adoptProviderProfile(
              db,
              account.userId,
              providerProfileFromClaims(decodeJwtPayload(account.idToken))
            )
          } catch (err) {
            console.error(`[auth] placeholder profile adoption failed:`, err)
          }
        },
      },
    },
    session: {
      create: {
        after: async (session) => {
          try {
            await claimPlaceholder(db, session.userId)
          } catch (err) {
            console.error(`[auth] placeholder claim failed:`, err)
          }
        },
      },
    },
    user: {
      create: {
        // EXP-857: a first sign-in with a one-time code from a client that
        // never asks for a name (API-driven or pre-EXP-1026 — current ones
        // pause on the name step, `ask-name.ts`) creates the account with no
        // name — default it from the mailbox so the chrome never shows an
        // empty identity. Apple's name-less accounts already fall back to
        // the email in the UI; this only fills what would otherwise be the
        // empty string.
        before: async (user) => {
          const name = fallbackUserName(user)
          if (name === null) return
          return { data: { ...user, name } }
        },
        after: async (user, ctx) => {
          try {
            await maybePromoteNewUser(user.id, user.email, user.emailVerified)
          } catch (err) {
            console.error(
              `[auth] maybePromoteNewUser failed for ${user.email}:`,
              err
            )
          }
          // No team auto-creation (EXP-188): new accounts start team-less
          // and the first-run onboarding offers "create a team or join one".

          // Conversion funnel (EXP-362, CLOUD-ONLY — self-hosted instances
          // record nothing): every account creation is one signup event (the
          // once-per-user index absorbs any re-fire). The daily anonymous
          // hash of the creating request links the account to its same-day
          // `landing` row; ref/utm attribution arrives separately via
          // users.claimSignupAttribution (cookieless — params ride URLs).
          try {
            if (!isCloudInstance()) return
            const headers = ctx?.headers ?? ctx?.request?.headers ?? null
            const anonymousId = headers
              ? dailyAnonymousIdFromHeaders(headers)
              : null
            if (anonymousId) {
              await db
                .update(schema.users)
                .set({ signupAnonymousId: anonymousId })
                .where(eq(schema.users.id, user.id))
            }
            await recordConversionEvent(db, {
              name: `signup`,
              userId: user.id,
              anonymousId,
            })
          } catch (err) {
            console.error(
              `[conversion] signup event failed for ${user.email}:`,
              err
            )
          }
        },
      },
      // EXP-1126: the ONLY Better Auth path that writes users.email is the
      // OTP change-email flow (core changeEmail stays off, updateUser refuses
      // the field, updateUserInfoOnLink is unset). Stamp the change so
      // INITIAL_ADMIN_EMAILS promotion (boot pass + verification hook) skips
      // this account for good: a changed address never grants admin.
      update: {
        before: async (data, ctx) => {
          if (typeof data.email !== `string`) return
          // Tell the OLD address (the requester's session still carries it)
          // that the account moved. Fire-and-forget: a mail failure never
          // blocks the change, and without a transport it is a no-op like
          // every other send.
          const notice = emailChangeNotice(data, ctx?.context.session)
          if (notice) {
            void sendEmailChangedNoticeEmail(notice)
              .then((result) =>
                recordEmailDelivery({
                  userId: ctx?.context.session?.user.id ?? null,
                  toEmail: notice.to,
                  kind: `email_changed_notice`,
                  result,
                })
              )
              .catch((err) => {
                console.error(`[auth] email-changed notice failed:`, err)
              })
          }
          return { data: { ...data, emailChangedAt: new Date() } }
        },
      },
    },
  },
  hooks: {
    // EXP-1026: a name-less first sign-in with a code, from a client that
    // sends `X-Exp-Ask-Name: 1`, answers NAME_REQUIRED with the code intact;
    // every other one gets its `name` trimmed and capped.
    before: askNameBeforeHook({
      signUpDisabled: isPasswordSignupDisabled(),
      allowedAttempts: EMAIL_OTP_ALLOWED_ATTEMPTS,
    }),
    after: createAuthMiddleware(async (ctx) => {
      // Re-evaluate admin status after every OIDC sign-in, so that group
      // changes upstream (added or revoked) take effect on next login.
      const newSession = ctx.context.newSession
      if (!newSession?.user) return

      // Match the genericOAuth callback path. better-auth mounts the plugin
      // under /oauth2/callback/:providerId.
      const path = ctx.path
      if (!path?.startsWith(`/oauth2/callback/`)) return

      const providerId = path.split(`/`).pop()
      if (!providerId) return

      const provider = oidcProviders.find((p) => p.id === providerId)
      if (!provider || !provider.adminGroups?.length) return

      try {
        const [account] = await db
          .select({ idToken: schema.accounts.idToken })
          .from(schema.accounts)
          .where(
            and(
              eq(schema.accounts.userId, newSession.user.id),
              eq(schema.accounts.providerId, providerId)
            )
          )
          .limit(1)

        if (!account?.idToken) return

        const claims = decodeJwtPayload(account.idToken)
        const shouldBeAdmin = isAdminFromProfile(claims, provider)
        const currentIsAdmin = isAdminUser(newSession.user)

        if (shouldBeAdmin !== currentIsAdmin) {
          await db
            .update(schema.users)
            .set({ isAdmin: shouldBeAdmin, updatedAt: new Date() })
            .where(eq(schema.users.id, newSession.user.id))
        }
      } catch (err) {
        console.error(`[auth] failed to re-evaluate admin status:`, err)
      }
    }),
  },
  plugins: [
    bearer(),
    // EXP-857 passwordless "Continue with email": a 6-digit code mailed by the
    // ONE transactional sender, valid 10 minutes, burned after 5 wrong tries.
    // Sign-up through the code follows the same public-signup switch as the
    // password form (a closed instance mails nothing for unknown addresses).
    // Registered only when mail is configured — without a transport the
    // endpoint would answer success and never deliver.
    ...(isEmailOtpEnabled()
      ? [
          emailOTP({
            otpLength: 6,
            expiresIn: 60 * 10,
            allowedAttempts: EMAIL_OTP_ALLOWED_ATTEMPTS,
            // Stored hashed: a live sign-in code in `verifications.value` is
            // a 10-minute session for anyone with a database read.
            storeOTP: `hashed`,
            disableSignUp: isPasswordSignupDisabled(),
            // EXP-1126: the primary email is changeable — a code mailed to
            // the NEW address (`/email-otp/request-email-change` then
            // `/email-otp/change-email`) proves the mailbox and swaps it,
            // verified. The current address is not re-proven (the session
            // is the proof of ownership here).
            changeEmail: { enabled: true },
            sendVerificationOTP: async ({ email, otp, type }) => {
              // Two codes are offered in the product: the sign-in code and
              // the change-email code. The plugin's other flows
              // (verification/reset by code) are never called by a client,
              // so their mail stays unsent.
              if (type === `sign-in`) {
                const result = await sendSignInCodeEmail({ to: email, code: otp })
                await recordEmailDelivery({
                  toEmail: email,
                  kind: `sign_in_code`,
                  result,
                })
                return
              }
              if (type === `change-email`) {
                const result = await sendEmailChangeCodeEmail({
                  to: email,
                  code: otp,
                })
                await recordEmailDelivery({
                  toEmail: email,
                  kind: `email_change_code`,
                  result,
                })
              }
            },
          }),
        ]
      : []),
    // EXP-857 "Login with passkey" (WebAuthn). rpID = the instance hostname;
    // the accepted client origins are the web origin (also what iOS's
    // associated-domain ceremony reports) plus one apk-key-hash origin per
    // Android signing cert (derived from the SAME fingerprints that feed
    // assetlinks.json). Registration needs a fresh session and happens on
    // the web Account page; natives only authenticate.
    ...(isPasskeyEnabled()
      ? [
          passkey({
            ...passkeyRelyingParty(),
            origin: [
              new URL(process.env.BETTER_AUTH_URL!).origin,
              ...androidPasskeyOrigins(
                parseFingerprints(process.env.ANDROID_APP_LINK_FINGERPRINTS)
              ),
            ],
            authenticatorSelection: {
              residentKey: `preferred`,
              userVerification: `preferred`,
            },
          }),
        ]
      : []),
    // EXP-1126: "at least one way in remains" in front of /unlink-account and
    // /passkey/delete-passkey for direct API callers (clients use tRPC).
    signInMethodsGuardPlugin(),
    apiKey({
      // Personal API keys (desktop coding sessions / MCP clients) — minted by
      // the user for their own auth, never a synthetic identity.
      defaultPrefix: `expu_`,
      enableMetadata: true,
      enableSessionForAPIKeys: true,
      // Default cap is 32, but keys are named `Device: <hostname>` and a
      // hostname like "macbook-pro-von-danny.local" blows past that → minting
      // would 500 on createApiKey. Give the name field generous room.
      maximumNameLength: 200,
      keyExpiration: { defaultExpiresIn: null },
      // Per-key rate limiting is off by default for personal keys — a desktop
      // coding session long-polls /api/shapes/* and would blow through the
      // plugin default of 10 req/day in seconds. The global Better Auth
      // rateLimit (configured above on the auth root) still applies, plus
      // network-level limits at the reverse proxy.
      rateLimit: { enabled: false },
      // Accept the key from either `x-api-key` (api-key plugin default) or
      // `Authorization: Bearer expu_...` so MCP clients that only know the
      // bearer convention can authenticate without a custom header.
      customAPIKeyGetter: (ctx) => {
        const direct = ctx.headers?.get(`x-api-key`)
        if (direct) return direct
        const authz = ctx.headers?.get(`authorization`)
        if (!authz) return null
        const match = authz.match(/^Bearer\s+(expu_[^\s]+)$/i)
        return match ? match[1] : null
      },
    }),
    // RFC 8628 device-code login for the `exponential` CLI (EXP-403): the CLI
    // POSTs /api/auth/device/code, the user approves on /auth/device, and the
    // CLI's token poll yields a regular Better Auth session token (accepted
    // everywhere via the bearer plugin). The default verification URI is
    // `/device` — point it at our route.
    deviceAuthorization({
      expiresIn: `10m`,
      interval: `5s`,
      verificationUri: `/auth/device`,
      // The plugin's option schema (zod 4) rejects an ABSENT `schema` key —
      // `z.custom(() => true)` without `.optional()`. An empty object is a
      // no-op for its mergeSchema and satisfies the parse.
      schema: {},
    }),
    mcp({
      loginPage: `/auth/login`,
      resource: process.env.BETTER_AUTH_URL
        ? `${process.env.BETTER_AUTH_URL.replace(/\/$/, ``)}/api/mcp`
        : undefined,
      // Human MCP clients (Claude etc.) hold a refreshable OAuth credential.
      // The mcp plugin reads token lifetimes from `oidcConfig`: give it a
      // generous refresh window so a client offline for weeks can still refresh
      // without re-authorizing; the short access-token life keeps rotation
      // frequent (getMcpSession ignores access expiry, but the client refreshes
      // proactively).
      oidcConfig: {
        loginPage: `/auth/login`,
        // Scope-selection consent screen (team/board multi-select →
        // mcp_grants). The /api/auth/$ route forces prompt=consent on every
        // mcp/authorize request so no client skips it.
        consentPage: `/auth/consent`,
        accessTokenExpiresIn: 60 * 60 * 24,
        refreshTokenExpiresIn: 60 * 60 * 24 * 90,
      },
    }),
    ...(oidcProviders.length > 0
      ? [
          genericOAuth({
            config: oidcProviders.map((p) => ({
              providerId: p.id,
              clientId: p.clientId,
              clientSecret: p.clientSecret,
              discoveryUrl: p.discoveryUrl,
              scopes:
                p.scopes ??
                (p.adminGroups?.length
                  ? [`openid`, `profile`, `email`, `groups`]
                  : [`openid`, `profile`, `email`]),
              mapProfileToUser: (profile) => ({
                name:
                  profile.name ||
                  profile.preferred_username ||
                  profile.nickname ||
                  profile.email,
                isAdmin: isAdminFromProfile(profile, p),
              }),
            })),
          }),
        ]
      : []),
    ...(isCloudInstance() && process.env.CREEM_API_KEY
      ? [
          creem({
            apiKey: process.env.CREEM_API_KEY,
            webhookSecret: process.env.CREEM_WEBHOOK_SECRET!,
            testMode:
              process.env.CREEM_API_KEY?.startsWith(`creem_test_`) ?? false,
            defaultSuccessUrl: `/settings/billing`,
            persistSubscriptions: true,
            // Bind the persisted subscription row to its team + seat count
            // from checkout metadata. The plugin's own persistence runs first
            // (creating the row keyed by creemSubscriptionId), then invokes this
            // callback with the flattened checkout entity, so the row exists by
            // the time we bind. See lib/billing/creem-binding.ts.
            onCheckoutCompleted: async (event) => {
              const bound = await bindSubscriptionToTeam(
                bindingInputFromCheckout(event)
              )
              if (bound) {
                process.stderr.write(
                  `[creem] bound subscription ${bound.creemSubscriptionId} → team ${bound.teamId} (${bound.seats} seats)\n`
                )
              }
            },
            onGrantAccess: async (event) => {
              process.stderr.write(
                `[creem] access granted: ${event.customer.email}, reason: ${event.reason}\n`
              )
              // Idempotent re-bind: heals the team/seats binding on the
              // subscription lifecycle events (active/trialing/paid), covering
              // the rare case where subscription.* lands before checkout.completed.
              await bindSubscriptionToTeam(bindingInputFromSubscription(event))
              // AFTER the bind so the row carries teamId/seats. Grant events
              // re-fire on every renewal — the once-per-sub index keeps only
              // the first trial_started / subscription_first_active.
              await recordSubscriptionLifecycleEvent({
                creemSubscriptionId: event.id,
                status: event.status,
                metadata: event.metadata,
              })
            },
            // Seat-count changes (billing.updateSeats, or a manual edit in the
            // Creem dashboard) arrive as subscription.update with the new
            // item units — re-bind so our `seats` column tracks them.
            onSubscriptionUpdate: async (event) => {
              await bindSubscriptionToTeam(bindingInputFromSubscription(event))
              await recordSubscriptionLifecycleEvent({
                creemSubscriptionId: event.id,
                status: event.status,
                metadata: event.metadata,
              })
            },
            // Terminal states → one subscription_canceled conversion event
            // (deduped per subscription; properties.status says which one).
            onSubscriptionCanceled: async (event) => {
              await recordSubscriptionLifecycleEvent({
                creemSubscriptionId: event.id,
                status: event.status,
                metadata: event.metadata,
                terminal: true,
              })
            },
            onSubscriptionExpired: async (event) => {
              await recordSubscriptionLifecycleEvent({
                creemSubscriptionId: event.id,
                status: event.status,
                metadata: event.metadata,
                terminal: true,
              })
            },
            onRevokeAccess: async ({ reason, customer }) => {
              process.stderr.write(
                `[creem] access revoked: ${customer.email}, reason: ${reason}\n`
              )
            },
          }),
        ]
      : []),
    // Unified onboarding gate: every client (web, iOS, Android) decides
    // "show the first-run wizard?" from this session's onboardingCompletedAt,
    // so the rule lives server-side in resolveOnboardingCompletedAt — users
    // who already have a real board get the flag backfilled on read.
    customSession(async ({ user, session }) => {
      // These flags out-live the 5-min session cookie cache via a fresh
      // resolve on read (see their resolvers for the one-way semantics).
      const [onboardingCompletedAt, dismissals] = await Promise.all([
        resolveOnboardingCompletedAt(user),
        resolveDesktopCardDismissal(user),
      ])
      return {
        user: { ...user, onboardingCompletedAt, ...dismissals },
        session,
      }
    }),
    // Must be last so it can capture Set-Cookie from any plugin's hooks.after.
    tanstackStartCookies(),
  ],
})

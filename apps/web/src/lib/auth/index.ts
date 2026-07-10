import { createPrivateKey, sign as cryptoSign } from "node:crypto"
import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import { bearer, customSession, genericOAuth, mcp } from "better-auth/plugins"
import { apiKey } from "@better-auth/api-key"
import { creem } from "@creem_io/better-auth"
import { createAuthMiddleware } from "better-auth/api"
import { tanstackStartCookies } from "better-auth/tanstack-start"
import { eq, and } from "drizzle-orm"
import { db } from "@/db/connection"
import * as schema from "@/db/auth-schema"
import { parseOidcProviders, type OidcProviderConfig } from "@/lib/oidc-providers"
import { isCloudInstance, maybePromoteNewUser } from "@/lib/bootstrap-cloud"
import { ensurePersonalWorkspace } from "@/lib/auth/personal-workspace"
import {
  emailEnabled,
  sendPasswordResetEmail,
  sendVerificationEmail,
} from "@/lib/email"
import { isAdminUser } from "./app-user"
import { resolveOnboardingCompletedAt } from "./onboarding"
import {
  bindSubscriptionToWorkspace,
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

// Sign in with Apple — required by App Store guideline 4.8 whenever the iOS
// app offers Google login. clientId is the Apple *Services ID* (web flow);
// the client secret is an ES256 JWT that Apple hard-caps at 6 months, so
// instead of a static APPLE_CLIENT_SECRET that someone must re-mint twice a
// year, the server mints it at boot from the SIWA .p8 key
// (APPLE_PRIVATE_KEY, base64 like GITHUB_APP_PRIVATE_KEY) — every
// restart/redeploy refreshes it. An explicit APPLE_CLIENT_SECRET still wins
// when set. Caveat: a container left running >6 months without a restart
// will see Apple logins fail until it is restarted.
function mintAppleClientSecret(): string | undefined {
  const keyB64 = process.env.APPLE_PRIVATE_KEY
  const keyId = process.env.APPLE_KEY_ID
  const teamId = process.env.APPLE_TEAM_ID
  const clientId = process.env.APPLE_CLIENT_ID
  if (!keyB64 || !keyId || !teamId || !clientId) return undefined
  try {
    const key = createPrivateKey(Buffer.from(keyB64, `base64`).toString(`utf8`))
    const b64u = (input: string | Buffer) =>
      Buffer.from(input).toString(`base64url`)
    const now = Math.floor(Date.now() / 1000)
    const data = `${b64u(JSON.stringify({ alg: `ES256`, kid: keyId }))}.${b64u(
      JSON.stringify({
        iss: teamId,
        iat: now,
        exp: now + 180 * 24 * 60 * 60, // Apple's maximum is 6 months
        aud: `https://appleid.apple.com`,
        sub: clientId,
      })
    )}`
    const sig = cryptoSign(`sha256`, Buffer.from(data), {
      key,
      dsaEncoding: `ieee-p1363`,
    })
    return `${data}.${b64u(sig)}`
  } catch (err) {
    console.error(
      `[auth] failed to mint the Apple client secret from APPLE_PRIVATE_KEY:`,
      err
    )
    return undefined
  }
}

const appleClientSecret =
  process.env.APPLE_CLIENT_SECRET || mintAppleClientSecret()
const appleClientConfigured = Boolean(
  process.env.APPLE_CLIENT_ID && appleClientSecret
)
const appleLoginEnabled =
  appleClientConfigured && process.env.APPLE_LOGIN_ENABLED === `true`

export const auth = betterAuth({
  database: drizzleAdapter(db, {
    provider: `pg`,
    usePlural: true,
    schema,
  }),
  emailAndPassword: {
    enabled: process.env.AUTH_PASSWORD_ENABLED !== `false`,
    // Public password sign-up: historically OFF in production (invite/OAuth
    // only). AUTH_SIGNUP_ENABLED overrides in either direction so the cloud
    // instance can open registration for launch.
    disableSignUp: process.env.AUTH_SIGNUP_ENABLED
      ? process.env.AUTH_SIGNUP_ENABLED === `false`
      : process.env.NODE_ENV === `production`,
    minPasswordLength: process.env.NODE_ENV === `production` ? 8 : 1,
    sendResetPassword: async ({ user, url }) => {
      await sendPasswordResetEmail({ to: user.email, url })
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
      await sendVerificationEmail({ to: user.email, url })
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
      // True for the synthetic widget-helpdesk bot user (owns issues created
      // through the embeddable feedback widget). Never settable by a client.
      isAgent: {
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
    enabled: process.env.NODE_ENV === `production`,
    window: 60,
    max: 200,
    customRules: {
      "/get-session": { window: 60, max: 600 },
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
                  appBundleIdentifier:
                    process.env.APPLE_APP_BUNDLE_IDENTIFIER,
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
      ],
      // Logged-in user's email (from an OIDC provider) likely differs from
      // their Google account email — without this, Better Auth refuses to link.
      allowDifferentEmails: true,
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
    user: {
      create: {
        after: async (user) => {
          try {
            await maybePromoteNewUser(user.id, user.email, user.emailVerified)
          } catch (err) {
            console.error(
              `[auth] maybePromoteNewUser failed for ${user.email}:`,
              err
            )
          }
          // Every real account gets its personal workspace at signup so any
          // client (web, mobile, desktop) sees a consistent state. Synthetic
          // agent users are inserted directly via Drizzle and never hit this
          // hook; the guard is defense-in-depth. Failures never block signup —
          // workspaces.ensureDefault self-heals later.
          try {
            if (!(user as { isAgent?: boolean }).isAgent) {
              await ensurePersonalWorkspace({
                userId: user.id,
                userName: user.name ?? null,
              })
            }
          } catch (err) {
            console.error(
              `[auth] ensurePersonalWorkspace failed for ${user.email}:`,
              err
            )
          }
        },
      },
    },
  },
  hooks: {
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
        // Scope-selection consent screen (workspace/project multi-select →
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
            testMode: process.env.CREEM_API_KEY?.startsWith(`creem_test_`) ?? false,
            defaultSuccessUrl: `/settings/billing`,
            persistSubscriptions: true,
            // Bind the persisted subscription row to its workspace + seat count
            // from checkout metadata. The plugin's own persistence runs first
            // (creating the row keyed by creemSubscriptionId), then invokes this
            // callback with the flattened checkout entity, so the row exists by
            // the time we bind. See lib/billing/creem-binding.ts.
            onCheckoutCompleted: async (event) => {
              const bound = await bindSubscriptionToWorkspace(
                bindingInputFromCheckout(event)
              )
              if (bound) {
                process.stderr.write(
                  `[creem] bound subscription ${bound.creemSubscriptionId} → workspace ${bound.workspaceId} (${bound.seats} seats)\n`
                )
              }
            },
            onGrantAccess: async (event) => {
              process.stderr.write(
                `[creem] access granted: ${event.customer.email}, reason: ${event.reason}\n`
              )
              // Idempotent re-bind: heals the workspace/seats binding on the
              // subscription lifecycle events (active/trialing/paid), covering
              // the rare case where subscription.* lands before checkout.completed.
              await bindSubscriptionToWorkspace(
                bindingInputFromSubscription(event)
              )
            },
            // Seat-count changes (billing.updateSeats, or a manual edit in the
            // Creem dashboard) arrive as subscription.update with the new
            // item units — re-bind so our `seats` column tracks them.
            onSubscriptionUpdate: async (event) => {
              await bindSubscriptionToWorkspace(
                bindingInputFromSubscription(event)
              )
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
    // who already have a real project get the flag backfilled on read.
    customSession(async ({ user, session }) => ({
      user: {
        ...user,
        onboardingCompletedAt: await resolveOnboardingCompletedAt(user),
      },
      session,
    })),
    // Must be last so it can capture Set-Cookie from any plugin's hooks.after.
    tanstackStartCookies(),
  ],
})

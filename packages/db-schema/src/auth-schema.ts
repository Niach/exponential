import {
  pgTable,
  text,
  timestamp,
  boolean,
  integer,
  index,
} from "drizzle-orm/pg-core"

export const users = pgTable(
  `users`,
  {
    id: text(`id`).primaryKey(),
    name: text(`name`).notNull(),
    email: text(`email`).notNull().unique(),
    emailVerified: boolean(`email_verified`)
      .$defaultFn(() => false)
      .notNull(),
    image: text(`image`),
    isAdmin: boolean(`is_admin`)
      .$defaultFn(() => false)
      .notNull(),
    creemCustomerId: text(`creem_customer_id`),
    hadTrial: boolean(`had_trial`).notNull().default(false),
    onboardingCompletedAt: timestamp(`onboarding_completed_at`, {
      withTimezone: true,
    }),
    // One-shot dismissal of the "Get the desktop app" card on the web Agents
    // view. SERVER-ONLY (users shape allowlist pins 6 columns; this never syncs).
    desktopAppCardDismissedAt: timestamp(`desktop_app_card_dismissed_at`, {
      withTimezone: true,
    }),
    // One-shot dismissal of the "Getting started" cards on the empty project
    // board (EXP-88). SERVER-ONLY like desktopAppCardDismissedAt; never syncs.
    gettingStartedDismissedAt: timestamp(`getting_started_dismissed_at`, {
      withTimezone: true,
    }),
    // Signup attribution (EXP-362) — SERVER-ONLY like the dismissal stamps;
    // never syncs. Stamped once, right after account creation: the ref/utm/
    // referrer values ride URLs (cookieless — see conversion_events), and the
    // anonymous id is the daily hash computed from the signup request, linking
    // the account to its same-day `landing` event.
    signupRef: text(`signup_ref`),
    signupUtmSource: text(`signup_utm_source`),
    signupUtmMedium: text(`signup_utm_medium`),
    signupUtmCampaign: text(`signup_utm_campaign`),
    signupReferrer: text(`signup_referrer`),
    signupLandingPath: text(`signup_landing_path`),
    // Creem's signed affiliate click token (EXP-384) — rides the same URL
    // pipeline as ref/utm and is re-appended to the hosted checkout URL so
    // the affiliate commission survives Creem's own redirect-set cookie being
    // purged between signup and checkout. Opaque, SERVER-ONLY like the rest.
    signupCreemRef: text(`signup_creem_ref`),
    signupAnonymousId: text(`signup_anonymous_id`),
    // IANA timezone name (e.g. `Europe/Berlin`) — the clock the daily digest's
    // send hour is interpreted in. Claimed once from the client at first
    // authenticated load; NULL means "never captured" and resolves to UTC.
    // SERVER-ONLY like the stamps above (the users shape allowlist pins 6
    // columns; this never syncs).
    timezone: text(`timezone`),
    createdAt: timestamp(`created_at`)
      .$defaultFn(() => /* @__PURE__ */ new Date())
      .notNull(),
    updatedAt: timestamp(`updated_at`)
      .$defaultFn(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index(`idx_users_signup_anonymous_id`).on(table.signupAnonymousId),
  ]
)

export const sessions = pgTable(`sessions`, {
  id: text(`id`).primaryKey(),
  expiresAt: timestamp(`expires_at`).notNull(),
  token: text(`token`).notNull().unique(),
  createdAt: timestamp(`created_at`).notNull(),
  updatedAt: timestamp(`updated_at`).notNull(),
  ipAddress: text(`ip_address`),
  userAgent: text(`user_agent`),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
})

export const accounts = pgTable(`accounts`, {
  id: text(`id`).primaryKey(),
  accountId: text(`account_id`).notNull(),
  providerId: text(`provider_id`).notNull(),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  accessToken: text(`access_token`),
  refreshToken: text(`refresh_token`),
  idToken: text(`id_token`),
  accessTokenExpiresAt: timestamp(`access_token_expires_at`),
  refreshTokenExpiresAt: timestamp(`refresh_token_expires_at`),
  scope: text(`scope`),
  password: text(`password`),
  createdAt: timestamp(`created_at`).notNull(),
  updatedAt: timestamp(`updated_at`).notNull(),
})

export const verifications = pgTable(`verifications`, {
  id: text(`id`).primaryKey(),
  identifier: text(`identifier`).notNull(),
  value: text(`value`).notNull(),
  expiresAt: timestamp(`expires_at`).notNull(),
  createdAt: timestamp(`created_at`).$defaultFn(
    () => /* @__PURE__ */ new Date()
  ),
  updatedAt: timestamp(`updated_at`).$defaultFn(
    () => /* @__PURE__ */ new Date()
  ),
})

// Tables for the better-auth `mcp` plugin. With `usePlural: true` on the
// drizzle adapter, Better Auth looks up schema exports `oauthApplications`,
// `oauthAccessTokens`, `oauthConsents`.

export const oauthApplications = pgTable(`oauth_applications`, {
  id: text(`id`).primaryKey(),
  name: text(`name`).notNull(),
  icon: text(`icon`),
  metadata: text(`metadata`),
  clientId: text(`client_id`).notNull().unique(),
  clientSecret: text(`client_secret`),
  redirectUrls: text(`redirect_urls`).notNull(),
  type: text(`type`).notNull(),
  disabled: boolean(`disabled`)
    .$defaultFn(() => false)
    .notNull(),
  userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }),
  createdAt: timestamp(`created_at`).notNull(),
  updatedAt: timestamp(`updated_at`).notNull(),
})

export const oauthAccessTokens = pgTable(`oauth_access_tokens`, {
  id: text(`id`).primaryKey(),
  accessToken: text(`access_token`).notNull().unique(),
  refreshToken: text(`refresh_token`).notNull().unique(),
  accessTokenExpiresAt: timestamp(`access_token_expires_at`).notNull(),
  refreshTokenExpiresAt: timestamp(`refresh_token_expires_at`).notNull(),
  clientId: text(`client_id`)
    .notNull()
    .references(() => oauthApplications.clientId, { onDelete: `cascade` }),
  userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }),
  scopes: text(`scopes`).notNull(),
  createdAt: timestamp(`created_at`).notNull(),
  updatedAt: timestamp(`updated_at`).notNull(),
})

export const oauthConsents = pgTable(`oauth_consents`, {
  id: text(`id`).primaryKey(),
  clientId: text(`client_id`)
    .notNull()
    .references(() => oauthApplications.clientId, { onDelete: `cascade` }),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  scopes: text(`scopes`).notNull(),
  consentGiven: boolean(`consent_given`).notNull(),
  createdAt: timestamp(`created_at`).notNull(),
  updatedAt: timestamp(`updated_at`).notNull(),
})

// NOTE: `creem_subscriptions` lives in `schema.ts` (not here) so its
// `team_id` FK can reference `teams` without auth-schema.ts taking a
// static import on schema.ts — that edge would form an eval-time circular
// import (schema.ts re-exports auth-schema.ts) and crash `createSelectSchema`.
// The Better Auth adapter still receives it via the `@/db/auth-schema`
// re-export in the web app.

// Table for the better-auth `deviceAuthorization` plugin (EXP-403 CLI
// login, RFC 8628). With `usePlural: true` on the drizzle adapter, Better
// Auth looks up schema export `deviceCodes` for the `deviceCode` model.
// Field list mirrors the plugin schema exactly (no timestamps there);
// `pollingInterval` is stored in milliseconds. Rows are short-lived — the
// plugin deletes them on token issue / deny / expiry-poll.
export const deviceCodes = pgTable(
  `device_codes`,
  {
    id: text(`id`).primaryKey(),
    deviceCode: text(`device_code`).notNull(),
    userCode: text(`user_code`).notNull(),
    // Claimed by the verifying session's user; null until claimed. Cascade
    // like every other user-linked auth table (REV2-16).
    userId: text(`user_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    expiresAt: timestamp(`expires_at`).notNull(),
    status: text(`status`).notNull(),
    lastPolledAt: timestamp(`last_polled_at`),
    pollingInterval: integer(`polling_interval`),
    clientId: text(`client_id`),
    scope: text(`scope`),
  },
  (table) => [
    index(`device_codes_device_code_idx`).on(table.deviceCode),
    index(`device_codes_user_code_idx`).on(table.userCode),
    index(`device_codes_user_id_idx`).on(table.userId),
  ]
)

// Table for the better-auth `@better-auth/api-key` plugin. With
// `usePlural: true` on the drizzle adapter, Better Auth looks up
// schema export `apikeys` for the `apikey` model.
export const apikeys = pgTable(
  `apikeys`,
  {
    id: text(`id`).primaryKey(),
    configId: text(`config_id`).notNull(),
    name: text(`name`),
    start: text(`start`),
    // The owning user's id. Better Auth's api-key plugin calls this
    // `reference_id` (not `user_id`), but it must cascade like every other
    // user-linked auth table — account deletion relies on it (REV2-16).
    referenceId: text(`reference_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    prefix: text(`prefix`),
    key: text(`key`).notNull(),
    refillInterval: integer(`refill_interval`),
    refillAmount: integer(`refill_amount`),
    lastRefillAt: timestamp(`last_refill_at`),
    enabled: boolean(`enabled`)
      .$defaultFn(() => true)
      .notNull(),
    rateLimitEnabled: boolean(`rate_limit_enabled`)
      .$defaultFn(() => true)
      .notNull(),
    rateLimitTimeWindow: integer(`rate_limit_time_window`),
    rateLimitMax: integer(`rate_limit_max`),
    requestCount: integer(`request_count`)
      .$defaultFn(() => 0)
      .notNull(),
    remaining: integer(`remaining`),
    lastRequest: timestamp(`last_request`),
    expiresAt: timestamp(`expires_at`),
    createdAt: timestamp(`created_at`).notNull(),
    updatedAt: timestamp(`updated_at`).notNull(),
    permissions: text(`permissions`),
    metadata: text(`metadata`),
  },
  (table) => [
    index(`apikeys_config_id_idx`).on(table.configId),
    index(`apikeys_reference_id_idx`).on(table.referenceId),
    index(`apikeys_key_idx`).on(table.key),
  ]
)

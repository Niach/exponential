import {
  pgTable,
  text,
  timestamp,
  boolean,
  integer,
  index,
} from "drizzle-orm/pg-core"

export const users = pgTable(`users`, {
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
  createdAt: timestamp(`created_at`)
    .$defaultFn(() => /* @__PURE__ */ new Date())
    .notNull(),
  updatedAt: timestamp(`updated_at`)
    .$defaultFn(() => /* @__PURE__ */ new Date())
    .notNull(),
})

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
  disabled: boolean(`disabled`).$defaultFn(() => false).notNull(),
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

// Table for the @creem_io/better-auth plugin. With `usePlural: true` on the
// drizzle adapter, Better Auth looks up schema export `creemSubscriptions`
// for the `creem_subscription` model.
export const creemSubscriptions = pgTable(`creem_subscriptions`, {
  id: text(`id`).primaryKey(),
  productId: text(`product_id`).notNull(),
  referenceId: text(`reference_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  creemCustomerId: text(`creem_customer_id`),
  creemSubscriptionId: text(`creem_subscription_id`),
  creemOrderId: text(`creem_order_id`),
  status: text(`status`).$defaultFn(() => `pending`).notNull(),
  periodStart: timestamp(`period_start`),
  periodEnd: timestamp(`period_end`),
  cancelAtPeriodEnd: boolean(`cancel_at_period_end`)
    .$defaultFn(() => false)
    .notNull(),
  createdAt: timestamp(`created_at`)
    .$defaultFn(() => /* @__PURE__ */ new Date())
    .notNull(),
  updatedAt: timestamp(`updated_at`)
    .$defaultFn(() => /* @__PURE__ */ new Date())
    .notNull(),
})

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
    referenceId: text(`reference_id`).notNull(),
    prefix: text(`prefix`),
    key: text(`key`).notNull(),
    refillInterval: integer(`refill_interval`),
    refillAmount: integer(`refill_amount`),
    lastRefillAt: timestamp(`last_refill_at`),
    enabled: boolean(`enabled`).$defaultFn(() => true).notNull(),
    rateLimitEnabled: boolean(`rate_limit_enabled`)
      .$defaultFn(() => true)
      .notNull(),
    rateLimitTimeWindow: integer(`rate_limit_time_window`),
    rateLimitMax: integer(`rate_limit_max`),
    requestCount: integer(`request_count`).$defaultFn(() => 0).notNull(),
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

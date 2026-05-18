import { pgTable, text, timestamp, boolean } from "drizzle-orm/pg-core"

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

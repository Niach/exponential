export * from "@exp/db-schema/auth-schema"
// `creem_subscriptions` is defined in schema.ts (so its team_id FK can
// reference `teams` without a circular import), but the Better Auth
// drizzle adapter resolves its model from this namespace — re-export it here.
export { creem_subscriptions } from "@exp/db-schema/schema"
// EXP-1291: Better Auth's `session` model (usePlural → the `sessions` key)
// is the `auth_sessions` table; the product `sessions` tree lives in schema.ts.
export { authSessions as sessions } from "@exp/db-schema/auth-schema"

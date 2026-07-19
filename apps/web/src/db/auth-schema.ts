export * from "@exp/db-schema/auth-schema"
// `creem_subscriptions` is defined in schema.ts (so its team_id FK can
// reference `teams` without a circular import), but the Better Auth
// drizzle adapter resolves its model from this namespace — re-export it here.
export { creem_subscriptions } from "@exp/db-schema/schema"

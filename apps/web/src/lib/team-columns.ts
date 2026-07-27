import { teams } from "@/db/schema"

// The synced teams contract — the Drizzle mirror of the teams shape proxy's
// server-pinned allowlist (routes/api/shapes/teams.ts TEAM_COLUMNS). Every
// member-facing read/return path projects THIS instead of `select()` /
// bare `.returning()`, so a server-only column (`comp_tier` today, billing
// metadata tomorrow) can no more leak through tRPC or MCP than it can
// through sync (REV2-67).
export const teamColumns = {
  id: teams.id,
  name: teams.name,
  slug: teams.slug,
  iconUrl: teams.iconUrl,
  helpdeskEnabled: teams.helpdeskEnabled,
  createdAt: teams.createdAt,
  updatedAt: teams.updatedAt,
} as const

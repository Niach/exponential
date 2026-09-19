// EXP-988 contract, open decision 1 (owner of everything else: EXP-891).
//
// The planner's proposal: MCP servers become rows scoped to (device, team
// member) instead of one user-wide set. The desktop / CLI autodetects what the
// local claude/codex config already has and offers to import it; rows are
// edited only in the IDE/CLI; the web shows them read-only per device.
//
// NOTE for the review: a TEAM-scoped, server-only `mcp_servers` table already
// exists (EXP-792: non-secret config + a per-device readiness matrix,
// `lib/trpc/mcp-servers.ts`). EXP-891 decides whether this type becomes a NEW
// table beside it or a `device_id`/`source` widening of the existing one; the
// migration is EXP-891's either way. The contract only fixes the SHAPE.
import type { McpTransport } from "@exp/db-schema/domain"

export const deviceMcpServerSources = [`detected`, `manual`] as const
export type DeviceMcpServerSource = (typeof deviceMcpServerSources)[number]

export interface DeviceMcpServer {
  id: string
  /** `devices.device_id` (the machine's stable id, not the row id). */
  deviceId: string
  /** The team member who owns the device. */
  userId: string
  name: string
  transport: McpTransport
  /** `http` transport. */
  url?: string | null
  /** `stdio` transport. */
  command?: string | null
  /** `detected` = imported from the local agent config; `manual` = typed. */
  source: DeviceMcpServerSource
  enabled: boolean
}

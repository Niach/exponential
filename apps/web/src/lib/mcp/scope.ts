import { and, eq, gt, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { mcpGrants, oauthAccessTokens, projects } from "@/db/schema"

// What an /api/mcp request may touch. Session-cookie and personal `expu_`
// api-key requests are the user's own credentials and get `full` access; an
// OAuth access token (a human MCP client like Claude) is confined to the
// workspaces/projects its consent grant selected. A token whose (user,
// client) pair has no grant row gets NOTHING — the holder must re-run the
// consent flow. The scope is enforced in the MCP tool layer; OAuth tokens are
// not accepted anywhere else (see resolveSession), so the tool layer is the
// complete surface.
export interface McpAccess {
  full: boolean
  /** Whole-workspace grants — includes projects created later. */
  fullWorkspaceIds: ReadonlySet<string>
  /** Individually granted projects. */
  grantedProjectIds: ReadonlySet<string>
  /**
   * Workspaces reachable at all: fully granted ones plus the hosts of
   * individually granted projects (those need workspace-level aux reads —
   * labels, members — for issue workflows).
   */
  visibleWorkspaceIds: ReadonlySet<string>
}

export const FULL_ACCESS: McpAccess = {
  full: true,
  fullWorkspaceIds: new Set(),
  grantedProjectIds: new Set(),
  visibleWorkspaceIds: new Set(),
}

export const NO_ACCESS: McpAccess = {
  full: false,
  fullWorkspaceIds: new Set(),
  grantedProjectIds: new Set(),
  visibleWorkspaceIds: new Set(),
}

interface GrantShape {
  allWorkspaces: boolean
  workspaceIds: string[]
  projectIds: string[]
}

// Pure core, unit-testable: `projectWorkspaceIds` maps each granted project
// to its workspace id (resolved by the caller).
export function buildMcpAccess(
  grant: GrantShape,
  projectWorkspaceIds: ReadonlyMap<string, string>
): McpAccess {
  if (grant.allWorkspaces) return FULL_ACCESS
  const fullWorkspaceIds = new Set(grant.workspaceIds)
  const grantedProjectIds = new Set(grant.projectIds)
  const visibleWorkspaceIds = new Set(grant.workspaceIds)
  for (const projectId of grant.projectIds) {
    const workspaceId = projectWorkspaceIds.get(projectId)
    if (workspaceId) visibleWorkspaceIds.add(workspaceId)
  }
  return { full: false, fullWorkspaceIds, grantedProjectIds, visibleWorkspaceIds }
}

export async function resolveMcpAccessForGrant(
  grant: GrantShape | null | undefined
): Promise<McpAccess> {
  if (!grant) return NO_ACCESS
  if (grant.allWorkspaces) return FULL_ACCESS
  const map = new Map<string, string>()
  if (grant.projectIds.length > 0) {
    const rows = await db
      .select({ id: projects.id, workspaceId: projects.workspaceId })
      .from(projects)
      .where(inArray(projects.id, grant.projectIds))
    for (const row of rows) map.set(row.id, row.workspaceId)
  }
  return buildMcpAccess(grant, map)
}

// Resolve an OAuth access token (already stripped of "Bearer ") to its user +
// scope. Returns null for unknown or expired tokens. Unlike better-auth's
// getMcpSession this DOES enforce access-token expiry — clients hold a
// refresh token and recover from the resulting 401.
export async function resolveMcpTokenAccess(
  bearerToken: string
): Promise<{ userId: string; clientId: string; access: McpAccess } | null> {
  const [token] = await db
    .select({
      userId: oauthAccessTokens.userId,
      clientId: oauthAccessTokens.clientId,
      expiresAt: oauthAccessTokens.accessTokenExpiresAt,
    })
    .from(oauthAccessTokens)
    .where(
      and(
        eq(oauthAccessTokens.accessToken, bearerToken),
        gt(oauthAccessTokens.accessTokenExpiresAt, new Date())
      )
    )
    .limit(1)
  if (!token?.userId) return null

  const [grant] = await db
    .select({
      allWorkspaces: mcpGrants.allWorkspaces,
      workspaceIds: mcpGrants.workspaceIds,
      projectIds: mcpGrants.projectIds,
    })
    .from(mcpGrants)
    .where(
      and(
        eq(mcpGrants.userId, token.userId),
        eq(mcpGrants.clientId, token.clientId)
      )
    )
    .limit(1)

  return {
    userId: token.userId,
    clientId: token.clientId,
    access: await resolveMcpAccessForGrant(grant),
  }
}

const deniedMessage = (target: string) =>
  `This MCP connection was not granted access to ${target}. Re-authenticate the MCP server and adjust the workspace/project selection on the consent screen.`

export function isWorkspaceVisible(access: McpAccess, workspaceId: string) {
  return access.full || access.visibleWorkspaceIds.has(workspaceId)
}

export function isWorkspaceFullyGranted(
  access: McpAccess,
  workspaceId: string
) {
  return access.full || access.fullWorkspaceIds.has(workspaceId)
}

export function isProjectGranted(
  access: McpAccess,
  projectId: string,
  workspaceId: string
) {
  return (
    access.full ||
    access.grantedProjectIds.has(projectId) ||
    access.fullWorkspaceIds.has(workspaceId)
  )
}

/** Workspace-level reads needed by issue workflows: labels, members, repos. */
export function assertWorkspaceVisible(access: McpAccess, workspaceId: string) {
  if (!isWorkspaceVisible(access, workspaceId)) {
    throw new Error(deniedMessage(`this workspace`))
  }
}

/** Workspace-level mutations: settings, invites, labels, repos, new projects. */
export function assertWorkspaceFullyGranted(
  access: McpAccess,
  workspaceId: string
) {
  if (!isWorkspaceFullyGranted(access, workspaceId)) {
    throw new Error(
      deniedMessage(`workspace-level operations in this workspace`)
    )
  }
}

export function assertProjectGranted(
  access: McpAccess,
  projectId: string,
  workspaceId: string
) {
  if (!isProjectGranted(access, projectId, workspaceId)) {
    throw new Error(deniedMessage(`this project`))
  }
}

/** Operations spanning every workspace (create workspace, whole inbox). */
export function assertFullAccess(access: McpAccess) {
  if (!access.full) {
    throw new Error(
      deniedMessage(`data outside its granted workspaces/projects`)
    )
  }
}

export function filterVisibleWorkspaceIds(
  access: McpAccess,
  workspaceIds: string[]
): string[] {
  if (access.full) return workspaceIds
  return workspaceIds.filter((id) => access.visibleWorkspaceIds.has(id))
}

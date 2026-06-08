import { eq } from "drizzle-orm"
import { apikeys } from "@/db/auth-schema"
import { issues, agentRegistrations, workspaceMembers } from "@/db/schema"

interface DeviceAgentRecord {
  id: string
  userId: string
  apiKeyId: string | null
}

// Revoke a desktop device (account-level): kill its API key(s), unassign every
// issue it holds across all workspaces, drop all of its agent memberships, and
// delete the registration row. The synthetic agent user is intentionally KEPT
// so its past comments / PR activity stay attributed; re-registering the same
// machine mints a fresh device + key.
export async function revokeDeviceAgent(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  agent: DeviceAgentRecord
) {
  await db.transaction(async (tx) => {
    // All expk_ keys minted for this device's agent user (key id may be stale
    // after a rotation, so match by reference too).
    await tx.delete(apikeys).where(eq(apikeys.referenceId, agent.userId))

    // Unassign every issue this device holds, in any workspace.
    await tx
      .update(issues)
      .set({ assigneeId: null })
      .where(eq(issues.assigneeId, agent.userId))

    // Remove the device from every workspace it fanned out into.
    await tx
      .delete(workspaceMembers)
      .where(eq(workspaceMembers.userId, agent.userId))

    await tx
      .delete(agentRegistrations)
      .where(eq(agentRegistrations.id, agent.id))
  })
}

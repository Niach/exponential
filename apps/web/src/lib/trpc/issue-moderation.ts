import { getWorkspaceById, isWorkspaceModerator } from "@/lib/workspace-membership"

// Fields a non-moderator may NOT set on issues in a PUBLIC workspace. Title,
// description and labels stay open; everything listed here is moderation-gated
// and is clamped (on create) or stripped (on update) server-side so a stale or
// tampered client cannot bypass the UI restrictions.
export const MODERATION_RESTRICTED_FIELDS = [
  `status`,
  `priority`,
  `assigneeId`,
  `dueDate`,
  `dueTime`,
  `endTime`,
  `recurrenceInterval`,
  `recurrenceUnit`,
  `archivedAt`,
] as const

/**
 * True when the user is a non-moderator acting in a public workspace, so the
 * moderation-gated fields must be clamped (create) or stripped (update).
 */
export async function isModerationRestricted(
  userId: string,
  workspaceId: string
): Promise<boolean> {
  const workspace = await getWorkspaceById(workspaceId)
  if (!workspace?.isPublic) return false
  return !(await isWorkspaceModerator(userId, workspaceId))
}

/** Remove the moderation-gated fields from an update payload in place. */
export function stripModerationFields(updates: Record<string, unknown>) {
  for (const field of MODERATION_RESTRICTED_FIELDS) {
    delete updates[field]
  }
}

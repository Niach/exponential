// EXP-630: the team as the plan builder and the dry run see it — one loader
// shared by the router (dry run, default plan) and the worker (apply ports).
import { and, eq, gt, inArray, isNull, isNotNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  boards,
  importEntityMap,
  issueStatuses,
  issues,
  labels,
  teamInvites,
  teamMembers,
  teams,
  users,
} from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { assertCanInviteMember, getTeamPlan, getTeamUsage } from "@/lib/billing"
import type { IssueEstimation, IssueStatus, IssueStatusCategory } from "@/lib/domain"
import type { TeamState } from "@/lib/import/plan"

export async function loadImportTeamState(
  teamId: string,
  options: {
    // The entity-map namespace (bundle.source) for the already-imported set.
    namespace: string | null
    // Existing boards whose taken numbers the dry run needs.
    boardIdsForNumbers?: string[]
  }
): Promise<TeamState> {
  // Archived boards (EXP-500) are included with their flag: they keep the
  // prefix reservation (a collision the dry run must name) and an earlier
  // run's archive board is a legitimate target for more archived issues.
  // Trashed boards stay out.
  const boardRows = await db
    .select({
      id: boards.id,
      name: boards.name,
      prefix: boards.prefix,
      archivedAt: boards.archivedAt,
      issueCount: sql<number>`(select count(*)::int from ${issues} where ${issues.boardId} = ${boards.id})`,
    })
    .from(boards)
    .where(and(eq(boards.teamId, teamId), isNull(boards.deletedAt)))

  const numbersByBoard = new Map<string, number[]>()
  const wanted = (options.boardIdsForNumbers ?? []).filter((id) =>
    boardRows.some((row) => row.id === id)
  )
  if (wanted.length > 0) {
    const rows = await db
      .select({ boardId: issues.boardId, number: issues.number })
      .from(issues)
      .where(inArray(issues.boardId, wanted))
    for (const row of rows) {
      const list = numbersByBoard.get(row.boardId) ?? []
      list.push(row.number)
      numbersByBoard.set(row.boardId, list)
    }
  }

  const statusRows = await db
    .select({
      id: issueStatuses.id,
      name: issueStatuses.name,
      category: issueStatuses.category,
      builtinKey: issueStatuses.builtinKey,
      color: issueStatuses.color,
    })
    .from(issueStatuses)
    .where(eq(issueStatuses.teamId, teamId))

  const labelRows = await db
    .select({ id: labels.id, name: labels.name, color: labels.color })
    .from(labels)
    .where(eq(labels.teamId, teamId))

  const [teamRow] = await db
    .select({ estimationType: teams.estimationType })
    .from(teams)
    .where(eq(teams.id, teamId))

  const memberRows = await db
    .select({ userId: teamMembers.userId, email: users.email, name: users.name })
    .from(teamMembers)
    .innerJoin(users, eq(users.id, teamMembers.userId))
    .where(eq(teamMembers.teamId, teamId))

  const inviteRows = await db
    .select({ email: teamInvites.email })
    .from(teamInvites)
    .where(
      and(
        eq(teamInvites.teamId, teamId),
        isNull(teamInvites.acceptedAt),
        gt(teamInvites.expiresAt, new Date()),
        isNotNull(teamInvites.email)
      )
    )

  let canInvite = true
  try {
    await assertCanInviteMember(teamId)
  } catch {
    canInvite = false
  }

  let storage: TeamState[`storage`] = { limitBytes: null, usedBytes: 0 }
  if (isCloudInstance()) {
    const [{ limits }, usage] = await Promise.all([getTeamPlan(teamId), getTeamUsage(teamId)])
    storage = {
      limitBytes: limits.storageMb === Infinity ? null : limits.storageMb * 1024 * 1024,
      usedBytes: Math.round(usage.storageMb * 1024 * 1024),
    }
  }

  const importedIssueKeys = new Set<string>()
  const importedBoards = new Map<string, string>()
  if (options.namespace) {
    const rows = await db
      .select({
        kind: importEntityMap.externalKind,
        externalId: importEntityMap.externalId,
        localId: importEntityMap.localId,
      })
      .from(importEntityMap)
      .where(
        and(
          eq(importEntityMap.teamId, teamId),
          eq(importEntityMap.source, options.namespace),
          inArray(importEntityMap.externalKind, [`issue`, `board`])
        )
      )
    for (const row of rows) {
      if (row.kind === `issue`) importedIssueKeys.add(row.externalId)
      else importedBoards.set(row.externalId, row.localId)
    }
  }

  return {
    teamId,
    boards: boardRows.map((row) => ({
      id: row.id,
      name: row.name,
      prefix: row.prefix,
      archived: row.archivedAt !== null,
      issueCount: Number(row.issueCount),
      numbers: numbersByBoard.get(row.id),
    })),
    statuses: statusRows.map((row) => ({
      id: row.id,
      name: row.name,
      category: row.category as IssueStatusCategory,
      builtinKey: (row.builtinKey ?? null) as IssueStatus | null,
      color: row.color,
    })),
    labels: labelRows,
    members: memberRows.map((row) => ({
      userId: row.userId,
      email: row.email ?? ``,
      name: row.name ?? ``,
    })),
    pendingInviteEmails: inviteRows.map((row) => row.email!).filter(Boolean),
    canInvite,
    storage,
    importedIssueKeys,
    importedBoards,
    estimationType: (teamRow?.estimationType ?? `none`) as IssueEstimation,
  }
}

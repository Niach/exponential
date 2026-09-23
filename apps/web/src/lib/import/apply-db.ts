// EXP-630: the drizzle implementation of `ApplyPorts` — the only file in the
// import that writes issue data. Boards, statuses, labels and invites go
// through their tRPC routers via `createCaller` with a synthetic context for
// the job's creator (the MCP layer's reuse pattern, lib/mcp/tools.ts
// buildCtx), so every validation those routers own (prefix/slug rules, the
// started-status cap and advisory lock, CI-unique names, the seat gate)
// applies unchanged. The batch write mirrors createWidgetSubmission
// (lib/widget/service.ts): objects uploaded first, then ONE transaction,
// uploads rolled back on failure — plus the transaction-local
// preserve-timestamps guard (0001_triggers.sql §1/§3) so the rows keep their
// source stamps. Deliberately NO notifications, mentions or subscriptions
// beyond the mapped assignee/creator: importing 1,300 issues must not fan
// out 1,300 emails.
import { randomUUID } from "crypto"
import { and, eq, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import {
  attachments,
  comments,
  importEntityMap,
  importJobs,
  issueEvents,
  issueLabels,
  issues,
} from "@/db/schema"
import { boardIconValues, type BoardIcon } from "@/lib/domain"
import { router, type Context } from "@/lib/trpc"
import { boardsRouter } from "@/lib/trpc/boards"
import { statusesRouter } from "@/lib/trpc/statuses"
import { labelsRouter } from "@/lib/trpc/labels"
import { teamInvitesRouter } from "@/lib/trpc/team-invites"
import { assertWithinStorageLimit } from "@/lib/billing"
import { deleteObject, uploadObject } from "@/lib/storage"
import {
  buildAttachmentStorageKey,
  canonicalizeContentType,
  isAcceptedImageContentType,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import {
  canonicalizeRelation,
  insertRelationInTx,
  syncDuplicateMirror,
} from "@/lib/issue-relations"
import { assertNoRelationCycle } from "@/lib/relation-cycles"
import {
  ImportAborted,
  type ApplyPorts,
  type ApplyTeamState,
  type FetchedAsset,
  type MapRowInput,
  type PlannedLink,
} from "@/lib/import/apply"
import type { ImportEntityKind, ImportProgress } from "@/lib/import/bundle"
import type { PlannedIssueWrite } from "@/lib/import/issue-write"
import type { ImportSource } from "@/lib/import/source-types"
import { loadImportTeamState } from "@/lib/import/team-state"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

export interface ImportUser {
  id: string
  email: string
  name: string
  image: string | null
  emailVerified: boolean
  createdAt: Date
  updatedAt: Date
}

// The routers only read `ctx.session.user.id` and `ctx.db`; the rest keeps
// the Context type honest. `viaMcp` stays unset — nothing here is an agent.
export function buildImportContext(user: ImportUser): Context {
  const now = new Date()
  return {
    db,
    request: new Request(`http://import.local/`),
    session: {
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        image: user.image,
        emailVerified: user.emailVerified,
        createdAt: user.createdAt,
        updatedAt: user.updatedAt,
      },
      session: {
        id: `import`,
        userId: user.id,
        token: `import`,
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000),
        createdAt: now,
        updatedAt: now,
        ipAddress: null,
        userAgent: `import`,
      },
    },
  } as unknown as Context
}

const importCaller = router({
  boards: boardsRouter,
  statuses: statusesRouter,
  labels: labelsRouter,
  teamInvites: teamInvitesRouter,
})

// The preserve-timestamps guard (0001_triggers.sql): transaction-local, so a
// pooled connection never leaks it.
export async function setPreserveTimestamps(tx: Tx): Promise<void> {
  await tx.execute(sql`SELECT set_config('exponential.preserve_timestamps', 'on', true)`)
}

export interface DbPortsArgs {
  job: { id: string; teamId: string; credential: string | null; claimToken: string }
  namespace: string
  importer: ImportUser
  source: ImportSource
}

export function createDbApplyPorts(args: DbPortsArgs): ApplyPorts {
  const { job, namespace, importer, source } = args
  const caller = importCaller.createCaller(buildImportContext(importer))

  // Every write while running carries the claim: zero rows = another worker
  // owns the job now (or it was cancelled) and this one must stop.
  async function fencedUpdate(set: Partial<typeof importJobs.$inferInsert>): Promise<boolean> {
    const rows = await db
      .update(importJobs)
      .set(set)
      .where(
        and(
          eq(importJobs.id, job.id),
          eq(importJobs.status, `running`),
          eq(importJobs.claimToken, job.claimToken)
        )
      )
      .returning({ id: importJobs.id })
    return rows.length > 0
  }

  async function setPendingKeys(keys: string[]): Promise<void> {
    const rows = await db
      .update(importJobs)
      .set({
        progress: sql`coalesce(${importJobs.progress}, '{}'::jsonb) || jsonb_build_object('pendingKeys', ${JSON.stringify(keys)}::jsonb)`,
      })
      .where(
        and(
          eq(importJobs.id, job.id),
          eq(importJobs.status, `running`),
          eq(importJobs.claimToken, job.claimToken)
        )
      )
      .returning({ id: importJobs.id })
    if (rows.length === 0) throw new ImportAborted(`claim_lost`)
  }

  return {
    async loadTeamState(): Promise<ApplyTeamState> {
      const state = await loadImportTeamState(job.teamId, { namespace })
      return {
        boards: state.boards.map(({ id, name, prefix }) => ({ id, name, prefix })),
        statuses: state.statuses,
        labels: state.labels,
        members: state.members,
        pendingInviteEmails: state.pendingInviteEmails,
      }
    },

    async loadMapped(kind: ImportEntityKind): Promise<Map<string, string>> {
      const rows = await db
        .select({ externalId: importEntityMap.externalId, localId: importEntityMap.localId })
        .from(importEntityMap)
        .where(
          and(
            eq(importEntityMap.teamId, job.teamId),
            eq(importEntityMap.source, namespace),
            eq(importEntityMap.externalKind, kind)
          )
        )
      return new Map(rows.map((row) => [row.externalId, row.localId]))
    },

    async recordMap(rows: MapRowInput[]): Promise<void> {
      if (rows.length === 0) return
      await db
        .insert(importEntityMap)
        .values(
          rows.map((row) => ({
            jobId: job.id,
            teamId: job.teamId,
            source: namespace,
            externalKind: row.kind,
            externalId: row.externalId,
            externalRef: row.externalRef,
            localId: row.localId,
          }))
        )
        .onConflictDoNothing()
    },

    async createBoard(input) {
      const icon =
        input.icon && (boardIconValues as readonly string[]).includes(input.icon)
          ? (input.icon as BoardIcon)
          : null
      const { board } = await caller.boards.create({
        teamId: job.teamId,
        name: input.name,
        prefix: input.prefix,
        icon,
      })
      return { id: board.id }
    },

    async createStatus(input) {
      const { status } = await caller.statuses.create({
        teamId: job.teamId,
        category: input.category,
        name: input.name,
        color: input.color,
      })
      return { id: status.id, name: status.name }
    },

    async createLabel(input) {
      const { label } = await caller.labels.create({
        teamId: job.teamId,
        name: input.name,
        color: input.color,
      })
      return { id: label.id }
    },

    async createInvite(email) {
      await caller.teamInvites.create({ teamId: job.teamId, email })
    },

    fetchAsset(ref) {
      return source.fetchAsset({ credential: job.credential, ref })
    },

    async writeBatch({ writes, assets }) {
      const uploaded: string[] = []
      type Stored = {
        storageKey: string
        filename: string
        contentType: string
        sizeBytes: number
        width: number | null
        height: number | null
      }
      const stored = new Map<string, Stored>()
      const pending: { attachmentId: string; asset: FetchedAsset; storageKey: string; filename: string }[] = []
      for (const write of writes) {
        for (const attachment of write.attachments) {
          const asset = assets.get(attachment.id)
          if (!asset) continue
          const filename = sanitizeUploadFilename(
            asset.filename ?? attachment.filename,
            `file`
          )
          pending.push({
            attachmentId: attachment.id,
            asset,
            filename,
            storageKey: buildAttachmentStorageKey(write.issue.id, attachment.id, filename),
          })
        }
      }
      const totalBytes = pending.reduce((sum, row) => sum + row.asset.bytes.byteLength, 0)
      try {
        if (pending.length > 0) {
          await assertWithinStorageLimit(job.teamId, totalBytes)
          await setPendingKeys(pending.map((row) => row.storageKey))
        }
        for (const row of pending) {
          const contentType = canonicalizeContentType(row.asset.contentType)
          await uploadObject({
            body: row.asset.bytes,
            contentLength: row.asset.bytes.byteLength,
            contentType,
            key: row.storageKey,
          })
          uploaded.push(row.storageKey)
          const dims = isAcceptedImageContentType(contentType)
            ? getImageDimensions(row.asset.bytes)
            : null
          stored.set(row.attachmentId, {
            storageKey: row.storageKey,
            filename: row.filename,
            contentType,
            sizeBytes: row.asset.bytes.byteLength,
            width: dims?.width ?? null,
            height: dims?.height ?? null,
          })
        }

        await db.transaction(async (tx) => {
          await setPreserveTimestamps(tx)
          for (const write of writes) {
            await insertPlannedIssue(tx, write, stored, { jobId: job.id, namespace })
          }
        })

        if (pending.length > 0) await setPendingKeys([])
      } catch (err) {
        for (const key of uploaded) {
          try {
            await deleteObject(key)
          } catch (deleteError) {
            console.error(`[import] failed to roll back uploaded object`, deleteError)
          }
        }
        throw err
      }
    },

    async linkRelations(links: PlannedLink[]) {
      let written = 0
      const warnings: string[] = []
      for (const link of links) {
        try {
          await db.transaction(async (tx) => {
            await setPreserveTimestamps(tx)
            if (link.type === `duplicate`) {
              const rows = await tx
                .update(issues)
                .set({
                  duplicateOfId: link.relatedIssueId,
                  status: `duplicate`,
                  statusId: sql`(select id from issue_statuses where team_id = ${job.teamId} and builtin_key = 'duplicate' limit 1)`,
                })
                .where(
                  and(
                    eq(issues.id, link.issueId),
                    sql`${issues.duplicateOfId} is distinct from ${link.relatedIssueId}`
                  )
                )
                .returning({ id: issues.id })
              await syncDuplicateMirror(tx, {
                issueId: link.issueId,
                teamId: job.teamId,
                actorUserId: null,
                previousDuplicateOfId: null,
                nextDuplicateOfId: link.relatedIssueId,
              })
              if (rows.length > 0) written += 1
              return
            }
            const canonical = canonicalizeRelation(link.issueId, link.relatedIssueId, link.type)
            if (canonical.type === `blocks`) {
              await assertNoRelationCycle(tx, {
                issueId: canonical.issueId,
                relatedIssueId: canonical.relatedIssueId,
                type: `blocks`,
              })
            }
            const row = await insertRelationInTx(tx, {
              ...canonical,
              source: `user`,
              teamId: job.teamId,
              actorUserId: null,
            })
            if (row) written += 1
          })
        } catch (err) {
          const message = err instanceof TRPCError || err instanceof Error ? err.message : String(err)
          warnings.push(`${link.externalRef}: ${link.type} relation not written (${message}).`)
        }
      }
      return { written, warnings }
    },

    async reportProgress(progress: ImportProgress): Promise<boolean> {
      return fencedUpdate({ progress, claimedAt: new Date() })
    },

    async shouldStop(): Promise<boolean> {
      const [row] = await db
        .select({ status: importJobs.status })
        .from(importJobs)
        .where(eq(importJobs.id, job.id))
      return !row || row.status !== `running`
    },

    newId: () => randomUUID(),
  }
}

async function insertPlannedIssue(
  tx: Tx,
  write: PlannedIssueWrite,
  stored: ReadonlyMap<
    string,
    {
      storageKey: string
      filename: string
      contentType: string
      sizeBytes: number
      width: number | null
      height: number | null
    }
  >,
  meta: { jobId: string; namespace: string }
): Promise<void> {
  const row = write.issue
  await tx.insert(issues).values({
    id: row.id,
    boardId: row.boardId,
    teamId: row.teamId,
    // 0 = let generate_issue_number allocate; >0 = the preserved source
    // number (the trigger clamps the counter past it).
    number: row.number ?? 0,
    title: row.title,
    description: row.description,
    status: row.status,
    statusId: row.statusId,
    priority: row.priority,
    assigneeId: row.assigneeId,
    creatorId: row.creatorId,
    source: `user`,
    dueDate: row.dueDate,
    completedAt: row.completedAt,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  })

  for (const comment of write.comments) {
    await tx.insert(comments).values({
      id: comment.id,
      issueId: comment.issueId,
      teamId: comment.teamId,
      boardId: comment.boardId,
      authorId: comment.authorId,
      parentId: comment.parentId,
      source: `user`,
      body: comment.body,
      editedAt: comment.editedAt,
      createdAt: comment.createdAt,
      updatedAt: comment.updatedAt,
    })
  }

  for (const attachment of write.attachments) {
    const file = stored.get(attachment.id)
    if (!file) continue
    await tx.insert(attachments).values({
      id: attachment.id,
      teamId: attachment.teamId,
      boardId: attachment.boardId,
      issueId: attachment.issueId,
      commentId: attachment.commentId,
      uploaderId: null,
      filename: file.filename,
      contentType: file.contentType,
      sizeBytes: file.sizeBytes,
      storageKey: file.storageKey,
      url: attachment.url,
      width: file.width,
      height: file.height,
      createdAt: attachment.createdAt,
      updatedAt: attachment.createdAt,
    })
  }

  if (write.labels.length > 0) {
    await tx.insert(issueLabels).values(write.labels).onConflictDoNothing()
  }

  for (const event of write.events) {
    await tx.insert(issueEvents).values({
      issueId: event.issueId,
      teamId: event.teamId,
      boardId: event.boardId,
      actorUserId: event.actorUserId,
      type: event.type,
      payload: event.payload,
      createdAt: event.createdAt,
      updatedAt: event.createdAt,
    })
  }

  for (const subscriber of write.subscribers) {
    await ensureSubscribed(tx, {
      issueId: row.id,
      userId: subscriber.userId,
      teamId: row.teamId,
      source: subscriber.source,
    })
  }

  await tx
    .insert(importEntityMap)
    .values(
      write.map.map((entry) => ({
        jobId: meta.jobId,
        teamId: row.teamId,
        source: meta.namespace,
        externalKind: entry.kind,
        externalId: entry.externalId,
        externalRef: entry.externalRef,
        localId: entry.localId,
      }))
    )
    .onConflictDoNothing()
}

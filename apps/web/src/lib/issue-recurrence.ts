import { sql } from "drizzle-orm"
import type { Issue } from "@/db/schema"
import { issueLabels, issues } from "@/db/schema"
import {
  addRecurrence,
  formatDateForMutation,
  getIssueDescriptionText,
} from "@/lib/domain"
import { stripMarkdownImages } from "@/lib/storage/issue-attachments"

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

interface CloneIssueForRecurrenceParams {
  sourceIssueId: string
  sourceProjectId: string
  sourceTitle: string
  sourcePriority: Issue[`priority`]
  sourceAssigneeId: Issue[`assigneeId`]
  sourceDescription: Issue[`description`]
  recurrenceInterval: number
  recurrenceUnit: NonNullable<Issue[`recurrenceUnit`]>
  creatorId: string
}

export async function cloneIssueForRecurrence(
  tx: Tx,
  params: CloneIssueForRecurrenceParams
): Promise<Issue> {
  const nextDueDate = formatDateForMutation(
    addRecurrence(new Date(), params.recurrenceInterval, params.recurrenceUnit)
  )

  const sourceDescriptionText = getIssueDescriptionText(params.sourceDescription)
  const clonedDescription = sourceDescriptionText
    ? { text: stripMarkdownImages(sourceDescriptionText) }
    : null

  const [insertedClone] = await tx
    .insert(issues)
    .values({
      projectId: params.sourceProjectId,
      title: params.sourceTitle,
      priority: params.sourcePriority,
      assigneeId: params.sourceAssigneeId,
      description: clonedDescription,
      status: `todo`,
      dueDate: nextDueDate,
      recurrenceInterval: params.recurrenceInterval,
      recurrenceUnit: params.recurrenceUnit,
      creatorId: params.creatorId,
    })
    .returning()

  await tx.execute(sql`
    INSERT INTO ${issueLabels} (issue_id, label_id, workspace_id)
    SELECT ${insertedClone.id}::uuid, label_id, workspace_id
    FROM ${issueLabels}
    WHERE issue_id = ${params.sourceIssueId}::uuid
  `)

  return insertedClone
}

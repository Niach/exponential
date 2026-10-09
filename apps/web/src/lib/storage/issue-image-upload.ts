export interface UploadedIssueAttachment {
  asFile?: boolean
  contentType: string
  filename: string
  height: number | null
  id: string
  sizeBytes: number
  url: string
  width: number | null
}

async function postIssueUpload(
  path: string,
  file: File,
  fallbackMessage: string,
  asFile = false
) {
  const formData = new FormData()
  formData.append(`file`, file)
  if (asFile) formData.append(`asFile`, `1`)

  const response = await fetch(path, {
    method: `POST`,
    body: formData,
    credentials: `same-origin`,
  })

  const result = (await response.json()) as
    | { error?: string }
    | UploadedIssueAttachment

  if (!response.ok || !(`url` in result)) {
    const message =
      `error` in result && typeof result.error === `string`
        ? result.error
        : fallbackMessage

    throw new Error(message)
  }

  return result
}

/**
 * Steer-image upload (EXP-702): EVERY steered image goes to the session's
 * own server-only store — issue runs included, so steering screenshots never
 * land in the issue's Files section. Same request/response contract as the
 * issue route; the server only accepts the inline image types (10 MB) and
 * only from the session's owner.
 */
export async function uploadSessionImageFile(sessionId: string, file: File) {
  return postIssueUpload(
    `/api/sessions/${sessionId}/files`,
    file,
    `Failed to upload image`
  )
}

/**
 * EXP-825: an image attached to a START — the Agent page composer — before
 * any session exists. Lands in the team's pending store; the device binds it
 * to the session row it creates (or the orphan sweep reclaims it).
 */
export async function uploadTeamSessionImageFile(teamId: string, file: File) {
  return postIssueUpload(
    `/api/teams/${teamId}/session-files`,
    file,
    `Failed to upload image`
  )
}

export interface AttachmentUploadOptions {
  /** EXP-1247: picked through a FILE/paperclip path — the row lists in Files
   *  even when it is an image, and is never inlined. */
  asFile?: boolean
}

/**
 * The ONE issue upload (EXP-297 `/files`): any type, 10 MB for the five
 * inline image types, 50 MB otherwise. Inline images ride `![](…)`; every
 * other row (and every `asFile` row) renders from the synced attachments
 * collection in the issue's Files section.
 */
export async function uploadIssueAttachment(
  issueId: string,
  file: File,
  { asFile = false }: AttachmentUploadOptions = {}
) {
  return postIssueUpload(
    `/api/issues/${issueId}/files`,
    file,
    asFile ? `Failed to upload file` : `Failed to upload image`,
    asFile
  )
}


/** EXP-878: the upload path of an issue DRAFT — same contract as the issue
 *  route, owner-only. `media-upload.ts` builds the same path for clips. */
export function draftUploadPath(draftId: string) {
  return `/api/issue-drafts/${draftId}/files`
}

/**
 * EXP-878: the ONE draft upload of the New issue page (EXP-1170). Uploads are
 * eager there — the row exists before the issue does (`attachments.draft_id`),
 * so an inline image carries its final `/api/attachments/{id}` URL from the
 * moment it lands, and Files rows are reparented onto the issue by
 * `issues.create({ draftId })`.
 */
export async function uploadDraftAttachment(
  draftId: string,
  file: File,
  { asFile = false }: AttachmentUploadOptions = {}
) {
  return postIssueUpload(
    draftUploadPath(draftId),
    file,
    asFile ? `Failed to upload file` : `Failed to upload image`,
    asFile
  )
}

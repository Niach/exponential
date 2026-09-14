export interface UploadedIssueAttachment {
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
  fallbackMessage: string
) {
  const formData = new FormData()
  formData.append(`file`, file)

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
 * Inline-image upload. Since EXP-297 this posts to the any-type `/files`
 * route — identical request/response contract; the server applies the 10 MB
 * ceiling only to the five accepted inline image types (everything else gets
 * the 50 MB file cap), so callers must pre-filter to accepted image types.
 */
export async function uploadIssueImageFile(issueId: string, file: File) {
  return postIssueUpload(
    `/api/issues/${issueId}/files`,
    file,
    `Failed to upload image`
  )
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

/**
 * Arbitrary-file upload (EXP-297): 50 MB for non-images, 10 MB for the inline
 * image types. Non-image rows never enter markdown — they render from the
 * synced attachments collection in the issue's Files section.
 */
export async function uploadIssueFile(issueId: string, file: File) {
  return postIssueUpload(
    `/api/issues/${issueId}/files`,
    file,
    `Failed to upload file`
  )
}

/** EXP-878: the upload path of an issue DRAFT — same contract as the issue
 *  route, owner-only. `media-upload.ts` builds the same path for clips. */
export function draftUploadPath(draftId: string) {
  return `/api/issue-drafts/${draftId}/files`
}

/**
 * EXP-878: an image pasted/dropped into the CREATE dialog. Uploads are eager
 * there — the row exists before the issue does (`attachments.draft_id`), so
 * the description carries the final `/api/attachments/{id}` URL from the
 * moment the image lands, exactly like the issue-detail editor.
 */
export async function uploadDraftImageFile(draftId: string, file: File) {
  return postIssueUpload(
    draftUploadPath(draftId),
    file,
    `Failed to upload image`
  )
}

/** EXP-878: a non-image attachment on a draft — the create dialog's Files
 *  rail, reparented onto the issue by `issues.create({ draftId })`. */
export async function uploadDraftFile(draftId: string, file: File) {
  return postIssueUpload(draftUploadPath(draftId), file, `Failed to upload file`)
}

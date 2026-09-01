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
 * Steer-image upload for coding sessions WITHOUT an issue (EXP-702: chat,
 * action and batch runs). Same request/response contract as the issue route;
 * the server only accepts the inline image types (10 MB) and only from the
 * session's owner.
 */
export async function uploadSessionImageFile(sessionId: string, file: File) {
  return postIssueUpload(
    `/api/sessions/${sessionId}/files`,
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

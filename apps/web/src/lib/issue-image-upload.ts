export interface UploadedIssueImage {
  contentType: string
  filename: string
  id: string
  sizeBytes: number
  url: string
}

export async function uploadIssueImageFile(issueId: string, file: File) {
  const formData = new FormData()
  formData.append(`file`, file)

  const response = await fetch(`/api/issues/${issueId}/images`, {
    method: `POST`,
    body: formData,
    credentials: `same-origin`,
  })

  const result = (await response.json()) as
    | { error?: string }
    | UploadedIssueImage

  if (!response.ok || !(`url` in result)) {
    const message =
      `error` in result && typeof result.error === `string`
        ? result.error
        : `Failed to upload image`

    throw new Error(message)
  }

  return result
}

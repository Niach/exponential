import {
  File as FileIcon,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileSpreadsheet,
  FileText,
  FilePlay,
  type LucideIcon,
} from "lucide-react"
import {
  isAcceptedImageContentType,
  isAudioContentType,
  isVideoContentType,
} from "@/lib/storage/issue-attachments"

/**
 * What the classifiers read: a bare content type (a local `File.type` before
 * upload) or a stored row, whose EXP-1247 `asFile` marker wins over its type.
 */
export type AttachmentClassInput =
  | string
  | { contentType: string; asFile?: boolean | null }

function contentTypeOf(input: AttachmentClassInput) {
  return typeof input === `string` ? input : input.contentType
}

function markedAsFile(input: AttachmentClassInput) {
  return typeof input !== `string` && input.asFile === true
}

/**
 * EXP-297 classification rule, shared by every client: a row is an INLINE
 * IMAGE iff its content type is one of the five accepted raster types (they
 * ride the `![](…)` markdown pipeline). Everything else — including other
 * `image/*` types like tiff — belongs in the Files section, except the
 * EXP-824 inline media classes below. EXP-1247: a row uploaded through a
 * FILE/paperclip path (`asFile`) is never inline, whatever its type.
 */
export function isInlineImageAttachment(input: AttachmentClassInput) {
  return !markedAsFile(input) && isAcceptedImageContentType(contentTypeOf(input))
}

/**
 * EXP-824: a `video/*` row is an INLINE VIDEO — embedded as a plain link
 * `[clip.mp4](/api/attachments/{id})` and rendered as a player (poster +
 * controls). Mirrored ×4 like isInlineImageAttachment.
 */
export function isInlineVideoAttachment(input: AttachmentClassInput) {
  return !markedAsFile(input) && isVideoContentType(contentTypeOf(input))
}

/** `audio/*` rides the same link form and renders as an audio player. */
export function isInlineAudioAttachment(input: AttachmentClassInput) {
  return !markedAsFile(input) && isAudioContentType(contentTypeOf(input))
}

/** Third class beside inline image and file: rows that render inline media. */
export function isInlineMediaAttachment(input: AttachmentClassInput) {
  return isInlineVideoAttachment(input) || isInlineAudioAttachment(input)
}

/** True for rows that belong in the Files rail: neither inline image nor
 *  media, OR marked `asFile` (EXP-1247). */
export function isFileAttachment(input: AttachmentClassInput) {
  return !isInlineImageAttachment(input) && !isInlineMediaAttachment(input)
}

const archiveTypes = new Set([
  `application/zip`,
  `application/x-zip-compressed`,
  `application/gzip`,
  `application/x-tar`,
  `application/x-7z-compressed`,
  `application/vnd.rar`,
  `application/x-rar-compressed`,
])

const codeTypes = new Set([
  `application/json`,
  `application/xml`,
  `text/xml`,
  `text/html`,
  `text/css`,
  `text/javascript`,
  `application/javascript`,
])

const spreadsheetTypes = new Set([
  `text/csv`,
  `application/vnd.ms-excel`,
  `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`,
  `application/vnd.oasis.opendocument.spreadsheet`,
])

/** Type-family glyph for an attachment row (files rail + storage manager). */
export function getAttachmentIcon(contentType: string): LucideIcon {
  const essence = contentType.split(`;`)[0]?.trim().toLowerCase() ?? ``

  if (essence.startsWith(`image/`)) return FileImage
  if (essence.startsWith(`video/`)) return FilePlay
  if (essence.startsWith(`audio/`)) return FileAudio
  if (archiveTypes.has(essence)) return FileArchive
  if (spreadsheetTypes.has(essence)) return FileSpreadsheet
  if (codeTypes.has(essence)) return FileCode
  if (essence === `application/pdf` || essence.startsWith(`text/`)) {
    return FileText
  }

  return FileIcon
}

/**
 * EXP-955: a Files-rail row that previews IN the app through the markdown
 * renderer instead of opening the byte route (which the browser would only
 * download — `text/markdown` is not something it renders). Matched by the
 * stored type, or by the extension for the browsers that upload `.md` with
 * an empty / generic type (Safari: octet-stream, some pickers: text/plain).
 */
export function isMarkdownAttachment(contentType: string, filename: string) {
  const essence = contentType.split(`;`)[0]?.trim().toLowerCase() ?? ``
  if (essence === `text/markdown` || essence === `text/x-markdown`) return true
  if (
    essence !== `` &&
    essence !== `text/plain` &&
    essence !== `application/octet-stream`
  ) {
    return false
  }
  return /\.(md|markdown)$/i.test(filename.trim())
}

/**
 * The largest markdown file the preview will fetch and render. A README is
 * kilobytes; anything bigger is a download, not a dialog.
 */
export const MARKDOWN_PREVIEW_MAX_BYTES = 1024 * 1024

/** Compact human size for attachment rows (1 KB = 1024 B). */
export function formatAttachmentSize(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return `0 B`
  if (bytes < 1024) return `${Math.round(bytes)} B`

  const units = [`KB`, `MB`, `GB`, `TB`]
  let value = bytes / 1024
  let unitIndex = 0

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  const rounded = value >= 10 ? Math.round(value) : Math.round(value * 10) / 10
  return `${rounded} ${units[unitIndex]}`
}

/** Byte-route URL that always forces a download rather than inline render. */
export function buildAttachmentDownloadUrl(url: string) {
  return url.includes(`?`) ? `${url}&download=1` : `${url}?download=1`
}

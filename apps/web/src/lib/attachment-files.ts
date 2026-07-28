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
import { isAcceptedImageContentType } from "@/lib/storage/issue-attachments"

/**
 * EXP-297 classification rule, shared by every client: a row is an INLINE
 * IMAGE iff its content type is one of the five accepted raster types (they
 * ride the `![](…)` markdown pipeline). Everything else — including other
 * `image/*` types like tiff — belongs in the Files section.
 */
export function isInlineImageAttachment(contentType: string) {
  return isAcceptedImageContentType(contentType)
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

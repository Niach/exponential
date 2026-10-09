import { describe, expect, it } from "vitest"
import {
  formatAttachmentSize,
  isFileAttachment,
  isInlineImageAttachment,
  isInlineMediaAttachment,
  isMarkdownAttachment,
} from "@/lib/attachment-files"

describe(`isMarkdownAttachment (EXP-955)`, () => {
  it(`matches the markdown types regardless of parameters`, () => {
    expect(isMarkdownAttachment(`text/markdown`, `notes.md`)).toBe(true)
    expect(isMarkdownAttachment(`text/markdown; charset=utf-8`, `x`)).toBe(
      true
    )
    expect(isMarkdownAttachment(`text/x-markdown`, `x.txt`)).toBe(true)
  })

  it(`falls back to the extension for empty and generic types`, () => {
    // Safari uploads .md as octet-stream, some pickers as text/plain.
    expect(isMarkdownAttachment(``, `README.md`)).toBe(true)
    expect(isMarkdownAttachment(`application/octet-stream`, `a.MD`)).toBe(true)
    expect(isMarkdownAttachment(`text/plain`, `spec.markdown`)).toBe(true)
    expect(isMarkdownAttachment(`text/plain`, `spec.txt`)).toBe(false)
  })

  it(`never claims a file of a specific other type`, () => {
    expect(isMarkdownAttachment(`application/pdf`, `spec.md`)).toBe(false)
    expect(isMarkdownAttachment(`image/png`, `x.md`)).toBe(false)
    expect(isMarkdownAttachment(`text/csv`, `x.md`)).toBe(false)
  })
})

describe(`formatAttachmentSize`, () => {
  it(`renders a synced byte count`, () => {
    expect(formatAttachmentSize(22345)).toBe(`22 KB`)
    expect(formatAttachmentSize(512)).toBe(`512 B`)
    expect(formatAttachmentSize(0)).toBe(`0 B`)
  })
})

// EXP-1247: a stored row's `asFile` marker wins over its content type.
describe(`attachment classification (EXP-1247)`, () => {
  it(`classifies a bare content type as before`, () => {
    expect(isInlineImageAttachment(`image/png`)).toBe(true)
    expect(isInlineMediaAttachment(`video/mp4`)).toBe(true)
    expect(isFileAttachment(`image/png`)).toBe(false)
    expect(isFileAttachment(`application/pdf`)).toBe(true)
    expect(isFileAttachment(`image/tiff`)).toBe(true)
  })

  it(`lists an asFile image or clip as a file, never inline`, () => {
    const png = { contentType: `image/png`, asFile: true }
    const clip = { contentType: `video/mp4`, asFile: true }
    expect(isInlineImageAttachment(png)).toBe(false)
    expect(isFileAttachment(png)).toBe(true)
    expect(isInlineMediaAttachment(clip)).toBe(false)
    expect(isFileAttachment(clip)).toBe(true)
  })

  it(`keeps an unmarked row on its content type`, () => {
    expect(isInlineImageAttachment({ contentType: `image/png`, asFile: false })).toBe(true)
    expect(isFileAttachment({ contentType: `image/png`, asFile: null })).toBe(false)
    expect(isFileAttachment({ contentType: `application/zip` })).toBe(true)
  })
})

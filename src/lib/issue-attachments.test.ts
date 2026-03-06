import { describe, expect, it } from "vitest"
import {
  extractAttachmentIdsFromDescription,
  extractMarkdownImageUrls,
  getRemovedAttachmentIds,
  hasMarkdownImages,
} from "@/lib/issue-attachments"

describe(`issue attachment helpers`, () => {
  it(`extracts markdown image urls`, () => {
    expect(
      extractMarkdownImageUrls(
        `Before ![one](/api/attachments/11111111-1111-1111-1111-111111111111) after`
      )
    ).toEqual([`/api/attachments/11111111-1111-1111-1111-111111111111`])
  })

  it(`accepts relative and same-origin attachment urls`, () => {
    expect(
      extractAttachmentIdsFromDescription(
        [
          `![one](/api/attachments/11111111-1111-1111-1111-111111111111)`,
          `![two](https://app.test/api/attachments/22222222-2222-2222-2222-222222222222)`,
        ].join(`\n`),
        `https://app.test/api/trpc`
      )
    ).toEqual({
      attachmentIds: [
        `11111111-1111-1111-1111-111111111111`,
        `22222222-2222-2222-2222-222222222222`,
      ],
      invalidUrls: [],
    })
  })

  it(`rejects external image urls`, () => {
    expect(
      extractAttachmentIdsFromDescription(
        `![one](https://cdn.example.com/cat.png)`,
        `https://app.test/api/trpc`
      )
    ).toEqual({
      attachmentIds: [],
      invalidUrls: [`https://cdn.example.com/cat.png`],
    })
  })

  it(`calculates removed attachment ids`, () => {
    expect(
      getRemovedAttachmentIds(
        [
          `![one](/api/attachments/11111111-1111-1111-1111-111111111111)`,
          `![two](/api/attachments/22222222-2222-2222-2222-222222222222)`,
        ].join(`\n`),
        `![two](/api/attachments/22222222-2222-2222-2222-222222222222)`,
        `https://app.test/api/trpc`
      )
    ).toEqual([`11111111-1111-1111-1111-111111111111`])
  })

  it(`detects markdown images`, () => {
    expect(hasMarkdownImages(`Plain text`)).toBe(false)
    expect(
      hasMarkdownImages(`![image](/api/attachments/11111111-1111-1111-1111-111111111111)`)
    ).toBe(true)
  })
})

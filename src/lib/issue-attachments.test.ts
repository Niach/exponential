import { describe, expect, it } from "vitest"
import {
  collectMarkdownImageUrls,
  extractMarkdownImageOccurrences,
  extractAttachmentIdsFromDescription,
  extractMarkdownImageUrls,
  getRemovedAttachmentIds,
  hasMarkdownImages,
  removeMarkdownImageByOccurrence,
  removeMarkdownImagesByUrl,
  replaceMarkdownImageUrls,
} from "@/lib/issue-attachments"

describe(`issue attachment helpers`, () => {
  it(`extracts markdown image urls`, () => {
    expect(
      extractMarkdownImageUrls(
        `Before ![one](/api/attachments/11111111-1111-1111-1111-111111111111) after`
      )
    ).toEqual([`/api/attachments/11111111-1111-1111-1111-111111111111`])
  })

  it(`collects markdown image urls in first-occurrence order`, () => {
    expect(
      collectMarkdownImageUrls(
        [
          `![one](blob:one)`,
          `![two](blob:two)`,
          `![one-again](blob:one)`,
        ].join(`\n`)
      )
    ).toEqual([`blob:one`, `blob:two`])
  })

  it(`extracts ordered markdown image occurrences with duplicate urls`, () => {
    expect(
      extractMarkdownImageOccurrences(
        [
          `Before`,
          `![first](blob:shared)`,
          `![second](blob:shared)`,
          `![third](blob:other)`,
        ].join(`\n`)
      )
    ).toEqual([
      expect.objectContaining({
        alt: `first`,
        occurrenceIndex: 0,
        url: `blob:shared`,
      }),
      expect.objectContaining({
        alt: `second`,
        occurrenceIndex: 1,
        url: `blob:shared`,
      }),
      expect.objectContaining({
        alt: `third`,
        occurrenceIndex: 2,
        url: `blob:other`,
      }),
    ])
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

  it(`removes only selected markdown image urls`, () => {
    expect(
      removeMarkdownImagesByUrl(
        [
          `Before text`,
          `![draft](blob:draft-image)`,
          `![kept](https://cdn.example.com/keep.png)`,
          `After text`,
        ].join(`\n`),
        [`blob:draft-image`]
      )
    ).toBe([`Before text`, ``, `![kept](https://cdn.example.com/keep.png)`, `After text`].join(`\n`))
  })

  it(`removes only the targeted markdown image occurrence`, () => {
    expect(
      removeMarkdownImageByOccurrence(
        [
          `Before text`,
          `![first](blob:shared)`,
          `![second](blob:shared)`,
          `After text`,
        ].join(`\n`),
        0
      )
    ).toBe([`Before text`, ``, `![second](blob:shared)`, `After text`].join(`\n`))
  })

  it(`replaces only selected markdown image urls`, () => {
    expect(
      replaceMarkdownImageUrls(
        [
          `![draft](blob:draft-image)`,
          `![kept](https://cdn.example.com/keep.png)`,
        ].join(`\n`),
        new Map([[`blob:draft-image`, `/api/attachments/33333333-3333-3333-3333-333333333333`]])
      )
    ).toBe(
      [
        `![draft](/api/attachments/33333333-3333-3333-3333-333333333333)`,
        `![kept](https://cdn.example.com/keep.png)`,
      ].join(`\n`)
    )
  })

  it(`detects markdown images`, () => {
    expect(hasMarkdownImages(`Plain text`)).toBe(false)
    expect(
      hasMarkdownImages(`![image](/api/attachments/11111111-1111-1111-1111-111111111111)`)
    ).toBe(true)
  })
})

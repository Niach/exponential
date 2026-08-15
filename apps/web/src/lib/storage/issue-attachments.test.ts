import { describe, expect, it } from "vitest"
import {
  buildContentDispositionHeader,
  buildDeletedAttachmentPlaceholder,
  getMaxUploadBytesForContentType,
  isInlineSafeContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
  replaceAttachmentReferencesWithPlaceholder,
  canonicalizeMarkdownImageUrls,
  collectMarkdownImageUrls,
  collectReferencedAttachmentIds,
  extractMarkdownImageOccurrences,
  extractAttachmentIdsFromDescription,
  extractMarkdownImageUrls,
  getAttachmentImageWidthFromUrl,
  canonicalizeContentType,
  hasMarkdownImages,
  removeMarkdownImageByOccurrence,
  removeMarkdownImagesByUrl,
  replaceMarkdownImageUrls,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"

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
        [`![one](blob:one)`, `![two](blob:two)`, `![one-again](blob:one)`].join(
          `\n`
        )
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

  it(`parses serializer-escaped alt text and unescapes it`, () => {
    // TipTap's prosemirror-markdown serializer escapes markdown punctuation in
    // alt text, so a `shot [1].png` filename round-trips as `\[1\]` (REV-6).
    expect(
      extractMarkdownImageOccurrences(`![shot \\[1\\].png](blob:draft)`)
    ).toEqual([
      expect.objectContaining({
        alt: `shot [1].png`,
        markdown: `![shot \\[1\\].png](blob:draft)`,
        url: `blob:draft`,
      }),
    ])
    expect(collectMarkdownImageUrls(`![shot \\[1\\].png](blob:draft)`)).toEqual(
      [`blob:draft`]
    )
    expect(hasMarkdownImages(`![shot \\[1\\].png](blob:draft)`)).toBe(true)
  })

  it(`parses alt text ending in an escaped backslash`, () => {
    expect(
      extractMarkdownImageOccurrences(`![trailing\\\\](blob:draft)`)
    ).toEqual([
      expect.objectContaining({ alt: `trailing\\`, url: `blob:draft` }),
    ])
  })

  it(`removes images with escaped alt text by url`, () => {
    expect(
      removeMarkdownImagesByUrl(`before\n![shot \\[1\\].png](blob:draft)`, [
        `blob:draft`,
      ])
    ).toBe(`before\n`)
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

  it(`extracts a lowercase id from an uppercase attachment URL`, () => {
    expect(
      extractAttachmentIdsFromDescription(
        `![shout](/api/attachments/11111111-1111-1111-1111-11111111111A)`,
        `https://app.test/api/trpc`
      ).attachmentIds
    ).toEqual([`11111111-1111-1111-1111-11111111111a`])
  })

  it(`canonicalizes content types to a lowercase parameter-free essence`, () => {
    expect(canonicalizeContentType(`image/png`)).toBe(`image/png`)
    expect(canonicalizeContentType(`IMAGE/PNG`)).toBe(`image/png`)
    expect(canonicalizeContentType(`image/jpeg; charset=binary`)).toBe(
      `image/jpeg`
    )
    expect(canonicalizeContentType(``)).toBe(`application/octet-stream`)
    expect(canonicalizeContentType(`  ;foo=bar`)).toBe(
      `application/octet-stream`
    )
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
    ).toBe(
      [
        `Before text`,
        ``,
        `![kept](https://cdn.example.com/keep.png)`,
        `After text`,
      ].join(`\n`)
    )
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
    ).toBe(
      [`Before text`, ``, `![second](blob:shared)`, `After text`].join(`\n`)
    )
  })

  it(`replaces only selected markdown image urls`, () => {
    expect(
      replaceMarkdownImageUrls(
        [
          `![draft](blob:draft-image)`,
          `![kept](https://cdn.example.com/keep.png)`,
        ].join(`\n`),
        new Map([
          [
            `blob:draft-image`,
            `/api/attachments/33333333-3333-3333-3333-333333333333`,
          ],
        ])
      )
    ).toBe(
      [
        `![draft](/api/attachments/33333333-3333-3333-3333-333333333333)`,
        `![kept](https://cdn.example.com/keep.png)`,
      ].join(`\n`)
    )
  })

  it(`unions attachment ids across description and comment bodies`, () => {
    expect(
      collectReferencedAttachmentIds(
        [
          `Description ![one](/api/attachments/11111111-1111-1111-1111-111111111111)`,
          `Comment ![two](https://app.test/api/attachments/22222222-2222-2222-2222-222222222222)`,
          `Second comment ![one-again](/api/attachments/11111111-1111-1111-1111-111111111111)`,
        ],
        `https://app.test/api/trpc`
      )
    ).toEqual(
      new Set([
        `11111111-1111-1111-1111-111111111111`,
        `22222222-2222-2222-2222-222222222222`,
      ])
    )
  })

  it(`ignores external and non-attachment urls when collecting referenced ids`, () => {
    expect(
      collectReferencedAttachmentIds(
        [`![x](https://cdn.example.com/cat.png)`, `![y](blob:draft)`],
        `https://app.test/api/trpc`
      )
    ).toEqual(new Set())
  })

  it(`returns an empty set for no texts`, () => {
    expect(
      collectReferencedAttachmentIds([], `https://app.test/api/trpc`)
    ).toEqual(new Set())
  })

  it(`detects markdown images`, () => {
    expect(hasMarkdownImages(`Plain text`)).toBe(false)
    expect(
      hasMarkdownImages(
        `![image](/api/attachments/11111111-1111-1111-1111-111111111111)`
      )
    ).toBe(true)
  })
})

describe(`canonicalizeMarkdownImageUrls`, () => {
  const origin = `https://app.test/api/trpc`
  const id = `11111111-1111-1111-1111-111111111111`

  it(`leaves the bare canonical url unchanged`, () => {
    const text = `Before ![one](/api/attachments/${id}) after`
    expect(canonicalizeMarkdownImageUrls(text, origin)).toBe(text)
  })

  it(`preserves an integer w display-width param`, () => {
    const text = `![one](/api/attachments/${id}?w=480)`
    expect(canonicalizeMarkdownImageUrls(text, origin)).toBe(text)
  })

  it(`strips every query param other than w`, () => {
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?w=480&token=abc&h=200)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id}?w=480)`)
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?token=abc)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id})`)
  })

  it(`drops a non-integer w`, () => {
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?w=abc)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id})`)
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?w=48.5)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id})`)
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?w=-480)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id})`)
  })

  it(`clamps w into sane bounds`, () => {
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?w=5)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id}?w=40)`)
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](/api/attachments/${id}?w=99999)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id}?w=4000)`)
  })

  it(`canonicalizes same-origin absolute urls to relative, preserving w`, () => {
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](https://app.test/api/attachments/${id}?w=480)`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id}?w=480)`)
    expect(
      canonicalizeMarkdownImageUrls(
        `![one](https://app.test/api/attachments/${id})`,
        origin
      )
    ).toBe(`![one](/api/attachments/${id})`)
  })

  it(`leaves external image urls untouched`, () => {
    const text = `![cat](https://cdn.example.com/cat.png?w=480)`
    expect(canonicalizeMarkdownImageUrls(text, origin)).toBe(text)
  })
})

describe(`getAttachmentImageWidthFromUrl`, () => {
  const origin = `https://app.test`

  it(`returns the integer w param`, () => {
    expect(
      getAttachmentImageWidthFromUrl(`/api/attachments/x?w=480`, origin)
    ).toBe(480)
  })

  it(`returns null without a valid w`, () => {
    expect(getAttachmentImageWidthFromUrl(`/api/attachments/x`, origin)).toBe(
      null
    )
    expect(
      getAttachmentImageWidthFromUrl(`/api/attachments/x?w=12px`, origin)
    ).toBe(null)
  })

  it(`clamps to the 40..4000 bounds`, () => {
    expect(getAttachmentImageWidthFromUrl(`/x?w=1`, origin)).toBe(40)
    expect(getAttachmentImageWidthFromUrl(`/x?w=999999`, origin)).toBe(4000)
  })
})

describe(`buildContentDispositionHeader`, () => {
  it(`passes plain ASCII filenames through without a filename* form`, () => {
    expect(buildContentDispositionHeader(`inline`, `report.png`)).toBe(
      `inline; filename="report.png"`
    )
  })

  it(`encodes non-Latin-1 filenames as RFC 5987 filename* with an ASCII fallback`, () => {
    const result = buildContentDispositionHeader(
      `inline`,
      `スクリーンショット 2026-07-10.png`
    )

    expect(result).toBe(
      `inline; filename="????????? 2026-07-10.png"; filename*=UTF-8''%E3%82%B9%E3%82%AF%E3%83%AA%E3%83%BC%E3%83%B3%E3%82%B7%E3%83%A7%E3%83%83%E3%83%88%202026-07-10.png`
    )
    // Regression: this exact value used to make the Headers constructor throw
    // a TypeError, turning every GET of the attachment into a permanent 500.
    expect(() => new Headers({ "Content-Disposition": result })).not.toThrow()
  })

  it(`strips CR/LF so header injection can never reach the wire`, () => {
    const result = buildContentDispositionHeader(`inline`, `a\r\nX-Evil: 1.png`)

    expect(result).not.toMatch(/[\r\n]/)
    expect(result).toBe(`inline; filename="aX-Evil: 1.png"`)
    expect(() => new Headers({ "Content-Disposition": result })).not.toThrow()
  })

  it(`keeps the quoted fallback parseable when the name contains quotes or backslashes`, () => {
    const result = buildContentDispositionHeader(
      `inline`,
      `she said "hi"\\.png`
    )

    expect(result).toBe(
      `inline; filename="she said 'hi''.png"; filename*=UTF-8''she%20said%20%22hi%22%5C.png`
    )
    expect(() => new Headers({ "Content-Disposition": result })).not.toThrow()
  })

  it(`encodes Latin-1-but-not-ASCII characters as UTF-8 in filename*`, () => {
    expect(buildContentDispositionHeader(`inline`, `ü.png`)).toBe(
      `inline; filename="?.png"; filename*=UTF-8''%C3%BC.png`
    )
  })

  it(`percent-encodes the RFC 5987 non-attr-chars '()* and spaces`, () => {
    expect(buildContentDispositionHeader(`inline`, `ü '()*.png`)).toBe(
      `inline; filename="? '()*.png"; filename*=UTF-8''%C3%BC%20%27%28%29%2A.png`
    )
  })

  it(`falls back to filename="file" for empty or control-only input`, () => {
    expect(buildContentDispositionHeader(`inline`, ``)).toBe(
      `inline; filename="file"`
    )
    expect(buildContentDispositionHeader(`inline`, `\x00\x01\x1f\x7f`)).toBe(
      `inline; filename="file"`
    )
  })

  it(`serves the ASCII fallback when a lone surrogate makes UTF-8 encoding impossible`, () => {
    expect(buildContentDispositionHeader(`inline`, `\uD800.png`)).toBe(
      `inline; filename="?.png"`
    )
  })

  it(`supports the attachment disposition`, () => {
    expect(buildContentDispositionHeader(`attachment`, `report.png`)).toBe(
      `attachment; filename="report.png"`
    )
  })
})

describe(`sanitizeUploadFilename`, () => {
  it(`strips control characters while preserving Unicode`, () => {
    expect(sanitizeUploadFilename(`スクショ\r\n.png`)).toBe(`スクショ.png`)
    expect(sanitizeUploadFilename(`a\x00b\x9fc.png`)).toBe(`abc.png`)
  })

  it(`falls back for empty, whitespace-only, and control-only input`, () => {
    expect(sanitizeUploadFilename(``)).toBe(`file`)
    expect(sanitizeUploadFilename(`   `)).toBe(`file`)
    expect(sanitizeUploadFilename(`\r\n\x7f`, `screenshot.png`)).toBe(
      `screenshot.png`
    )
  })

  it(`clamps to 255 characters`, () => {
    expect(sanitizeUploadFilename(`${`a`.repeat(300)}.png`)).toHaveLength(255)
  })
})

// ── EXP-297: any-file attachments ────────────────────────────────────────────

describe(`getMaxUploadBytesForContentType`, () => {
  it(`keeps the 10 MB ceiling for the five inline image types`, () => {
    for (const contentType of [
      `image/png`,
      `image/jpeg`,
      `image/webp`,
      `image/gif`,
      `image/avif`,
    ]) {
      expect(getMaxUploadBytesForContentType(contentType)).toBe(
        maxImageUploadBytes
      )
    }
  })

  it(`gives everything else the 50 MB file cap`, () => {
    for (const contentType of [
      `application/pdf`,
      `application/zip`,
      `video/mp4`,
      `text/plain`,
      `image/svg+xml`,
      `image/tiff`,
      ``,
    ]) {
      expect(getMaxUploadBytesForContentType(contentType)).toBe(
        maxFileUploadBytes
      )
    }
  })
})

describe(`isInlineSafeContentType`, () => {
  it(`allows the inline allowlist`, () => {
    for (const contentType of [
      `image/png`,
      `image/jpeg`,
      `image/webp`,
      `image/gif`,
      `image/avif`,
      `application/pdf`,
      `text/plain`,
      `video/mp4`,
      `video/quicktime`,
      `audio/mpeg`,
    ]) {
      expect(isInlineSafeContentType(contentType)).toBe(true)
    }
  })

  it(`forces everything scriptable or unknown to download`, () => {
    for (const contentType of [
      `text/html`,
      `image/svg+xml`,
      `application/xhtml+xml`,
      `application/zip`,
      `application/octet-stream`,
      `application/javascript`,
      ``,
    ]) {
      expect(isInlineSafeContentType(contentType)).toBe(false)
    }
  })

  it(`ignores parameters and case`, () => {
    expect(isInlineSafeContentType(`TEXT/PLAIN; charset=utf-8`)).toBe(true)
    expect(isInlineSafeContentType(`text/html; charset=utf-8`)).toBe(false)
  })
})

describe(`buildDeletedAttachmentPlaceholder`, () => {
  it(`is plain text, never image-shaped markdown`, () => {
    const placeholder = buildDeletedAttachmentPlaceholder(`shot.png`)
    expect(placeholder).toBe(`*(deleted image: shot.png)*`)
    expect(placeholder).not.toContain(`![`)
  })

  it(`strips markdown-structural characters and newlines`, () => {
    const placeholder = buildDeletedAttachmentPlaceholder(
      `![evil](http://x)\nmore]( stuff`
    )
    expect(placeholder).toBe(`*(deleted image: evilhttp://x more stuff)*`)
    expect(placeholder).not.toContain(`![`)
    expect(placeholder).not.toContain(`\n`)
  })

  it(`clamps a very long label`, () => {
    const placeholder = buildDeletedAttachmentPlaceholder(`a`.repeat(500))
    expect(placeholder).toBe(`*(deleted image: ${`a`.repeat(100)})*`)
  })

  it(`falls back when the label sanitizes away`, () => {
    expect(buildDeletedAttachmentPlaceholder(``)).toBe(
      `*(deleted image: image)*`
    )
    expect(buildDeletedAttachmentPlaceholder(`!!![]()`)).toBe(
      `*(deleted image: image)*`
    )
  })
})

describe(`replaceAttachmentReferencesWithPlaceholder`, () => {
  const origin = `http://localhost:5173/api/trpc/x`
  const target = `11111111-1111-1111-1111-111111111111`
  const other = `22222222-2222-2222-2222-222222222222`

  it(`replaces every occurrence: relative, ?w= variant, and absolute same-origin`, () => {
    const text = [
      `intro ![alt one](/api/attachments/${target})`,
      `![alt two](/api/attachments/${target}?w=480)`,
      `![](http://localhost:5173/api/attachments/${target})`,
      `![keep](/api/attachments/${other})`,
    ].join(`\n`)

    const result = replaceAttachmentReferencesWithPlaceholder(
      text,
      target,
      origin,
      `fallback.png`
    )

    expect(result.changed).toBe(true)
    expect(result.text).toBe(
      [
        `intro *(deleted image: alt one)*`,
        `*(deleted image: alt two)*`,
        `*(deleted image: fallback.png)*`,
        `![keep](/api/attachments/${other})`,
      ].join(`\n`)
    )
    // The rewritten text no longer references the deleted attachment at all.
    expect(
      extractAttachmentIdsFromDescription(result.text, origin).attachmentIds
    ).toEqual([other])
  })

  it(`rewrites an image with serializer-escaped alt text`, () => {
    const text = `![shot \\[1\\].png](/api/attachments/${target})`
    const result = replaceAttachmentReferencesWithPlaceholder(
      text,
      target,
      origin,
      `fallback.png`
    )
    expect(result.changed).toBe(true)
    // Unescaped alt, then the placeholder's own bracket strip.
    expect(result.text).toBe(`*(deleted image: shot 1.png)*`)
  })

  it(`rewrites a stored uppercase-id URL (extracted ids are lowercased)`, () => {
    const text = `![alt](/api/attachments/${target.toUpperCase()})`
    const result = replaceAttachmentReferencesWithPlaceholder(
      text,
      target,
      origin,
      `fallback.png`
    )
    expect(result.changed).toBe(true)
    expect(result.text).toBe(`*(deleted image: alt)*`)
  })

  it(`reports changed=false and returns the text untouched when nothing matches`, () => {
    const text = `![keep](/api/attachments/${other}) plus ${target} as bare text`
    const result = replaceAttachmentReferencesWithPlaceholder(
      text,
      target,
      origin,
      `fallback.png`
    )
    expect(result.changed).toBe(false)
    expect(result.text).toBe(text)
  })

  it(`ignores a same-id image hosted on a foreign origin`, () => {
    const text = `![x](https://evil.example.com/api/attachments/${target})`
    const result = replaceAttachmentReferencesWithPlaceholder(
      text,
      target,
      origin,
      `fallback.png`
    )
    expect(result.changed).toBe(false)
    expect(result.text).toBe(text)
  })
})

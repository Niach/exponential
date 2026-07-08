import { describe, expect, it } from "vitest"
import {
  injectMeta,
  matchPublicPath,
  stripMarkdownToPlainText,
  type PublicPageMeta,
} from "@/lib/seo/public-meta"

describe(`matchPublicPath`, () => {
  it(`matches a board path`, () => {
    expect(matchPublicPath(`/w/feedback/projects/exponential`)).toEqual({
      workspaceSlug: `feedback`,
      projectSlug: `exponential`,
    })
  })

  it(`matches an issue path`, () => {
    expect(
      matchPublicPath(`/w/feedback/projects/exponential/issues/EXP-42`)
    ).toEqual({
      workspaceSlug: `feedback`,
      projectSlug: `exponential`,
      issueIdentifier: `EXP-42`,
    })
  })

  it(`rejects trailing slashes`, () => {
    expect(matchPublicPath(`/w/feedback/projects/exponential/`)).toBeNull()
    expect(
      matchPublicPath(`/w/feedback/projects/exponential/issues/EXP-1/`)
    ).toBeNull()
  })

  it(`rejects unrelated and deeper paths`, () => {
    expect(matchPublicPath(`/`)).toBeNull()
    expect(matchPublicPath(`/w/feedback`)).toBeNull()
    expect(matchPublicPath(`/w/feedback/projects`)).toBeNull()
    expect(matchPublicPath(`/w/feedback/settings`)).toBeNull()
    expect(
      matchPublicPath(`/w/feedback/projects/exp/issues/EXP-1/extra`)
    ).toBeNull()
  })

  it(`decodes percent-encoded segments`, () => {
    expect(matchPublicPath(`/w/my%20ws/projects/proj`)).toEqual({
      workspaceSlug: `my ws`,
      projectSlug: `proj`,
    })
  })

  it(`returns null on malformed encoding`, () => {
    expect(matchPublicPath(`/w/%E0%A4%A/projects/proj`)).toBeNull()
  })
})

describe(`stripMarkdownToPlainText`, () => {
  it(`returns empty for nullish input`, () => {
    expect(stripMarkdownToPlainText(null)).toBe(``)
    expect(stripMarkdownToPlainText(undefined)).toBe(``)
    expect(stripMarkdownToPlainText(``)).toBe(``)
  })

  it(`strips headings, emphasis and inline code`, () => {
    expect(
      stripMarkdownToPlainText(`# Title\n\nSome **bold** and _italic_ and \`code\`.`)
    ).toBe(`Title Some bold and italic and code.`)
  })

  it(`unwraps links to their text and drops images`, () => {
    expect(
      stripMarkdownToPlainText(`See [the docs](https://x.com) ![shot](/a.png) now`)
    ).toBe(`See the docs now`)
  })

  it(`strips list, task-list and blockquote markers`, () => {
    const md = `> quote\n- item one\n- [ ] todo\n- [x] done\n1. first`
    expect(stripMarkdownToPlainText(md)).toBe(`quote item one todo done first`)
  })

  it(`drops code fences but keeps inner code text`, () => {
    expect(stripMarkdownToPlainText("```ts\nconst x = 1\n```")).toBe(`const x = 1`)
  })

  it(`leaves @email mentions verbatim`, () => {
    expect(stripMarkdownToPlainText(`ping @dev@example.com please`)).toBe(
      `ping @dev@example.com please`
    )
  })

  it(`collapses whitespace`, () => {
    expect(stripMarkdownToPlainText(`a\n\n\n  b\t c`)).toBe(`a b c`)
  })
})

describe(`injectMeta`, () => {
  const baseHtml = `<html><head><meta name="robots" content="noindex"><title>Exponential</title></head><body></body></html>`
  const meta: PublicPageMeta = {
    title: `EXP-1: Fix login · Exponential`,
    description: `A login bug`,
    url: `/w/feedback/projects/exponential/issues/EXP-1`,
    imagePath: `/og-card.png`,
  }

  it(`injects OG and Twitter meta with absolute urls`, () => {
    const out = injectMeta(baseHtml, meta, `https://app.exponential.at`)
    expect(out).toContain(`<meta property="og:type" content="website" />`)
    expect(out).toContain(
      `<meta property="og:url" content="https://app.exponential.at/w/feedback/projects/exponential/issues/EXP-1" />`
    )
    expect(out).toContain(
      `<meta property="og:image" content="https://app.exponential.at/og-card.png" />`
    )
    expect(out).toContain(
      `<meta name="twitter:card" content="summary_large_image" />`
    )
    expect(out).toContain(
      `<link rel="canonical" href="https://app.exponential.at/w/feedback/projects/exponential/issues/EXP-1" />`
    )
    expect(out).toContain(`</head>`)
  })

  it(`flips the default noindex to index,follow`, () => {
    const out = injectMeta(baseHtml, meta, `https://app.exponential.at`)
    expect(out).toContain(`<meta name="robots" content="index,follow" />`)
    expect(out).not.toContain(`content="noindex"`)
  })

  it(`adds a robots tag when none is present`, () => {
    const out = injectMeta(
      `<html><head><title>x</title></head><body></body></html>`,
      meta,
      `https://app.exponential.at`
    )
    expect(out).toContain(`<meta name="robots" content="index,follow" />`)
  })

  it(`strips a trailing slash from the origin`, () => {
    const out = injectMeta(baseHtml, meta, `https://app.exponential.at/`)
    expect(out).toContain(
      `content="https://app.exponential.at/w/feedback/projects/exponential/issues/EXP-1"`
    )
    expect(out).not.toContain(`.at//w/`)
  })

  it(`HTML-escapes hostile titles so they cannot break out of the attribute`, () => {
    const hostile: PublicPageMeta = {
      ...meta,
      title: `</title><script>alert(1)</script>`,
      description: `"onload="x`,
    }
    const out = injectMeta(baseHtml, hostile, `https://app.exponential.at`)
    expect(out).not.toContain(`<script>alert(1)</script>`)
    expect(out).toContain(`&lt;script&gt;alert(1)&lt;/script&gt;`)
    expect(out).toContain(`&quot;onload=&quot;x`)
  })

  it(`leaves html without a head untouched`, () => {
    const noHead = `<html><body>no head</body></html>`
    expect(injectMeta(noHead, meta, `https://app.exponential.at`)).toBe(noHead)
  })
})

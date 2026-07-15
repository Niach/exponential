import { describe, expect, it } from "vitest"
import {
  injectMeta,
  matchPublicPath,
  stripMarkdownToPlainText,
  type PublicPageMeta,
} from "@/lib/seo/public-meta"

describe(`matchPublicPath`, () => {
  it(`matches a board path`, () => {
    expect(matchPublicPath(`/t/feedback/projects/exponential`)).toEqual({
      workspaceSlug: `feedback`,
      projectSlug: `exponential`,
    })
  })

  it(`matches an issue path`, () => {
    expect(
      matchPublicPath(`/t/feedback/projects/exponential/issues/EXP-42`)
    ).toEqual({
      workspaceSlug: `feedback`,
      projectSlug: `exponential`,
      issueIdentifier: `EXP-42`,
    })
  })

  it(`still matches the legacy /w/ prefix`, () => {
    expect(matchPublicPath(`/w/feedback/projects/exponential`)).toEqual({
      workspaceSlug: `feedback`,
      projectSlug: `exponential`,
    })
    expect(
      matchPublicPath(`/w/feedback/projects/exponential/issues/EXP-42`)
    ).toEqual({
      workspaceSlug: `feedback`,
      projectSlug: `exponential`,
      issueIdentifier: `EXP-42`,
    })
  })

  it(`rejects trailing slashes`, () => {
    expect(matchPublicPath(`/t/feedback/projects/exponential/`)).toBeNull()
    expect(
      matchPublicPath(`/t/feedback/projects/exponential/issues/EXP-1/`)
    ).toBeNull()
  })

  it(`rejects unrelated and deeper paths`, () => {
    expect(matchPublicPath(`/`)).toBeNull()
    expect(matchPublicPath(`/t/feedback`)).toBeNull()
    expect(matchPublicPath(`/t/feedback/projects`)).toBeNull()
    expect(matchPublicPath(`/t/feedback/settings`)).toBeNull()
    expect(
      matchPublicPath(`/t/feedback/projects/exp/issues/EXP-1/extra`)
    ).toBeNull()
    expect(matchPublicPath(`/x/feedback/projects/exponential`)).toBeNull()
  })

  it(`decodes percent-encoded segments`, () => {
    expect(matchPublicPath(`/t/my%20ws/projects/proj`)).toEqual({
      workspaceSlug: `my ws`,
      projectSlug: `proj`,
    })
  })

  it(`returns null on malformed encoding`, () => {
    expect(matchPublicPath(`/t/%E0%A4%A/projects/proj`)).toBeNull()
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
    url: `/t/feedback/projects/exponential/issues/EXP-1`,
    imagePath: `/og-card.png`,
  }

  it(`injects OG and Twitter meta with absolute urls`, () => {
    const out = injectMeta(baseHtml, meta, `https://app.exponential.at`)
    expect(out).toContain(`<meta property="og:type" content="website" />`)
    expect(out).toContain(
      `<meta property="og:url" content="https://app.exponential.at/t/feedback/projects/exponential/issues/EXP-1" />`
    )
    expect(out).toContain(
      `<meta property="og:image" content="https://app.exponential.at/og-card.png" />`
    )
    expect(out).toContain(
      `<meta name="twitter:card" content="summary_large_image" />`
    )
    expect(out).toContain(
      `<link rel="canonical" href="https://app.exponential.at/t/feedback/projects/exponential/issues/EXP-1" />`
    )
    expect(out).toContain(`</head>`)
  })

  // EXP-99: OG meta is for link unfurlers, never an invitation to index. A
  // public board must stay out of search results.
  it(`preserves the default noindex`, () => {
    const out = injectMeta(baseHtml, meta, `https://app.exponential.at`)
    expect(out).toContain(`content="noindex"`)
    expect(out).not.toContain(`index,follow`)
  })

  it(`adds a noindex robots tag when none is present`, () => {
    const out = injectMeta(
      `<html><head><title>x</title></head><body></body></html>`,
      meta,
      `https://app.exponential.at`
    )
    expect(out).toContain(`<meta name="robots" content="noindex" />`)
  })

  it(`strips a trailing slash from the origin`, () => {
    const out = injectMeta(baseHtml, meta, `https://app.exponential.at/`)
    expect(out).toContain(
      `content="https://app.exponential.at/t/feedback/projects/exponential/issues/EXP-1"`
    )
    expect(out).not.toContain(`.at//t/`)
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

  // htmlEscape turns ' into &#39;, so a title containing $' becomes $&#39; —
  // which a string replacement pattern reads as $& (the whole match), splicing
  // </head> (and with $\` the entire document prefix) into the attribute.
  it(`is immune to replace() $-pattern injection from titles`, () => {
    const hostile: PublicPageMeta = {
      ...meta,
      title: `pay $\` up front and $' later`,
      description: `also $& here`,
    }
    const out = injectMeta(baseHtml, hostile, `https://app.exponential.at`)
    // The escaped title must land verbatim — no spliced document content.
    expect(out).toContain(`pay $\` up front and $&#39; later`)
    expect(out).toContain(`also $&amp; here`)
    // No head-content duplication: exactly one </head>, one <title>.
    expect(out.match(/<\/head>/g)).toHaveLength(1)
    expect(out.match(/<title>/g)).toHaveLength(1)
    expect(out).toMatch(/<\/head><body><\/body><\/html>$/)
  })

  it(`leaves html without a head untouched`, () => {
    const noHead = `<html><body>no head</body></html>`
    expect(injectMeta(noHead, meta, `https://app.exponential.at`)).toBe(noHead)
  })
})

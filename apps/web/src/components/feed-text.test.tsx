import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { FeedText } from "@/components/feed-text"
import { attachmentImageSize, entityTokens, linkBareUrls, proseSegments } from "@/components/read-markdown"

// #VAPP-258: a chat's text is genUI's Markdown with the app as its host: a
// resolved `@<email>` is a mention chip (chat e4a04d22), a resolved
// `#IDENT` (and, in agent prose, a bare one) an issue chip, a bare URL a link.

const issues: Record<string, { identifier: string; title: string; statusIcon: string; statusColor: string }> = {
  "EXP-7": { identifier: `EXP-7`, title: `Fix login`, statusIcon: `status-todo`, statusColor: `#888` },
}
const refs = { resolve: (id: string) => (issues[id.toUpperCase()] as never) ?? null }
vi.mock(`@/components/issue-ref-provider`, () => ({ useIssueRefs: () => refs }))
vi.mock(`@/hooks/use-app-link-click`, () => ({ useAppLinkClick: () => () => {} }))
const members: Record<string, string> = { "ada@exp.dev": `Ada Lovelace` }
const mentions = { resolve: (email: string) => (members[email.toLowerCase()] ? { name: members[email.toLowerCase()]! } : null) }
vi.mock(`@/components/mention-provider`, () => ({ useMentions: () => mentions }))

describe(`FeedText`, () => {
  it(`chips a member's mention and leaves an unknown email as text`, () => {
    const { container } = render(<FeedText text="ask @ada@exp.dev or @x@y.com" ariaLabel="message" />)
    const chips = container.querySelectorAll(`a[data-chip="mention"]`)
    expect(chips).toHaveLength(1)
    expect(chips[0]!.textContent).toBe(`@Ada Lovelace`)
    expect(container.textContent).toBe(`ask @Ada Lovelace or @x@y.com`)
  })
  it(`chips bare and #-identifiers in agent prose, links bare URLs, breaks hard`, () => {
    window.history.pushState({}, ``, `/t/acme/inbox`)
    const { container } = render(<FeedText text={"landed EXP-7 and #EXP-7\nsee https://example.dev/x."} ariaLabel="message" hardBreaks />)
    expect(container.querySelectorAll(`a[data-chip="issue"]`)).toHaveLength(2)
    expect(container.querySelector(`a[data-chip="issue"]`)!.textContent).toBe(`EXP-7Fix login`)
    expect(container.querySelector(`a[data-chip="issue"] .xui-Markdown-chipIdent`)!.textContent).toBe(`EXP-7`)
    expect(container.querySelector(`a[href="https://example.dev/x"]`)).toBeTruthy()
    expect(container.querySelector(`br`)).toBeTruthy()
    expect(container.querySelector(`[data-r-density="chat"]`)).toBeTruthy()
    expect(container.querySelector(`a[data-chip="issue"]`)!.getAttribute(`href`)).toBe(`${window.location.origin}/t/acme/issues/EXP-7`)
  })
})

describe(`read-markdown helpers`, () => {
  it(`links bare URLs outside code, never inside a link`, () => {
    expect(linkBareUrls(`go https://a.dev/x.`)).toBe(`go [https://a.dev/x](https://a.dev/x).`)
    expect(linkBareUrls("`https://a.dev` [d](https://a.dev)")).toBe("`https://a.dev` [d](https://a.dev)")
    expect(linkBareUrls("```\nhttps://a.dev\n```")).toBe("```\nhttps://a.dev\n```")
  })
  it(`turns rewritten entity links into chip tokens`, () => {
    const { text, entities } = entityTokens(`open [Web](https://app.dev/t/a/boards/web#exp-chip-board) now`)
    expect(entities).toEqual([{ label: `Web`, href: `https://app.dev/t/a/boards/web`, icon: `nav-boards` }])
    expect(proseSegments(text, entities, null, null, false)).toEqual([
      { text: `open ` },
      { text: `Web`, href: `https://app.dev/t/a/boards/web`, chip: { kind: `entity`, label: `Web`, icon: `nav-boards` } },
      { text: ` now` },
    ])
  })
  it(`sizes a picture from its row, scaled to the ?w= width`, () => {
    expect(attachmentImageSize(`/api/attachments/a`, { width: 800, height: 400 })).toEqual({ width: 800, height: 400 })
    expect(attachmentImageSize(`/api/attachments/a?w=400`, { width: 800, height: 400 })).toEqual({ width: 400, height: 200 })
    expect(attachmentImageSize(`/api/attachments/a`, undefined)).toBeNull()
  })

  // #EXP-1945 (image-actions.json `imageSource`): another website's picture
  // is never fetched: THE link in its place (alt, else `Image: <host>`).
  it(`an external picture is a link with no <img>; our attachment loads`, () => {
    const { container } = render(<FeedText text={"![A cat](https://example.com/cat.png)\n\n![](http://evil.test/x.png)\n\n![shot](/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90)"} ariaLabel="message" />)
    const imgs = [...container.querySelectorAll(`img`)].map((img) => img.getAttribute(`src`))
    expect(imgs).toEqual([`${window.location.origin}/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90`])
    const links = [...container.querySelectorAll(`a`)].map((a) => [a.getAttribute(`href`), a.textContent])
    expect(links).toEqual([
      [`https://example.com/cat.png`, `A cat`],
      [`http://evil.test/x.png`, `Image: evil.test`],
    ])
  })
})

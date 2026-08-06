import { describe, expect, it } from "vitest"
import { linkSegments } from "./linkify"

// The captured claude v2.1.222 sign-in URL (EXP-430) — the load-bearing
// case: it must survive tokenization byte-for-byte.
const SIGN_IN_URL = `https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference+user%3Asessions%3Aclaude_code+user%3Amcp_servers+user%3Afile_upload&code_challenge=j7BY1qKMJ1Y2LC5xNqD5VUJayK_UZbPl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE`

const rejoin = (text: string) =>
  linkSegments(text)
    .map((s) => s.text)
    .join(``)

describe(`linkSegments`, () => {
  it(`passes plain text through as one segment`, () => {
    expect(linkSegments(`Session started`)).toEqual([
      { text: `Session started` },
    ])
    expect(linkSegments(``)).toEqual([{ text: `` }])
  })

  it(`links the claude sign-in URL intact`, () => {
    const text = `Claude sign-in: open this link in your browser to authorize, then send the code you receive back here as a regular message:\n\n${SIGN_IN_URL}`
    const segments = linkSegments(text)
    expect(segments).toHaveLength(2)
    expect(segments[1]).toEqual({ text: SIGN_IN_URL, href: SIGN_IN_URL })
    expect(rejoin(text)).toBe(text)
  })

  it(`links a URL in the middle of prose and preserves the text around it`, () => {
    const text = `Opened https://github.com/x/y/pull/12 for review`
    expect(linkSegments(text)).toEqual([
      { text: `Opened ` },
      {
        text: `https://github.com/x/y/pull/12`,
        href: `https://github.com/x/y/pull/12`,
      },
      { text: ` for review` },
    ])
  })

  it(`keeps trailing prose punctuation out of the link`, () => {
    const segments = linkSegments(`(see https://x.dev/docs).`)
    expect(segments).toEqual([
      { text: `(see ` },
      { text: `https://x.dev/docs`, href: `https://x.dev/docs` },
      { text: `).` },
    ])
  })

  it(`keeps balanced parens inside the link`, () => {
    const url = `https://en.wikipedia.org/wiki/Bracket_(disambiguation)`
    expect(linkSegments(url)).toEqual([{ text: url, href: url }])
  })

  it(`handles multiple URLs`, () => {
    const text = `a https://one.test b http://two.test c`
    const segments = linkSegments(text)
    expect(segments.filter((s) => s.href)).toHaveLength(2)
    expect(rejoin(text)).toBe(text)
  })
})

// VAPP-107: property tests (fast-check) on the built-in markdown painter:
// any text renders (never a throw), no HTML passes through, every href and
// src on the output passed the host's policy. The nightly job runs them
// with FC_NUM_RUNS=5000; a PR run keeps the default budget.

import { describe, expect, it } from "vitest"
import fc from "fast-check"
import { renderToStaticMarkup } from "react-dom/server"
import { decideUrl, mediaRequest } from "@exponential-at/ui"
import { ownPrivateHosts } from "@exponential-at/ui/web"
import { linkAt, parseList, renderMarkdown } from "./markdown"

const runs = Number(process.env.FC_NUM_RUNS ?? 200)
// FC_SEED replays a reported run (`seed` in fast-check's failure message).
const params = { numRuns: runs, ...(process.env.FC_SEED ? { seed: Number(process.env.FC_SEED) } : {}) }

const markdownish = fc.array(
  fc.oneof(
    fc.string({ maxLength: 24 }),
    fc.constantFrom(`# `, `## `, `- `, `* `, `1. `, `> `, `\`\`\``, `**`, `_`, `\``, `[`, `](`, `)`, `![`, `<script>alert(1)</script>`, `<img src=x onerror=alert(1)>`, `javascript:alert(1)`, `http://127.0.0.1/x.png`, `https://exponential.at/`, `|`, `\n`, `\\`, `(`, `<`, `>`)
  ),
  { maxLength: 24 }
).map((parts) => parts.join(``))

/** Renders `text` and checks the output against the SAME policy the
 *  renderer applies: the page's own private host (jsdom's localhost:3000,
 *  `ownPrivateHosts`) is opted in, so a relative destination (`#`, `foo`)
 *  resolves there and is allowed, every other private host stays denied. */
function checkPoliced(text: string): void {
  const html = renderToStaticMarkup(<>{renderMarkdown(text, {})}</>)
  // React escapes `<` in text, so a literal tag here would be a real element.
  expect(html).not.toMatch(/<script|<img[^>]*onerror|<iframe/i)
  const allowPrivate = ownPrivateHosts()
  for (const m of html.matchAll(/ href="([^"]*)"/g)) {
    const href = m[1]!.replace(/&amp;/g, `&`).replace(/&#x27;/g, `'`).replace(/&quot;/g, `"`)
    expect(decideUrl({ baseUrl: `http://localhost/`, allowPrivate }, href).allowed, href).toBe(true)
  }
  for (const m of html.matchAll(/ src="([^"]*)"/g)) {
    const src = m[1]!.replace(/&amp;/g, `&`)
    expect(mediaRequest(src, { baseUrl: `http://localhost/`, allowPrivate }), src).not.toBeNull()
  }
}

describe(`VAPP-107 property: the built-in markdown painter`, () => {
  it(`any text renders without a throw, without HTML, with every destination policed`, () => {
    fc.assert(fc.property(markdownish, checkPoliced), params)
  })
  // Empty and bare-fragment destinations (fast-check seed -713952292 found
  // `[](# )`): policed like any `#foo` anchor.
  it.each([`[](# )`, `[](#)`, `[]( )`, `[](<>)`, `[x](#foo)`])(`a %s destination is policed`, (text) => {
    checkPoliced(text)
  })
  it(`linkAt and parseList take any input`, () => {
    fc.assert(
      fc.property(fc.string({ maxLength: 64 }), fc.nat({ max: 64 }), fc.array(fc.string({ maxLength: 24 }), { maxLength: 8 }), (s, at, lines) => {
        expect(() => linkAt(s, Math.min(at, s.length))).not.toThrow()
        expect(() => parseList(lines)).not.toThrow()
      }),
      params
    )
  })
})

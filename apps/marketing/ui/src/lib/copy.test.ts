/* The site copy lint (scripts/check-copy.ts) as a test, so CI's
   `bun --filter @exp/marketing test` runs it: no "X, never Y" / "X, not Y"
   contrast construction (or its variants) in the prose of exponential.at. */
import { expect, test } from "bun:test"
import { checkCopy, checkSource } from "../../../scripts/check-copy"

test(`site copy has no contrast constructions`, () => {
  const findings = checkCopy().map((f) => `${f.file}:${f.line} [${f.pattern}] ${f.text}`)
  expect(findings).toEqual([])
})

test(`the lint catches the construction in prose and leaves code alone`, () => {
  const page = `export const P = () => (
  <section>
    <h2>Apps are data,
      <br /><em>never code.</em></h2>
    <p>An app is <strong>data, never code</strong>: one package.</p>
    <p>Pick a tag{\` \`}
      instead of a branch.</p>
    <DocsCode>{\`// data, never code\`}</DocsCode>
    <p>Run <code>data, never code</code> to see it.</p>
  </section>
)
// A comment, not prose.
const desc = \`Results carry ids, not markup.\`
`
  const found = checkSource(`page.tsx`, page).map((f) => `${f.line} ${f.pattern}`)
  expect(found).toEqual([`3 X, never Y`, `5 X, never Y`, `7 instead of`, `13 X, not Y`])
  expect(checkSource(`a.md`, "Plain words.\n\n```ts\nconst a = 1, never = 2\n```\n")).toEqual([])
  expect(checkSource(`b.md`, `It's a tool, not a toy.`).length).toBe(1)
  expect(checkSource(`c.html`, `<meta name="description" content="Apps are data, never code." />`).length).toBeGreaterThan(0)
})

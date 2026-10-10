import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { classifyAppLink, type AppLink } from "./app-link"

// EXP-1188: the ×4 contract — desktop `domain::app_link`, iOS `AppLinkTests`,
// Android `AppLinkTest` run the same cases.
const fixture = JSON.parse(
  readFileSync(
    join(
      import.meta.dirname,
      `../../../../packages/domain-contract/fixtures/app-link.json`,
    ),
    `utf8`,
  ),
) as { origin: string; cases: { name: string; href: string; link: AppLink }[] }

describe(`classifyAppLink`, () => {
  for (const c of fixture.cases) {
    it(c.name, () => {
      expect(classifyAppLink(c.href, fixture.origin)).toEqual(c.link)
    })
  }
})

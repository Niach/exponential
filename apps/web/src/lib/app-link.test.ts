import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { classifyAppLink, isRoutedAppPath, ROUTED_APP_PREFIXES, type AppLink } from "./app-link"

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

// Web-only: an `app` link routes in-app only under a prefix the router
// serves; a same-origin resource stays an ordinary link for the browser.
describe(`isRoutedAppPath`, () => {
  const routeTree = readFileSync(join(import.meta.dirname, `../routeTree.gen.ts`), `utf8`)
  const fullPaths = [...new Set([...routeTree.matchAll(/fullPath: '([^']*)'/g)].map((m) => m[1]!))]
  const resources = (path: string) =>
    path.startsWith(`/api/`) || path.startsWith(`/.well-known/`) || path === `/robots.txt`

  it(`reads every screen of the route tree as routed`, () => {
    expect(fullPaths.length).toBeGreaterThan(20)
    for (const path of fullPaths.filter((p) => !resources(p))) {
      expect(isRoutedAppPath(path), path).toBe(true)
    }
  })

  it(`leaves the route tree's resources to the browser`, () => {
    const resourcePaths = fullPaths.filter(resources)
    expect(resourcePaths.length).toBeGreaterThan(5)
    for (const path of resourcePaths) expect(isRoutedAppPath(path), path).toBe(false)
  })

  it(`leaves static files and the widget to the browser`, () => {
    for (const path of [
      `/api/attachments/7f3c2a10-0000-4000-8000-000000000001`,
      `/api/attachments/x?poster=1`,
      `/NOTICES.txt`,
      `/widget/v1/demo.html`,
      `/robots.txt`,
      `/favicon.ico`,
    ]) {
      expect(isRoutedAppPath(path), path).toBe(false)
    }
  })

  it(`routes the root, a query on it, and case-insensitive screen prefixes`, () => {
    expect(isRoutedAppPath(`/`)).toBe(true)
    expect(isRoutedAppPath(`/?x=1`)).toBe(true)
    expect(isRoutedAppPath(`/t/acme/reviews?tab=mine`)).toBe(true)
    expect(isRoutedAppPath(`/T/acme`)).toBe(true)
    expect(isRoutedAppPath(`/%74/acme`)).toBe(true)
    expect(isRoutedAppPath(`/settings/account`)).toBe(false)
    expect(isRoutedAppPath(`/account/notifications#top`)).toBe(true)
  })

  it(`names only route-tree dirs`, () => {
    const firstSegments = new Set(
      fullPaths.filter((p) => !resources(p)).map((p) => p.split(`/`)[1]).filter((s) => s)
    )
    expect([...ROUTED_APP_PREFIXES].sort()).toEqual([...firstSegments].sort())
  })
})

import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { cardLinkOpens, classifyAppLink, isRoutedAppPath, crossAppLink, issueTextLinks, linkOpenOutcome, ROUTED_APP_PREFIXES, type AppLink } from "./app-link"
import planCard from "@exp/domain-contract/fixtures/plan-card.json"

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
) as {
  origin: string
  team: string
  cases: { name: string; href: string; team?: string | null; link: AppLink }[]
  openCases: { name: string; input: Parameters<typeof linkOpenOutcome>[0]; outcome: string }[]
}

describe(`classifyAppLink`, () => {
  for (const c of fixture.cases) {
    it(c.name, () => {
      expect(classifyAppLink(c.href, fixture.origin, c.team === undefined ? fixture.team : c.team)).toEqual(c.link)
    })
  }
})

describe(`linkOpenOutcome`, () => {
  for (const c of fixture.openCases) {
    it(c.name, () => {
      expect(linkOpenOutcome(c.input)).toBe(c.outcome)
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
    expect(isRoutedAppPath(`/t/acme/threads?tab=guide`)).toBe(true)
    expect(isRoutedAppPath(`/T/acme`)).toBe(true)
    expect(isRoutedAppPath(`/%74/acme`)).toBe(true)
    expect(isRoutedAppPath(`/settings/account`)).toBe(false)
    expect(isRoutedAppPath(`/onboarding#top`)).toBe(true)
  })

  it(`names only route-tree dirs`, () => {
    const firstSegments = new Set(
      fullPaths.filter((p) => !resources(p)).map((p) => p.split(`/`)[1]).filter((s) => s)
    )
    expect([...ROUTED_APP_PREFIXES].sort()).toEqual([...firstSegments].sort())
  })
})

describe(`#EXP-1674: a chat card's links`, () => {
  for (const c of planCard.linkCases as { name: string; href: string; team?: string | null; opens: boolean }[]) {
    it(c.name, () => {
      expect(cardLinkOpens(c.href, planCard.linkOrigin, c.team === undefined ? planCard.linkTeam : c.team)).toBe(c.opens)
    })
  }

  it(`only refs that resolve become links, on the team's short route`, () => {
    const known = (id: string) => id !== `VAPP-203`
    const subtitle = `#VAPP-202 #VAPP-203 merged · V1 #1758`
    const segments = issueTextLinks(subtitle, `my team`, known)!
    expect(segments.map((segment) => segment.text).join(``)).toBe(subtitle)
    expect(segments.filter((segment) => segment.href)).toEqual([{ text: `#VAPP-202`, href: `/t/my%20team/issues/VAPP-202` }])
    expect(issueTextLinks(`#VAPP-203`, `acme`, known)).toBeNull()
    expect(issueTextLinks(`no refs`, `acme`, known)).toBeNull()
    expect(issueTextLinks(`#VAPP-202`, null, known)).toBeNull()
  })
})

describe(`#EXP-1674: crossAppLink`, () => {
  const screen = (name: string, params: Record<string, string> = {}): AppLink => ({ kind: `appScreen`, teamSlug: `acme`, appSlug: `crm`, screen: name, params })
  const pkg = {
    screens: [
      { template: `home`, title: `Home`, exposed: { description: `The list` } },
      { template: `inner`, title: `Inner` },
      { template: `deal`, title: `Deal`, nav: false, exposed: { description: `A deal`, params: { id: { required: true } } } },
    ],
  }
  const find = (_team: string, slug: string) => (slug === `crm` ? { id: `a-crm`, package: pkg } : null)

  it(`a link at another app needs an exposed screen and its required params`, () => {
    expect(crossAppLink(screen(`deal`, { id: `d1`, admin: `1` }), `a-other`, find)).toEqual(screen(`deal`, { id: `d1` }))
    expect(crossAppLink(screen(`deal`), `a-other`, find)).toBeNull()
    expect(crossAppLink(screen(`inner`), `a-other`, find)).toBeNull()
  })

  it(`a missing app or an unpublished one refuses exactly the same`, () => {
    expect(crossAppLink({ kind: `appHome`, teamSlug: `acme`, appSlug: `gone` }, `a-other`, find)).toBeNull()
    expect(crossAppLink(screen(`deal`, { id: `d1` }), `a-other`, () => ({ id: `a-crm`, package: null }))).toBeNull()
  })

  it(`an app's own screens and every other link stay as they are`, () => {
    expect(crossAppLink(screen(`inner`), `a-crm`, find)).toEqual(screen(`inner`))
    const issue: AppLink = { kind: `issueRef`, teamSlug: `acme`, identifier: `EXP-1` }
    expect(crossAppLink(issue, `a-other`, find)).toEqual(issue)
  })

  it(`the app's home goes to its first navigation screen, when exposed`, () => {
    expect(crossAppLink({ kind: `appHome`, teamSlug: `acme`, appSlug: `crm` }, `a-other`, find)).toEqual(screen(`home`))
  })
})

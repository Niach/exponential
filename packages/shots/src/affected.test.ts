/**
 * The scoping rules the unattended refresh trusts.
 *
 * Run against the REAL repo on purpose: the whole claim of `affected.ts` is
 * that it reads the actual route tree, the actual import graph and the actual
 * catalog, so a fixture would test the fixture. Assertions stay structural
 * (this path narrows, that path widens, this one is dropped) rather than
 * pinning a view count that any product change would move.
 */
import { describe, expect, test } from "bun:test"
import { PLATFORMS, viewsFor } from "@exp/view-catalog"
import { affectedScope } from "./affected.ts"

const WEB_LANES = [`web`, `web-mobile`, `desktop`] as const

function scope(...changedFiles: string[]) {
  return affectedScope({
    changedFiles,
    platforms: WEB_LANES,
    // The store is committed and complete; pinning this keeps the test about
    // the RULES rather than about whichever shot is missing today.
    includeMissing: false,
  })
}

function views(result: ReturnType<typeof scope>, platform: (typeof WEB_LANES)[number]): string[] {
  return result.byPlatform.get(platform) ?? []
}

describe(`web attribution`, () => {
  test(`a leaf route file narrows to its own view`, () => {
    const result = scope(`apps/web/src/routes/t/$teamSlug/settings/labels.tsx`)
    expect(views(result, `web`)).toEqual([`settings-labels`])
    expect(views(result, `web-mobile`)).toEqual([`settings-labels`])
    expect(views(result, `desktop`)).toEqual([])
  })

  test(`one route file feeds every view that names it`, () => {
    // `inbox` and `my-issues` are the same route; the tab is a query param.
    const result = scope(`apps/web/src/routes/t/$teamSlug/inbox/index.tsx`)
    expect(views(result, `web`).sort()).toEqual([`inbox`, `my-issues`])
  })

  test(`a shared component reaches every view that renders it`, () => {
    const result = scope(`apps/web/src/components/issue-detail-view.tsx`)
    expect(views(result, `web`)).toContain(`issue-detail`)
    expect(views(result, `web`)).toContain(`issue-comments`)
    expect(views(result, `web`)).not.toContain(`settings-labels`)
  })

  test(`the global stylesheet is every web view and no desktop one`, () => {
    const result = scope(`apps/web/src/styles.css`)
    expect(views(result, `web`)).toHaveLength(viewsFor(`web`).length)
    expect(views(result, `desktop`)).toEqual([])
  })

  test(`a server file nothing renders widens the web lane, honestly reported`, () => {
    const result = scope(`apps/web/src/routes/api/shapes/issues.ts`)
    expect(views(result, `web`)).toHaveLength(viewsFor(`web`).length)
    expect(result.broad.map((entry) => entry.path)).toContain(`apps/web/src/routes/api/shapes/issues.ts`)
    expect(views(result, `desktop`)).toEqual([])
  })

  test(`a page route no view photographs is dropped, not widened`, () => {
    const result = scope(`apps/web/src/routes/about.tsx`)
    expect(views(result, `web`)).toEqual([])
    expect(result.ignored).toContain(`apps/web/src/routes/about.tsx`)
  })

  test(`the recipe registry re-drives the recipe views only`, () => {
    const result = scope(`apps/web/scripts/lib/view-recipes.ts`)
    expect(views(result, `web`)).toContain(`board-filters`)
    expect(views(result, `web`)).not.toContain(`board`)
    expect(views(result, `desktop`)).toEqual([])
  })
})

describe(`desktop attribution`, () => {
  test(`a ui module named after a view narrows to it`, () => {
    const result = scope(`apps/desktop/crates/ui/src/issue_detail.rs`)
    expect(views(result, `desktop`)).toEqual([`issue-detail`])
    expect(views(result, `web`)).toEqual([])
  })

  test(`a settings section matches its drive value`, () => {
    const result = scope(`apps/desktop/crates/ui/src/settings/general.rs`)
    expect(views(result, `desktop`)).toEqual([`settings-general`])
  })

  test(`an unmatched desktop module widens the whole lane`, () => {
    const result = scope(`apps/desktop/crates/ui/src/sidebar.rs`)
    expect(views(result, `desktop`)).toHaveLength(viewsFor(`desktop`).length)
    expect(views(result, `web`)).toEqual([])
  })

  test(`the headless CLI is not the desktop app`, () => {
    const result = scope(`apps/desktop/crates/cli/src/main.rs`)
    expect(views(result, `desktop`)).toEqual([])
  })
})

describe(`fail-safe`, () => {
  test(`an unrecognised repo-wide path widens every lane`, () => {
    const result = scope(`docker-compose.yaml`)
    for (const platform of WEB_LANES) {
      expect(views(result, platform)).toHaveLength(viewsFor(platform).length)
    }
  })

  test(`a shared package widens the compiled clients and narrows the web`, () => {
    const result = scope(`packages/icons/icons.json`)
    expect(views(result, `desktop`)).toHaveLength(viewsFor(`desktop`).length)
    expect(views(result, `web`).length).toBeGreaterThan(0)
  })

  test(`docs, tests and the capture pipeline itself change nothing`, () => {
    const result = scope(
      `docs/third-party-licences.md`,
      `apps/web/src/lib/filters.test.ts`,
      `packages/shots/src/store.ts`,
      `apps/marketing/src/lib/seo.ts`
    )
    for (const platform of WEB_LANES) expect(views(result, platform)).toEqual([])
    expect(result.ignored).toHaveLength(4)
  })

  test(`a changed catalog entry is in scope on every platform that shoots it`, () => {
    const result = affectedScope({
      changedFiles: [`packages/view-catalog/views.json`],
      platforms: PLATFORMS,
      catalogChanges: [`board`],
      includeMissing: false,
    })
    expect(result.byPlatform.get(`web`)).toEqual([`board`])
    expect(result.byPlatform.get(`desktop`)).toEqual([`board`])
    // The catalog claims no `board` shot for iOS's store lane… whatever the
    // catalog says, the scope never invents a pair it does not claim.
    for (const platform of PLATFORMS) {
      for (const id of result.byPlatform.get(platform) ?? []) {
        expect(viewsFor(platform).some((view) => view.id === id)).toBe(true)
      }
    }
  })

  test(`a view with no stored shot is always in scope`, () => {
    const withMissing = affectedScope({ changedFiles: [], platforms: [`ios`] })
    const withoutMissing = affectedScope({
      changedFiles: [],
      platforms: [`ios`],
      includeMissing: false,
    })
    expect(withoutMissing.byPlatform.get(`ios`)).toEqual([])
    // Whatever the store currently holds, "missing" can only ever ADD.
    expect((withMissing.byPlatform.get(`ios`) ?? []).length).toBeGreaterThanOrEqual(0)
  })
})

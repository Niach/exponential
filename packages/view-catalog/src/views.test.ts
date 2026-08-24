/**
 * The catalog's drift gates.
 *
 * A manifest nobody checks rots within a release. These tests tie `views.json`
 * to the three places the product actually lives:
 *
 *   1. shape        — the file parses, ids are unique, nothing dangles
 *   2. native       — BIDIRECTIONAL against the iOS/Android UI-test suites:
 *                     no manifest shot without a test, no test shot without a
 *                     manifest entry
 *   3. web routes   — every route file in apps/web is either captured by a view
 *                     or excluded WITH A REASON. Adding a route without
 *                     touching the catalog fails here, on purpose.
 *   4. recipes      — every recipe name resolves in the web recipe registry
 *   5. geometry     — the stored frames match the capture aspect ratios
 *
 * Gates 2 and 4 read files owned by sibling workstreams. While one of those is
 * still landing the file is simply absent: the gate then warns and checks only
 * the manifest side, rather than blocking the catalog on someone else's merge.
 * Once the file exists the gate is unconditional.
 */
import { describe, expect, test } from "bun:test"
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative, resolve } from "node:path"
import { z } from "zod"
import {
  DEFAULT_PLATFORMS,
  EXCLUDED_ROUTES,
  GROUPS,
  PLATFORMS,
  PLATFORM_FRAME,
  STORE_DEFAULT_TOLERANCE,
  STORE_DIR,
  VIEWS,
  captureFor,
  shotPath,
  viewById,
  viewsFor,
  type Platform,
  type View,
  type WebCapture,
} from "./index"

const REPO_ROOT = resolve(import.meta.dir, `../../..`)
const ROUTES_DIR = resolve(REPO_ROOT, `apps/web/src/routes`)
const RECIPES_FILE = resolve(REPO_ROOT, `apps/web/scripts/lib/view-recipes.ts`)
const IOS_STORE = resolve(
  REPO_ROOT,
  `apps/ios/ExponentialUITests/StoreScreenshots.swift`
)
const IOS_STYLEGUIDE = resolve(
  REPO_ROOT,
  `apps/ios/ExponentialUITests/StyleguideScreenshots.swift`
)
const ANDROID_STORE = resolve(
  REPO_ROOT,
  `apps/android/app/src/androidTest/java/com/exponential/app/StoreScreenshotsTest.kt`
)
const ANDROID_STYLEGUIDE = resolve(
  REPO_ROOT,
  `apps/android/app/src/androidTest/java/com/exponential/app/StyleguideScreenshotsTest.kt`
)

/**
 * The pinned recipe registry. The real source of truth is
 * `apps/web/scripts/lib/view-recipes.ts`; this list is the fallback the gate
 * uses until that file lands, and the cross-check afterwards.
 */
const PINNED_RECIPES = [
  `openRegister`,
  `openFilterPopover`,
  `scrollToComments`,
  `openCreateIssue`,
  `openSearch`,
  `openStartCoding`,
  `openStartCodingActions`,
  `openStartCodingChat`,
  `openOnboardingCreateTeam`,
  `openOnboardingJoin`,
  `openBoardBulkEdit`,
  `openBoardSwitcher`,
  `openMachineSettings`,
  `openAddServer`,
  `openActionCreate`,
  `openAutomationsTab`,
  `openSuggestionsTab`,
  `openWidgetEditor`,
  `openAgentDock`,
  `expandFirstDiffFile`,
  `openFirstThread`,
  `openActionEditor`,
  `openAutomationEditor`,
  `openGettingStarted`,
]

// ---------------------------------------------------------------- 1. shape

const groupIds = [
  `auth`,
  `issues`,
  `my-work`,
  `coding`,
  `reviews`,
  `actions`,
  `support`,
  `settings`,
  `ide`,
  `getting-started`,
] as const

const platformEnum = z.enum([`web`, `web-mobile`, `desktop`, `ios`, `android`])

const anchorSchema = z
  .strictObject({
    text: z.string().min(1).optional(),
    testId: z.string().min(1).optional(),
    timeoutMs: z.number().int().positive().optional(),
  })
  .refine((anchor) => Boolean(anchor.text ?? anchor.testId), {
    message: `an anchor needs text or a testId — a bare timeout photographs the loading state`,
  })

const webCaptureSchema = z.strictObject({
  route: z.string().startsWith(`/`),
  anchor: anchorSchema,
  auth: z.enum([`demo`, `anonymous`, `newcomer`]).optional(),
  recipe: z.string().min(1).optional(),
  settleMs: z.number().int().nonnegative().optional(),
  fullPage: z.boolean().optional(),
})

const nativeCaptureSchema = z.strictObject({
  shot: z.string().min(1),
  lane: z.enum([`store`, `styleguide`]),
})

const desktopCaptureSchema = z.strictObject({
  drive: z.union([
    z.strictObject({
      kind: z.enum([`screen`, `tool`, `settings`, `dialog`]),
      value: z.string().min(1),
    }),
    z.strictObject({ kind: z.enum([`login`, `manual`]) }),
  ]),
  anchorDelayMs: z.number().int().nonnegative().optional(),
  // Layered on top of the drive's own vars. DEV-ONLY by construction: anything
  // outside the EXP_DEV_ family would change how a real install behaves.
  env: z.record(z.string().startsWith(`EXP_DEV_`), z.string().min(1)).optional(),
})

const viewSchema = z.strictObject({
  id: z.string().regex(/^[a-z0-9]+(-[a-z0-9]+)*$/),
  group: z.enum(groupIds),
  title: z.string().min(1),
  blurb: z.string().min(1),
  order: z.number().int().positive(),
  web: webCaptureSchema.optional(),
  webMobile: z.union([z.literal(`inherit`), webCaptureSchema]).optional(),
  ios: nativeCaptureSchema.optional(),
  android: nativeCaptureSchema.optional(),
  desktop: desktopCaptureSchema.optional(),
  diffTolerance: z.number().gt(0).lt(1).optional(),
  notes: z.partialRecord(platformEnum, z.string().min(1)).optional(),
})

const catalogSchema = z.strictObject({
  groups: z
    .array(
      z.strictObject({
        id: z.enum(groupIds),
        label: z.string().min(1),
        blurb: z.string().min(1),
        order: z.number().int().positive(),
      })
    )
    .min(1),
  excludedRoutes: z.array(
    z.strictObject({
      route: z.string().startsWith(`/`),
      reason: z.string().min(10),
    })
  ),
  views: z.array(viewSchema).min(1),
})

const rawCatalog = JSON.parse(
  readFileSync(resolve(import.meta.dir, `../views.json`), `utf8`)
)

describe(`catalog shape`, () => {
  test(`views.json validates`, () => {
    const parsed = catalogSchema.safeParse(rawCatalog)
    if (!parsed.success) {
      throw new Error(
        `views.json is invalid:\n${JSON.stringify(parsed.error.issues, null, 2)}`
      )
    }
  })

  test(`view ids are unique`, () => {
    const seen = new Set<string>()
    const dupes: string[] = []
    for (const view of VIEWS) {
      if (seen.has(view.id)) dupes.push(view.id)
      seen.add(view.id)
    }
    expect(dupes).toEqual([])
  })

  test(`group ids are unique and every view's group exists`, () => {
    const ids = GROUPS.map((group) => group.id)
    expect(new Set(ids).size).toBe(ids.length)
    const orphans = VIEWS.filter((view) => !ids.includes(view.group)).map(
      (view) => `${view.id} → ${view.group}`
    )
    expect(orphans).toEqual([])
  })

  test(`group orders are unique`, () => {
    const orders = GROUPS.map((group) => group.order)
    expect(new Set(orders).size).toBe(orders.length)
  })

  test(`view order is unique within its group`, () => {
    const clashes: string[] = []
    for (const group of GROUPS) {
      const orders = VIEWS.filter((view) => view.group === group.id).map(
        (view) => view.order
      )
      if (new Set(orders).size !== orders.length) clashes.push(group.id)
    }
    expect(clashes).toEqual([])
  })

  test(`every view has a capture somewhere, or notes saying why not`, () => {
    const undeclared: string[] = []
    for (const view of VIEWS) {
      const captured = PLATFORMS.filter(
        (platform) => captureFor(view, platform) !== undefined
      )
      if (captured.length === 0 && !view.notes) {
        undeclared.push(view.id)
      }
    }
    expect(undeclared).toEqual([])
  })

  test(`every platform without a capture has a note explaining the gap`, () => {
    // Silent gaps are the failure mode this catalog exists to prevent: a view
    // that simply lacks an iOS entry is indistinguishable from one nobody got
    // round to. Every hole is annotated.
    const gaps: string[] = []
    for (const view of VIEWS) {
      for (const platform of DEFAULT_PLATFORMS) {
        if (captureFor(view, platform) !== undefined) continue
        if (view.notes?.[platform]) continue
        gaps.push(`${view.id} · ${platform}`)
      }
    }
    expect(gaps).toEqual([])
  })

  test(`webMobile: 'inherit' always has a web capture to inherit`, () => {
    const broken = VIEWS.filter(
      (view) => view.webMobile === `inherit` && !view.web
    ).map((view) => view.id)
    expect(broken).toEqual([])
  })

  test(`store-lane shot names are unique per platform`, () => {
    for (const key of [`ios`, `android`] as const) {
      const names = VIEWS.map((view) => view[key]?.shot).filter(
        (name): name is string => Boolean(name)
      )
      expect(new Set(names).size).toBe(names.length)
    }
  })
})

// --------------------------------------------------------------- 2. native

/** `snapshot("01_board")` → `01_board`. */
function swiftShots(source: string): string[] {
  return [...source.matchAll(/\bsnapshot\(\s*"([^"]+)"/g)].map(
    (match) => match[1]!
  )
}

/** `Screengrab.screenshot("1_board")` → `1_board`. */
function kotlinShots(source: string): string[] {
  return [...source.matchAll(/\bscreenshot\(\s*"([^"]+)"/g)].map(
    (match) => match[1]!
  )
}

function manifestShots(
  keys: readonly (`ios` | `android`)[],
  lane: `store` | `styleguide`
): string[] {
  const names = new Set<string>()
  for (const view of VIEWS) {
    for (const key of keys) {
      const capture = view[key]
      if (capture?.lane === lane) names.add(capture.shot)
    }
  }
  return [...names].sort()
}

function checkSuite(
  label: string,
  file: string,
  parse: (source: string) => string[],
  expected: string[]
) {
  if (!existsSync(file)) {
    // Sibling workstream: the styleguide suites land alongside this package.
    console.warn(
      `[view-catalog] ${label} not found at ${relative(REPO_ROOT, file)} — ` +
        `skipping the bidirectional gate. Expected ${expected.length} shot(s): ` +
        `${expected.join(`, `) || `(none)`}`
    )
    return
  }
  const actual = parse(readFileSync(file, `utf8`))
  const missingInSuite = expected.filter((name) => !actual.includes(name))
  const missingInManifest = actual.filter((name) => !expected.includes(name))
  expect({ missingInSuite, missingInManifest }).toEqual({
    missingInSuite: [],
    missingInManifest: [],
  })
}

describe(`native suites match the manifest`, () => {
  test(`iOS store lane`, () => {
    checkSuite(
      `StoreScreenshots.swift`,
      IOS_STORE,
      swiftShots,
      manifestShots([`ios`], `store`)
    )
  })

  test(`iOS styleguide lane`, () => {
    checkSuite(
      `StyleguideScreenshots.swift`,
      IOS_STYLEGUIDE,
      swiftShots,
      manifestShots([`ios`], `styleguide`)
    )
  })

  test(`Android store lane`, () => {
    checkSuite(
      `StoreScreenshotsTest.kt`,
      ANDROID_STORE,
      kotlinShots,
      manifestShots([`android`], `store`)
    )
  })

  test(`Android styleguide lane`, () => {
    checkSuite(
      `StyleguideScreenshotsTest.kt`,
      ANDROID_STYLEGUIDE,
      kotlinShots,
      manifestShots([`android`], `styleguide`)
    )
  })

  test(`the styleguide lane is name-identical on iOS and Android`, () => {
    // The whole point of the styleguide lane is side-by-side comparison; a
    // name that exists on one platform only has nothing to sit next to.
    const ios = manifestShots([`ios`], `styleguide`)
    const android = manifestShots([`android`], `styleguide`)
    expect(ios).toEqual(android)
  })

  test(`store shot names carry their catalog order`, () => {
    // The store listing is ordered by filename, so the numeric prefix is load
    // bearing: iOS pads to two digits, Android does not.
    for (const view of VIEWS) {
      if (view.ios?.lane === `store`) {
        expect(view.ios.shot).toMatch(/^\d{2}_[a-z0-9-]+$/)
      }
      if (view.android?.lane === `store`) {
        expect(view.android.shot).toMatch(/^\d_[a-z0-9-]+$/)
      }
    }
  })

  test(`styleguide shot names are sg_-prefixed`, () => {
    for (const view of VIEWS) {
      for (const key of [`ios`, `android`] as const) {
        const capture = view[key]
        if (capture?.lane === `styleguide`) {
          expect(capture.shot).toMatch(/^sg_[a-z0-9-]+$/)
        }
      }
    }
  })
})

// ---------------------------------------------------------- 3. web routes

/**
 * Turn a route FILE path into its URL path, or null when the file is not a
 * route (layouts, helpers, tests, API handlers).
 *
 * TanStack flat-route conventions: a leading `_` segment is pathless, a `.` in
 * a filename is a path separator, a trailing `_` opts out of nesting, and
 * `index` collapses to its directory.
 */
function routeUrlFromFile(rel: string): string | null {
  if (!rel.endsWith(`.tsx`)) return null
  if (rel.startsWith(`api/`)) return null
  const stem = rel.slice(0, -`.tsx`.length)
  const parts = stem.split(`/`)
  const base = parts[parts.length - 1]!
  if (base === `__root` || base === `route`) return null
  if (base.startsWith(`-`) || base.startsWith(`_`)) return null
  if (base.endsWith(`.test`)) return null
  if (parts.some((part) => part.startsWith(`-`))) return null

  const segments: string[] = []
  for (const part of parts) {
    for (const piece of part.split(`.`)) {
      const trimmed = piece.replace(/_$/, ``)
      if (trimmed === `` || trimmed === `index`) continue
      if (trimmed.startsWith(`_`)) continue
      segments.push(trimmed)
    }
  }
  return `/${segments.join(`/`)}`
}

function walk(dir: string, base = dir): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      out.push(...walk(full, base))
    } else {
      out.push(relative(base, full))
    }
  }
  return out
}

/**
 * Does a declared path satisfy a route pattern? A `$param` segment in the
 * pattern matches any concrete value, so the manifest may name real seeded
 * identifiers (`/reviews/APP-14`) against `/reviews/$issueIdentifier`.
 */
function matchesRoute(pattern: string, declared: string): boolean {
  const patternParts = pattern.split(`/`)
  const declaredParts = declared.split(`?`)[0]!.split(`/`)
  if (patternParts.length !== declaredParts.length) return false
  return patternParts.every(
    (part, index) => part.startsWith(`$`) || part === declaredParts[index]
  )
}

function declaredWebRoutes(): string[] {
  const routes: string[] = []
  for (const view of VIEWS) {
    if (view.web) routes.push(view.web.route)
    if (view.webMobile && view.webMobile !== `inherit`) {
      routes.push(view.webMobile.route)
    }
  }
  return routes
}

describe(`web route coverage`, () => {
  const routeUrls = [
    ...new Set(
      walk(ROUTES_DIR)
        .map(routeUrlFromFile)
        .filter((url): url is string => url !== null)
    ),
  ].sort()

  test(`the walker found the routes it should have`, () => {
    // A regression in routeUrlFromFile would otherwise make the gate below
    // pass by finding nothing at all.
    expect(routeUrls.length).toBeGreaterThan(30)
    expect(routeUrls).toContain(`/auth/login`)
    expect(routeUrls).toContain(`/t/$teamSlug/boards/$boardSlug`)
    expect(routeUrls).toContain(
      `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
    )
    expect(routeUrls).toContain(`/admin/teams/$teamId`)
    expect(routeUrls).not.toContain(`/api/mcp`)
  })

  test(`every route is captured by a view or excluded with a reason`, () => {
    const declared = declaredWebRoutes()
    const excluded = EXCLUDED_ROUTES.map((entry) => entry.route)
    const uncovered = routeUrls.filter(
      (url) =>
        !declared.some((route) => matchesRoute(url, route)) &&
        !excluded.some((route) => matchesRoute(url, route))
    )
    expect(uncovered).toEqual([])
  })

  test(`every excluded route still exists in apps/web`, () => {
    const stale = EXCLUDED_ROUTES.filter(
      (entry) => !routeUrls.some((url) => matchesRoute(url, entry.route))
    ).map((entry) => entry.route)
    expect(stale).toEqual([])
  })

  test(`excluded routes are listed once`, () => {
    const routes = EXCLUDED_ROUTES.map((entry) => entry.route)
    expect(new Set(routes).size).toBe(routes.length)
  })

  test(`declared web routes resolve to a real route file`, () => {
    const dangling = declaredWebRoutes().filter(
      (route) => !routeUrls.some((url) => matchesRoute(url, route))
    )
    expect(dangling).toEqual([])
  })
})

// ------------------------------------------------------------- 4. recipes

function registryRecipes(): string[] | null {
  if (!existsSync(RECIPES_FILE)) return null
  const source = readFileSync(RECIPES_FILE, `utf8`)
  const block = source.match(/RECIPES\s*(?::[^=]+)?=\s*\{([\s\S]*?)\n\}/)
  if (!block) return null
  return [...block[1]!.matchAll(/^\s*([A-Za-z][A-Za-z0-9]*)\s*[:(]/gm)].map(
    (match) => match[1]!
  )
}

describe(`recipes`, () => {
  const used = [
    ...new Set(
      VIEWS.flatMap((view) => [
        view.web?.recipe,
        view.webMobile && view.webMobile !== `inherit`
          ? view.webMobile.recipe
          : undefined,
      ]).filter((name): name is string => Boolean(name))
    ),
  ].sort()

  test(`every recipe used is in the pinned registry`, () => {
    const unknown = used.filter((name) => !PINNED_RECIPES.includes(name))
    expect(unknown).toEqual([])
  })

  test(`the pinned registry matches apps/web/scripts/lib/view-recipes.ts`, () => {
    const actual = registryRecipes()
    if (actual === null) {
      console.warn(
        `[view-catalog] apps/web/scripts/lib/view-recipes.ts not found (or its ` +
          `RECIPES map could not be parsed) — checking the pinned list only. ` +
          `Pinned: ${PINNED_RECIPES.join(`, `)}`
      )
      return
    }
    const missing = PINNED_RECIPES.filter((name) => !actual.includes(name))
    const extra = actual.filter((name) => !PINNED_RECIPES.includes(name))
    expect({ missing, extra }).toEqual({ missing: [], extra: [] })
  })

  test(`the pinned registry has no duplicates`, () => {
    expect(new Set(PINNED_RECIPES).size).toBe(PINNED_RECIPES.length)
  })
})

// ------------------------------------------------------------ 5. geometry

describe(`store geometry`, () => {
  /** Source capture sizes the nominal frames are derived from. */
  const SOURCE: Record<Platform, { w: number; h: number }> = {
    web: { w: 1440, h: 960 },
    [`web-mobile`]: { w: 390, h: 844 },
    desktop: { w: 1440, h: 900 },
    ios: { w: 1320, h: 2868 },
    android: { w: 1080, h: 2400 },
  }

  test(`every platform has a frame`, () => {
    for (const platform of PLATFORMS) {
      expect(PLATFORM_FRAME[platform]).toBeDefined()
    }
  })

  test(`every frame's long edge is 1800`, () => {
    for (const platform of PLATFORMS) {
      const frame = PLATFORM_FRAME[platform]
      expect(Math.max(frame.w, frame.h)).toBe(1800)
    }
  })

  test(`frames preserve the capture aspect ratio`, () => {
    for (const platform of PLATFORMS) {
      const frame = PLATFORM_FRAME[platform]
      const source = SOURCE[platform]
      expect(Math.abs(frame.w / frame.h - source.w / source.h)).toBeLessThan(
        0.005
      )
    }
  })

  test(`phones and tablets are portrait, browsers and the IDE landscape`, () => {
    for (const platform of [`ios`, `android`, `web-mobile`] as const) {
      expect(PLATFORM_FRAME[platform].h).toBeGreaterThan(
        PLATFORM_FRAME[platform].w
      )
    }
    for (const platform of [`web`, `desktop`] as const) {
      expect(PLATFORM_FRAME[platform].w).toBeGreaterThan(
        PLATFORM_FRAME[platform].h
      )
    }
  })
})

// ---------------------------------------------------------------- helpers

describe(`api`, () => {
  test(`shotPath addresses one file per view per platform`, () => {
    expect(shotPath(`board`, `ios`)).toBe(`${STORE_DIR}/board/ios.webp`)
    const paths = VIEWS.flatMap((view) =>
      PLATFORMS.map((platform) => shotPath(view.id, platform))
    )
    expect(new Set(paths).size).toBe(paths.length)
  })

  test(`viewById round-trips every view and misses cleanly`, () => {
    for (const view of VIEWS) {
      expect(viewById(view.id)).toBe(view as View)
    }
    expect(viewById(`no-such-view`)).toBeUndefined()
  })

  test(`viewsFor only returns views with a capture, in catalog order`, () => {
    for (const platform of PLATFORMS) {
      const views = viewsFor(platform)
      for (const view of views) {
        expect(captureFor(view, platform)).toBeDefined()
      }
      const groupOrder = new Map(GROUPS.map((group) => [group.id, group.order]))
      const keys = views.map(
        (view) => (groupOrder.get(view.group) ?? 0) * 1000 + view.order
      )
      expect(keys).toEqual([...keys].sort((a, b) => a - b))
    }
    expect(viewsFor(`web`).length).toBeGreaterThan(0)
  })

  test(`captureFor resolves webMobile inheritance`, () => {
    const inheriting = VIEWS.find((view) => view.webMobile === `inherit`)
    expect(inheriting).toBeDefined()
    expect(captureFor(inheriting!, `web-mobile`)).toBe(inheriting!.web!)

    const own = VIEWS.find(
      (view) => view.webMobile !== undefined && view.webMobile !== `inherit`
    )
    expect(own).toBeDefined()
    expect(captureFor(own!, `web-mobile`)).toBe(own!.webMobile as WebCapture)
  })

  test(`DEFAULT_PLATFORMS is every platform`, () => {
    expect([...DEFAULT_PLATFORMS]).toEqual([...PLATFORMS])
  })

  test(`the default tolerance is a sane fraction`, () => {
    expect(STORE_DEFAULT_TOLERANCE).toBeGreaterThan(0)
    expect(STORE_DEFAULT_TOLERANCE).toBeLessThan(0.05)
  })
})

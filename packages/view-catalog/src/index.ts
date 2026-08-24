/**
 * The view catalog (EXP-566) — the ONE manifest of every product view, and the
 * addressing scheme the cross-platform screenshot store is built on.
 *
 * Four clients photograph the same product. Without a shared manifest each one
 * grows its own ad-hoc list of shots, the names drift, and nobody can put a web
 * board next to an iOS board and see that they disagree. This package is that
 * list: `views.json` names each view once, says how every platform reaches it,
 * and the tests keep it honest against the code (web routes, native UI-test
 * suites, the web recipe registry).
 *
 * ZERO runtime dependencies on purpose — a capture script, a native codegen
 * step and a docs page all import this, and none of them should inherit a
 * validation library. `zod` lives in the test only.
 *
 * ## Placeholders
 *
 * The manifest is written against the SEEDED demo instance
 * (`apps/web/scripts/seed-screenshots.ts`) but never hardcodes ids that only
 * exist after a seed run. Two escaping conventions:
 *
 *   - `$teamSlug` / `$boardSlug` / `$issueIdentifier` in `web.route` are the
 *     literal TanStack Router param names. Capturers substitute them; on the
 *     demo instance that is team `acme`, board `mobile-app`.
 *   - `$NAME` inside a `DesktopDrive.value` is a runtime lookup: `issue:$APP-5`
 *     means "the UUID of the issue whose identifier is APP-5", `pr:$APP-14` the
 *     same for a PR diff, and `support:$thread` means "any open support thread"
 *     (support data is server-only tRPC, so it has no stable identifier to
 *     name). A value with no `$` is literal.
 *
 * Human-readable issue identifiers (APP-5, APP-14) DO appear in `web.route`,
 * because that is exactly what the URL carries.
 */

import catalogJson from "../views.json"

/** Every surface the store holds an image for. */
export type Platform =
  | `web`
  | `web-mobile`
  | `desktop`
  | `ios`
  | `android`
  | `ipad`

/** Group ids, kept in sync with `views.json`'s `groups`. */
export type GroupId =
  | `auth`
  | `issues`
  | `my-work`
  | `coding`
  | `reviews`
  | `actions`
  | `support`
  | `settings`
  | `getting-started`

/** A section of the catalog. Purely presentational grouping. */
export interface Group {
  id: GroupId
  label: string
  blurb: string
  order: number
}

/**
 * What the capturer waits for before it presses the shutter. `text` is a
 * SEEDED string (never a generic label like "Loading"), `testId` a
 * `data-testid`. At least one of them, or the shot races the sync.
 */
export interface Anchor {
  text?: string
  testId?: string
  timeoutMs?: number
}

/** How a browser reaches the view. */
export interface WebCapture {
  /** Router path with literal `$param` placeholders; may carry a query string. */
  route: string
  anchor: Anchor
  /** Name from the web recipe registry (`apps/web/scripts/lib/view-recipes.ts`). */
  recipe?: string
  /** Extra quiet time after the anchor lands — avatars and icon fonts trail it. */
  settleMs?: number
  fullPage?: boolean
}

/**
 * How a native UI test reaches the view. `store` = the 8 App Store / Play
 * listing shots; `styleguide` = the wider parity lane that exists to be
 * compared against web, not published.
 */
export interface NativeCapture {
  shot: string
  lane: `store` | `styleguide`
}

/**
 * How the desktop IDE is driven to the view.
 *
 *   - `screen`  → `EXP_DEV_SCREEN` (`navigation::parse_dev_screen`): `settings`,
 *                 `actions`, `getting-started`, or `issue:<uuid>`.
 *   - `tool`    → a sidebar tool window (`sidebar::ToolWindow`): `board`,
 *                 `inbox`, `my-issues`, `reviews`, `support`.
 *   - `settings`→ a `settings::SettingsSection` slug.
 *   - `manual`  → no automated path; the note says why.
 */
export type DesktopDrive =
  | { kind: `screen` | `tool` | `settings`; value: string }
  | { kind: `manual` }

export interface DesktopCapture {
  drive: DesktopDrive
  anchorDelayMs?: number
}

/**
 * One product view. A view with no capture on any platform is still a legal
 * entry — it is a DECLARED gap, and `notes` has to say what blocks it. That is
 * the point: the catalog records the whole product, not just the parts that
 * already photograph.
 */
export interface View {
  id: string
  group: GroupId
  title: string
  blurb: string
  order: number
  web?: WebCapture
  /** `inherit` = the same capture as `web`, shot at the phone viewport. */
  webMobile?: `inherit` | WebCapture
  ios?: NativeCapture
  ipad?: NativeCapture
  android?: NativeCapture
  desktop?: DesktopCapture
  /** Per-view override of `STORE_DEFAULT_TOLERANCE`. */
  diffTolerance?: number
  notes?: Partial<Record<Platform, string>>
}

/** A route that deliberately has no view, and why. */
export interface ExcludedRoute {
  route: string
  reason: string
}

export interface Catalog {
  groups: Group[]
  excludedRoutes: ExcludedRoute[]
  views: View[]
}

const catalog = catalogJson as unknown as Catalog

export const PLATFORMS: readonly Platform[] = [
  `web`,
  `web-mobile`,
  `desktop`,
  `ios`,
  `android`,
  `ipad`,
]

/**
 * The platforms a normal run captures. iPad is opt-in: it mirrors the iOS shot
 * list on a second simulator and roughly doubles the native lane's runtime.
 */
export const DEFAULT_PLATFORMS: readonly Platform[] = PLATFORMS.filter(
  (platform) => platform !== `ipad`
)

export const GROUPS: readonly Group[] = catalog.groups
export const VIEWS: readonly View[] = catalog.views
export const EXCLUDED_ROUTES: readonly ExcludedRoute[] = catalog.excludedRoutes

/** Where the images live, relative to the store root. */
export const STORE_DIR = `shots`

/** Default per-pixel diff tolerance for the parity comparison. */
export const STORE_DEFAULT_TOLERANCE = 0.005

/**
 * The NOMINAL stored size per platform. Captures happen at each platform's
 * native device resolution and are then downscaled to an 1800px long edge, so
 * these are the capture aspect ratios normalised to that edge:
 *
 *   web         1440×960  @2x  → 2880×1920 → 1800×1200
 *   web-mobile   390×844  @3x  → 1170×2532 →  832×1800
 *   desktop     1440×900  @2x  → 2880×1800 → 1800×1125
 *   ios         iPhone 17 Pro Max 1320×2868 → 828×1800
 *   android     1080×2400                   → 810×1800
 *   ipad        2064×2752                   → 1350×1800
 */
export const PLATFORM_FRAME: Record<Platform, { w: number; h: number }> = {
  web: { w: 1800, h: 1200 },
  [`web-mobile`]: { w: 832, h: 1800 },
  desktop: { w: 1800, h: 1125 },
  ios: { w: 828, h: 1800 },
  android: { w: 810, h: 1800 },
  ipad: { w: 1350, h: 1800 },
}

const BY_ID = new Map(catalog.views.map((view) => [view.id, view]))

export function viewById(id: string): View | undefined {
  return BY_ID.get(id)
}

/**
 * Every view that HAS a capture on the platform, in catalog order (group order,
 * then view order). A `webMobile: 'inherit'` counts as available — inheriting is
 * a capture instruction, not a gap.
 */
export function viewsFor(platform: Platform): View[] {
  const groupOrder = new Map(catalog.groups.map((group) => [group.id, group.order]))
  return catalog.views
    .filter((view) => captureFor(view, platform) !== undefined)
    .sort(
      (a, b) =>
        (groupOrder.get(a.group) ?? 0) - (groupOrder.get(b.group) ?? 0) ||
        a.order - b.order ||
        a.id.localeCompare(b.id)
    )
}

/**
 * The capture instruction for one view on one platform, with `inherit`
 * resolved. `undefined` means the platform has no shot for this view — check
 * `view.notes[platform]` for why.
 */
export function captureFor(
  view: View,
  platform: Platform
): WebCapture | NativeCapture | DesktopCapture | undefined {
  switch (platform) {
    case `web`:
      return view.web
    case `web-mobile`:
      return view.webMobile === `inherit` ? view.web : view.webMobile
    case `desktop`:
      return view.desktop
    case `ios`:
      return view.ios
    case `ipad`:
      return view.ipad
    case `android`:
      return view.android
  }
}

/** The stored image's path, relative to the store root. */
export function shotPath(viewId: string, platform: Platform): string {
  return `${STORE_DIR}/${viewId}/${platform}.webp`
}

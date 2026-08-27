/**
 * "Which views did this diff actually touch?" (EXP-566).
 *
 * A full `bun run shots` run photographs ~45 web views twice and relaunches the
 * desktop app once per view — half an hour of wall clock to discover that a PR
 * which renamed a tRPC field moved nothing. The unattended refresh automation
 * runs after EVERY merge, so it needs the cheap answer first: given the files
 * that landed, which `<view, platform>` pairs can possibly look different?
 *
 * The rule that makes this safe to trust is FAIL-SAFE ATTRIBUTION: a changed
 * path is only narrowed to specific views when something in the repo actually
 * proves the connection. Anything the rules cannot explain widens to every view
 * of every platform it could belong to. Over-capturing costs minutes; a missed
 * view commits a stale screenshot and nobody notices, so the asymmetry is
 * resolved in one direction, always.
 *
 * How each platform is attributed:
 *
 *   web / web-mobile  PRECISE. Every view names a router path; `routeTree.gen.ts`
 *                     maps that path to a route file and its layout ancestors;
 *                     a regex import scan over `apps/web/src` turns those into a
 *                     transitive module set (workspace packages collapse to one
 *                     `pkg:<name>` node). A changed file inside a view's module
 *                     set touches that view — and a changed `apps/web` file
 *                     inside NO view's set (a tRPC router, a shape proxy, a
 *                     server lib) widens to every web view, because data changes
 *                     are exactly as visible as markup changes.
 *   desktop           COARSE. Rust has no import graph here, so a file under
 *                     `crates/ui/src` is matched by NAME against each view's id
 *                     and drive value (`issue_detail.rs` → `issue-detail`,
 *                     `settings/general.rs` → `settings-general`), and its
 *                     wrapper-stripped path as a family prefix
 *                     (`start_coding_dialog.rs` → the three start-coding tabs).
 *                     Everything else under `apps/desktop` — shell, theme, sync,
 *                     an unmatched ui module — widens to every desktop view. The
 *                     win that matters is a web-only PR skipping the desktop
 *                     lane entirely.
 *   ios / ipad /      BY NAME, like desktop. A Swift/Kotlin file's basename
 *   android           minus its role suffix (`IssueDetailView` → `issue-detail`)
 *                     is matched against view ids and shot names, and its
 *                     DIRECTORY against a view family (`UI/Support` → the two
 *                     support views). `NATIVE_SHARED` lists what can never
 *                     narrow — ExpCore/ExpUI, themes, icons, the fastlane lanes
 *                     — and widens the platform. `ipad` rides `ios`: one lane
 *                     photographs both frames.
 *
 * Two inputs are read directly rather than inferred: `views.json` is diffed
 * ENTRY BY ENTRY against the base revision (an added or edited view is in scope
 * on every platform), and a view with no stored image yet is always in scope.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { dirname, join } from "node:path"
import {
  PLATFORMS,
  VIEWS,
  viewsFor,
  type Platform,
  type View,
} from "@exp/view-catalog"
import { run } from "./lib/proc.ts"
import { repoRoot, storeShotPath } from "./paths.ts"

/** Repo-relative, forward-slashed — the shape `git diff --name-only` prints. */
type RepoPath = string

/** Paths that cannot move a pixel in any shot, on any platform. */
const IGNORED: RegExp[] = [
  /(^|\/)[^/]*\.(test|spec)\.[a-z]+$/,
  /^apps\/web\/(tests|e2e)\//,
  /^apps\/web\/src\/test\//,
  /(^|\/)__tests__\//,
  // Native unit tests. They carry no `.test.` infix — the convention is a
  // `Tests` suffix (`SteerReconnectPolicyTests.swift`) or a test source set
  // (`app/src/test/`) — so the rule above misses them and an ExpCore test file
  // widened both iOS lanes. A test renders nothing, on any platform.
  /(^|\/)[A-Za-z0-9_]+Tests?\.(swift|kt)$/,
  /^apps\/android\/app\/src\/test\//,
  /^apps\/ios\/[^/]+\/Tests\//,
  // The CLI is gpui-free by construction; the styleguide site READS the store.
  /^apps\/desktop\/crates\/cli\//,
  /^apps\/(marketing|push-relay|steer-relay|styleguide)\//,
  // The capture pipeline itself, and packages no client renders.
  /^packages\/(shots|tsconfig|steer-ticket|electric-protocol|widget)\//,
  // Licence bookkeeping: the generated inventories, the curated overrides and
  // the notice texts. NO catalog view photographs a licence surface — `/about`
  // is an excludedRoute, `NOTICES.txt` is ignored above, and the desktop's
  // settings/about.rs has no view either. Without this rule an Android
  // dependency bump rewrites `inventory/android.json`, which matches no
  // SOURCE_ROOT and therefore widened EVERY lane on EVERY platform — the single
  // biggest source of pointless captures the automation ever hit.
  /^packages\/licenses\//,
  /^packages\/view-catalog\/(src|package\.json|tsconfig\.json)/,
  /^(docs|selfhost|shots)\//,
  /^\.github\//,
  /\.md$/,
  // Store LISTING text and artwork: release notes, descriptions, keywords.
  // fastlane's lane files are not here — those drive the capture (NATIVE_SHARED).
  /^apps\/(ios|android)\/fastlane\/metadata\//,
  // The changelog and its "What's new" card: every capture context pre-seeds
  // `exp.changelogSeenId` (capture-web.ts) precisely so the card never opens
  // over a shot, so a released entry cannot reach the store.
  /^apps\/web\/src\/lib\/changelog(-seen)?\.ts$/,
  /^apps\/web\/src\/components\/whats-new\.tsx$/,
  // Static assets no photographed screen renders: the service worker, the PWA
  // manifest, the licence dump, and the icons that live in a browser chrome the
  // capture crops away. (`logo-*.svg` is NOT here — the sidebar draws it.)
  /^apps\/web\/public\/(sw\.js|manifest\.json|NOTICES\.txt|og-card\.png|favicon\.ico|apple-touch-icon\.png|icon-\d+\.(png|jpg))$/,
  // Server surfaces with no photographed screen behind them: the MCP endpoint
  // and its scope machinery, the admin-only metrics, the GitHub/SES webhooks,
  // the unsubscribe link and the marketing contact form. None of them can move
  // a pixel on a SEEDED instance — the seed writes the state they would.
  /^apps\/web\/src\/routes\/api\/(mcp\.ts|contact\.ts|webhooks\/|email\/)/,
  /^apps\/web\/src\/lib\/(mcp|metrics)\//,
  // One-off operator scripts. Named individually rather than by a `scripts/`
  // prefix, because the capture pipeline itself lives in that directory and
  // absolutely does widen (see BROAD).
  /^apps\/web\/scripts\/(backfill-[a-z-]+|scrub-widget-descriptions|fix-server-trace|capture-social-shots)\.ts$/,
]

/** The catalog file, diffed entry by entry instead of matched by rule. */
const CATALOG_FILE: RepoPath = `packages/view-catalog/views.json`

/** The web recipe registry: a change there re-drives every recipe-driven view. */
const RECIPES_FILE: RepoPath = `apps/web/scripts/lib/view-recipes.ts`

/** Changed paths that widen to whole platforms, in match order. */
const BROAD: { test: RegExp; platforms: readonly Platform[]; why: string }[] = [
  {
    test: /^apps\/web\/scripts\/seed-screenshots\.ts$/,
    platforms: PLATFORMS,
    why: `the seeded demo data every platform photographs`,
  },
  {
    test: /^apps\/web\/scripts\/(screenshot-demo|lib\/demo-ids)\.ts$/,
    platforms: PLATFORMS,
    why: `the demo identity and ids every lane resolves against`,
  },
  {
    test: /^apps\/web\/scripts\/(capture-views\.ts|lib\/capture-web\.ts|screenshot-ids\.ts)$/,
    platforms: [`web`, `web-mobile`],
    why: `the browser capturer itself`,
  },
  {
    // The relay stub is what puts a device online: without it the desktop app,
    // the browser and the natives ALL photograph "no desktop online" states.
    // It used to widen the web lane only, which was a fail-safe bug.
    test: /^apps\/web\/scripts\/screenshot-(desktop|prune-devices)\.ts$/,
    platforms: [`web`, `web-mobile`, `desktop`],
    why: `the stand-in desktop every steering and launcher view needs online`,
  },
]

/**
 * Native paths that can never narrow to a view, in match order.
 *
 * The name-based attribution below only works on a file that IS a screen. A
 * shared component, a colour token, an icon set or the fastlane lane itself
 * feeds every shot on its platform, so it widens — the same bargain the desktop
 * lane strikes with "an unmatched ui module widens the whole lane".
 */
const NATIVE_SHARED: { test: RegExp; platforms: readonly Platform[]; why: string }[] = [
  {
    test: /^apps\/ios\/(ExpCore|ExpUI|Tuist|Project\.swift|Exponential\/(Data|Notifications|Assets\.xcassets|Resources)\/|Exponential\/UI\/(Components|Markdown|Navigation)\/|ExponentialUITests\/|fastlane\/|scripts\/)/,
    platforms: [`ios`, `ipad`],
    why: `shared iOS code the whole app renders through`,
  },
  {
    test: /^apps\/android\/(gradle|build\.gradle|settings\.gradle|fastlane\/|app\/src\/androidTest\/|app\/src\/main\/res\/|app\/src\/main\/java\/com\/exponential\/app\/(data|domain|navigation|App[A-Za-z]*\.kt|MainActivity\.kt)|app\/src\/main\/java\/com\/exponential\/app\/ui\/(components|theme|icons|emoji|markdown)\/)/,
    platforms: [`android`],
    why: `shared Android code the whole app renders through`,
  },
]

/** Where a changed path lands when nothing narrowed it — its platform roots. */
const SOURCE_ROOTS: { test: RegExp; platforms: readonly Platform[] }[] = [
  { test: /^apps\/web\//, platforms: [`web`, `web-mobile`] },
  { test: /^apps\/desktop\//, platforms: [`desktop`] },
  // One simulator lane produces both frames, so an unmapped iOS file moves both.
  { test: /^apps\/ios\//, platforms: [`ios`, `ipad`] },
  { test: /^apps\/android\//, platforms: [`android`] },
]

export interface AffectedScope {
  /** View ids per platform, in catalog order. An empty array = skip the lane. */
  byPlatform: Map<Platform, string[]>
  /** Why each view is in scope, for the run log. */
  reasons: Map<string, string[]>
  /** Changed paths that widened to whole platforms. */
  broad: { path: RepoPath; platforms: Platform[]; why: string }[]
  /** Changed paths the rules deliberately dropped. */
  ignored: RepoPath[]
  /** Every changed path considered. */
  changed: RepoPath[]
}

export interface AffectedOptions {
  changedFiles: RepoPath[]
  /** Platforms the run covers. Defaults to all of them. */
  platforms?: readonly Platform[]
  /** View ids whose `views.json` entry was added or edited. */
  catalogChanges?: string[]
  /** Also pull in views with no stored image yet. Default true. */
  includeMissing?: boolean
}

/* ------------------------------------------------------------------- the map */

/**
 * Decide the scope of a run from a list of changed files.
 *
 * Pure apart from reading the repo it is run in (the web import graph, the
 * route tree, the store) — the git plumbing lives in `changedSince` so tests
 * can hand this function a file list directly.
 */
export function affectedScope(options: AffectedOptions): AffectedScope {
  const platforms = [...(options.platforms ?? PLATFORMS)]
  const includeMissing = options.includeMissing ?? true

  const hits = new Map<Platform, Set<string>>(platforms.map((p) => [p, new Set()]))
  const reasons = new Map<string, string[]>()
  const broad: AffectedScope[`broad`] = []
  const ignored: RepoPath[] = []

  const note = (viewId: string, why: string): void => {
    const list = reasons.get(viewId) ?? []
    if (!list.includes(why)) list.push(why)
    reasons.set(viewId, list)
  }
  const add = (viewId: string, platform: Platform, why: string): void => {
    const set = hits.get(platform)
    if (!set) return
    if (!captured(viewId, platform)) return
    set.add(viewId)
    note(viewId, why)
  }
  const widen = (path: RepoPath, to: readonly Platform[], why: string): void => {
    const applied = to.filter((platform) => hits.has(platform))
    if (applied.length === 0) return
    broad.push({ path, platforms: applied, why })
    for (const platform of applied) {
      for (const view of viewsFor(platform)) add(view.id, platform, `${path}: ${why}`)
    }
  }

  for (const viewId of options.catalogChanges ?? []) {
    for (const platform of platforms) add(viewId, platform, `${CATALOG_FILE}: catalog entry changed`)
  }

  const wantsWeb = platforms.some((platform) => platform === `web` || platform === `web-mobile`)
  const web = wantsWeb ? webAttribution() : undefined
  const graph = web?.views

  for (const path of options.changedFiles) {
    if (path === CATALOG_FILE) continue // handled entry by entry above
    if (IGNORED.some((rule) => rule.test(path))) {
      ignored.push(path)
      continue
    }

    if (path === RECIPES_FILE) {
      for (const view of VIEWS) {
        if (!recipeOf(view)) continue
        add(view.id, `web`, `${path}: drives this view's recipe`)
        add(view.id, `web-mobile`, `${path}: drives this view's recipe`)
      }
      continue
    }

    const broadRule = BROAD.find((rule) => rule.test.test(path))
    if (broadRule) {
      widen(path, broadRule.platforms, broadRule.why)
      continue
    }

    let attributed = false

    if (web && web.pageRoutes.has(path) && !web.views.has(path)) {
      // A page route no view photographs — `/about`, the admin console, the
      // password-recovery forms. `views.json` says so explicitly (its
      // `excludedRoutes`), so this is a decision, not a gap: drop it rather
      // than widen the lane over a screen the store does not hold.
      ignored.push(path)
      continue
    }

    if (graph && path.startsWith(`apps/web/`)) {
      const viewIds = graph.get(path)
      if (viewIds && viewIds.length > 0) {
        for (const viewId of viewIds) {
          add(viewId, `web`, `${path}: in this view's module graph`)
          add(viewId, `web-mobile`, `${path}: in this view's module graph`)
        }
        attributed = true
      }
    }

    if (!attributed && hits.has(`desktop`)) {
      const viewIds = desktopMatches(path)
      if (viewIds.length > 0) {
        for (const viewId of viewIds) add(viewId, `desktop`, `${path}: names this view's module`)
        attributed = true
      }
    }

    if (!attributed && /^apps\/(ios|android)\//.test(path)) {
      const shared = NATIVE_SHARED.find((rule) => rule.test.test(path))
      if (shared) {
        widen(path, shared.platforms, shared.why)
        continue
      }
      // `ipad` is not a lane of its own — whatever the iOS file names, it names
      // on both frames, so the two are attributed together.
      const natives: Platform[] = path.startsWith(`apps/ios/`)
        ? [`ios`, `ipad`]
        : [`android`]
      for (const platform of natives) {
        if (!hits.has(platform)) continue
        const viewIds = nativeMatches(path, platform)
        if (viewIds.length === 0) continue
        for (const viewId of viewIds) add(viewId, platform, `${path}: names this view's screen`)
        attributed = true
      }
      if (attributed) continue
    }

    if (!attributed && graph && path.startsWith(`packages/`)) {
      // A workspace package the web app imports (`@exp/icons`, `@exp/db-schema`):
      // the graph carries it as a single `pkg:<name>` node, so it narrows to the
      // views that pull it in. The compiled clients embed the same package with
      // no graph to narrow them down, so they widen.
      const viewIds = graph.get(packageNode(path) ?? ``)
      if (viewIds && viewIds.length > 0) {
        for (const viewId of viewIds) {
          add(viewId, `web`, `${path}: imported by this view`)
          add(viewId, `web-mobile`, `${path}: imported by this view`)
        }
        // Only the WEB side is explained; other platforms still widen.
        const rest = platforms.filter((platform) => platform !== `web` && platform !== `web-mobile`)
        if (rest.length > 0) {
          widen(path, rest, `a shared package the compiled clients embed`)
        }
        attributed = true
      }
    }

    if (attributed) continue

    const root = SOURCE_ROOTS.find((rule) => rule.test.test(path))
    widen(
      path,
      root ? root.platforms : platforms,
      root ? `unmapped source file` : `a repo-wide path with no per-view mapping`
    )
  }

  if (includeMissing) {
    for (const platform of platforms) {
      for (const view of viewsFor(platform)) {
        if (existsSync(storeShotPath(view.id, platform))) continue
        add(view.id, platform, `no stored shot yet`)
      }
    }
  }

  const byPlatform = new Map<Platform, string[]>()
  for (const platform of platforms) {
    const set = hits.get(platform)!
    byPlatform.set(
      platform,
      viewsFor(platform)
        .map((view) => view.id)
        .filter((id) => set.has(id))
    )
  }
  return { byPlatform, reasons, broad, ignored, changed: [...options.changedFiles] }
}

/** Does the catalog claim a shot for this pair? */
function captured(viewId: string, platform: Platform): boolean {
  return viewsFor(platform).some((view) => view.id === viewId)
}

function recipeOf(view: View): string | undefined {
  const web = view.web ?? (view.webMobile === `inherit` ? undefined : view.webMobile)
  return web?.recipe
}

/** `packages/icons/src/x.ts` → the `pkg:icons` graph node. */
function packageNode(path: RepoPath): string | undefined {
  const match = path.match(/^packages\/([^/]+)\//)
  return match ? `pkg:${match[1]}` : undefined
}

/* ------------------------------------------------------------ desktop by name */

/**
 * Module names one desktop view answers to: its id and the drive that opens it,
 * both in `snake_case`. Runtime placeholders (`issue:$APP-5`) contribute only
 * their literal half.
 */
function desktopTokens(view: View): string[] {
  const tokens = new Set<string>([view.id.replace(/-/g, `_`)])
  const drive = view.desktop?.drive
  if (drive && `value` in drive) {
    for (const part of drive.value.split(`:`)) {
      if (!part || part.startsWith(`$`)) continue
      tokens.add(part.replace(/-/g, `_`).toLowerCase())
    }
  }
  return [...tokens]
}

/**
 * WRAPPER suffixes a desktop module carries on top of the thing it draws —
 * `notifications_prefs` is the notifications pane, `start_coding_dialog` is the
 * start-coding surface. Stripping one yields the view family the file belongs to.
 *
 * `_list`, `_row`, `_card` and `_view` are deliberately NOT here: they name
 * distinct components reused across screens, not wrappers. `issue_list.rs` is
 * pulled in by `board.rs`, `search_sheet.rs` and `sidebar.rs`, so narrowing it
 * to the issue views would silently commit a stale board shot — exactly the
 * asymmetry this module refuses to trade away.
 */
const DESKTOP_WRAPPER = /_(prefs|pane|section|dialog|popover|sheet)$/

/** Views a `crates/ui/src/**.rs` path names. Empty = widen to the whole lane. */
function desktopMatches(path: RepoPath): string[] {
  const match = path.match(/^apps\/desktop\/crates\/ui\/src\/(.+)\.rs$/)
  if (!match) return []
  const segments = match[1]!.split(`/`).filter((segment) => segment !== `mod`)
  const candidates = new Set([...segments, segments.join(`_`)])
  // The wrapper-stripped FULL path, matched as a family PREFIX so a file that
  // draws a tabbed surface claims every tab: `start_coding_dialog.rs` renders
  // start-coding, -actions and -chat, and matching only the exact stem would
  // drop two of them. Bare segments never get the prefix treatment — that would
  // turn `settings/account.rs` into all thirteen settings views.
  const stem = segments
    .map((segment) => segment.replace(DESKTOP_WRAPPER, ``))
    .filter(Boolean)
    .join(`_`)
  return VIEWS.filter((view) => {
    if (!view.desktop) return false
    const tokens = desktopTokens(view)
    if (tokens.some((token) => candidates.has(token))) return true
    return stem !== `` && tokens.some((token) => token === stem || token.startsWith(`${stem}_`))
  }).map((view) => view.id)
}

/* ------------------------------------------------------------- native by name */

/**
 * Directory → the view FAMILY it draws, for the native files whose name matches
 * nothing. `UI/Support/SupportInboxListContent.swift` is not a view id, but
 * everything in that folder feeds the two support views and nothing else.
 *
 * Families are deliberately GENEROUS supersets: over-capturing inside a family
 * costs one extra simulator shot, while a family that is too tight silently
 * commits a stale one. A directory nobody claims here widens the platform.
 */
const NATIVE_DIRS: { dir: string; views: RegExp }[] = [
  { dir: `auth`, views: /^sign-in$/ },
  { dir: `onboarding`, views: /^onboarding/ },
  { dir: `invite`, views: /^invite-accept$/ },
  { dir: `search`, views: /^search$/ },
  { dir: `inbox`, views: /^inbox$/ },
  { dir: `myissues`, views: /^my-issues$/ },
  { dir: `mywork`, views: /^my-issues$/ },
  { dir: `personal`, views: /^(inbox|my-issues)$/ },
  { dir: `support`, views: /^support-/ },
  { dir: `settings`, views: /^settings-/ },
  { dir: `actions`, views: /^(agents|actions-mobile|action-|automations)/ },
  // The coding family: the launcher's three tabs, the agents surface, the
  // machine dialogs and the steering dock all live in one folder on both
  // platforms and share their view models.
  { dir: `session`, views: /^(steering|start-coding|agents|machine-settings|add-server)/ },
  { dir: `steer`, views: /^(steering|start-coding)/ },
]

/** Role suffixes a screen's filename carries on one platform or the other. */
const NATIVE_SUFFIX = /(View|Screen|Content|Sheet|Test|Row|List)$/

/** `IssueDetailView.swift` → `issue-detail`; `OnboardingScreen.kt` → `onboarding`. */
export function nativeStem(path: RepoPath): string | undefined {
  const match = path.match(/\/([A-Za-z0-9_]+)\.(swift|kt)$/)
  if (!match) return undefined
  const base = match[1]!.replace(NATIVE_SUFFIX, ``)
  if (base === ``) return undefined
  return base
    .replace(/([a-z0-9])([A-Z])/g, `$1-$2`)
    .replace(/_/g, `-`)
    .toLowerCase()
}

/**
 * Names one native view answers to: its id plus its shot names with the lane
 * prefix stripped (`sg_board-filters` → `board-filters`, `01_board` → `board`).
 */
function nativeTokens(view: View): string[] {
  const tokens = new Set<string>([view.id])
  for (const capture of [view.ios, view.ipad, view.android]) {
    if (!capture) continue
    tokens.add(capture.shot.replace(/^(sg_|\d+_)/, ``))
  }
  return [...tokens]
}

/**
 * Views a Swift/Kotlin path names — the native mirror of `desktopMatches`.
 * Empty = nothing proved a connection, so the caller widens the platform.
 */
export function nativeMatches(path: RepoPath, platform: Platform): string[] {
  const stem = nativeStem(path)
  const segments = path.toLowerCase().split(`/`)
  const family = NATIVE_DIRS.find((entry) => segments.includes(entry.dir))
  const ids = new Set<string>()
  for (const view of viewsFor(platform)) {
    if (stem && nativeTokens(view).includes(stem)) ids.add(view.id)
    else if (family && family.views.test(view.id)) ids.add(view.id)
  }
  return [...ids]
}

/* ------------------------------------------------------------------ web graph */

const WEB_SRC = `apps/web/src`
const RESOLVE_SUFFIXES = [``, `.ts`, `.tsx`, `.css`, `/index.ts`, `/index.tsx`]

interface WebAttribution {
  /** `module path → the view ids that render it`. */
  views: Map<string, string[]>
  /** Page route files (API handlers excluded) — a route photographed or not. */
  pageRoutes: Set<string>
}

let cachedAttribution: WebAttribution | undefined

/**
 * `module path → the view ids that render it`.
 *
 * Built once per process: walk `apps/web/src`, extract every import specifier,
 * resolve the ones that stay inside the repo, then flood outward from each
 * view's route file (plus its layout ancestors, which draw the sidebar that is
 * in every shot).
 */
function webAttribution(): WebAttribution {
  if (cachedAttribution) return cachedAttribution

  const deps = moduleGraph()
  const routes = routeFiles()
  const attribution = new Map<string, string[]>()
  const pageRoutes = new Set(
    [...routes.values()].map((entry) => entry.file).filter((file) => !file.includes(`/routes/api/`))
  )

  for (const view of VIEWS) {
    const web = view.web ?? (view.webMobile === `inherit` ? undefined : view.webMobile)
    if (!web) continue
    const entries = routeChain(web.route, routes)
    if (entries.length === 0) continue
    for (const module of reachable(entries, deps)) {
      const list = attribution.get(module) ?? []
      list.push(view.id)
      attribution.set(module, list)
    }
  }
  cachedAttribution = { views: attribution, pageRoutes }
  return cachedAttribution
}

/** Only for tests: drop the memoised graph so a fixture repo is re-read. */
export function resetWebAttribution(): void {
  cachedAttribution = undefined
}

function reachable(entries: string[], deps: Map<string, string[]>): Set<string> {
  const seen = new Set<string>()
  const stack = [...entries]
  while (stack.length > 0) {
    const current = stack.pop()!
    if (seen.has(current)) continue
    seen.add(current)
    for (const next of deps.get(current) ?? []) stack.push(next)
  }
  return seen
}

function moduleGraph(): Map<string, string[]> {
  const root = repoRoot()
  const graph = new Map<string, string[]>()
  for (const file of walk(join(root, WEB_SRC))) {
    const rel = file.slice(root.length + 1)
    if (!/\.(ts|tsx)$/.test(rel)) continue
    const edges: string[] = []
    for (const spec of importSpecifiers(readFileSync(file, `utf8`))) {
      const resolved = resolveSpec(spec, rel, root)
      if (resolved) edges.push(resolved)
    }
    graph.set(rel, edges)
  }
  return graph
}

function walk(dir: string): string[] {
  const out: string[] = []
  let entries: string[]
  try {
    entries = readdirSync(dir)
  } catch {
    return out
  }
  for (const name of entries) {
    if (name === `node_modules` || name.startsWith(`.`)) continue
    const full = join(dir, name)
    if (statSync(full).isDirectory()) out.push(...walk(full))
    else out.push(full)
  }
  return out
}

const IMPORT_RE =
  /\bimport\s+["'`]([^"'`]+)["'`]|\bfrom\s+["'`]([^"'`]+)["'`]|\bimport\s*\(\s*["'`]([^"'`]+)["'`]/g

/**
 * Type-only imports, erased before the graph is built.
 *
 * They are not an edge: a type cannot render. And one of them is load-bearing
 * here — `trpc-client.ts` imports the router TYPE from `routes/api/trpc/$.ts`,
 * which would otherwise put every server router, every db helper and every
 * auth lib inside the module set of every page that can call tRPC, i.e. all of
 * them. Erasing type imports keeps a server change honestly reported as "this
 * widened the whole lane" instead of dressing it up as per-view attribution.
 */
const TYPE_IMPORT_RE =
  /\b(?:import|export)\s+type\s+(?:\{[^}]*\}|[A-Za-z_$][\w$]*|\*\s+as\s+[A-Za-z_$][\w$]*)\s+from\s*["'`][^"'`]+["'`]/g

function importSpecifiers(source: string): string[] {
  const out: string[] = []
  for (const match of source.replace(TYPE_IMPORT_RE, ``).matchAll(IMPORT_RE)) {
    const spec = match[1] ?? match[2] ?? match[3]
    if (spec) out.push(spec)
  }
  return out
}

/**
 * A specifier as a graph node: a repo-relative file, `pkg:<name>` for a
 * workspace package, or `undefined` for node_modules and builtins (nothing in
 * the repo can change them, so they are not worth an edge).
 */
function resolveSpec(spec: string, fromRel: RepoPath, root: string): string | undefined {
  const clean = spec.split(`?`)[0]!
  if (clean.startsWith(`@exp/`)) return `pkg:${clean.slice(`@exp/`.length).split(`/`)[0]}`
  let base: string
  if (clean.startsWith(`@/`)) base = `${WEB_SRC}/${clean.slice(2)}`
  else if (clean.startsWith(`.`)) base = normalize(`${dirname(fromRel)}/${clean}`)
  else return undefined
  for (const suffix of RESOLVE_SUFFIXES) {
    const candidate = `${base}${suffix}`
    if (existsSync(join(root, candidate)) && statSync(join(root, candidate)).isFile()) {
      return candidate
    }
  }
  return undefined
}

function normalize(path: string): string {
  const out: string[] = []
  for (const segment of path.split(`/`)) {
    if (segment === `.` || segment === ``) continue
    if (segment === `..`) out.pop()
    else out.push(segment)
  }
  return out.join(`/`)
}

interface RouteEntry {
  file: RepoPath
  parent?: string
}

/**
 * `fullPath → route file + parent`, read out of the generated route tree —
 * the only place that mapping exists, and it is regenerated by the router
 * plugin, so it never drifts from the filesystem.
 */
function routeFiles(): Map<string, RouteEntry> {
  const root = repoRoot()
  const source = readFileSync(join(root, WEB_SRC, `routeTree.gen.ts`), `utf8`)
  const files = new Map<string, string>()
  for (const match of source.matchAll(
    /^import \{ Route as (\w+) \} from ['"]\.\/(routes\/[^'"]+)['"]/gm
  )) {
    const rel = `${WEB_SRC}/${match[2]!}`
    for (const suffix of [`.tsx`, `.ts`]) {
      if (existsSync(join(root, `${rel}${suffix}`))) {
        files.set(match[1]!, `${rel}${suffix}`)
        break
      }
    }
  }

  const byName = new Map<string, RouteEntry>()
  const byPath = new Map<string, RouteEntry>()
  for (const match of source.matchAll(
    /fullPath: '([^']*)'\s*\n\s*preLoaderRoute: typeof (\w+)\s*\n\s*parentRoute: typeof (\w+)/g
  )) {
    const file = files.get(match[2]!)
    if (!file) continue
    const entry: RouteEntry = { file, parent: match[3]! }
    byName.set(match[2]!, entry)
    byPath.set(trimSlash(match[1]!), entry)
  }
  // Parents are named by their import symbol, not their path — keep both keyed
  // in one map so `routeChain` can walk up without a second lookup table.
  for (const [name, entry] of byName) byPath.set(`@${name}`, entry)
  for (const [name, file] of files) {
    if (!byName.has(name)) byPath.set(`@${name}`, { file })
  }
  return byPath
}

function trimSlash(path: string): string {
  return path.length > 1 ? path.replace(/\/+$/, ``) : path
}

/** The route file for a view's URL plus every layout it renders inside. */
function routeChain(route: string, routes: Map<string, RouteEntry>): string[] {
  const wanted = trimSlash(route.split(`?`)[0]!)
  let entry: RouteEntry | undefined
  for (const [key, candidate] of routes) {
    if (key.startsWith(`@`)) continue
    if (routeMatches(key, wanted)) {
      entry = candidate
      break
    }
  }
  if (!entry) return []
  const chain: string[] = []
  let current: RouteEntry | undefined = entry
  const guard = new Set<string>()
  while (current && !guard.has(current.file)) {
    guard.add(current.file)
    chain.push(current.file)
    // A block names its own file by the IMPORT symbol (`FooRouteImport`) but
    // its parent by the ROUTE constant (`FooRoute`) — the same symbol minus the
    // suffix. Try both, or every chain stops at its first layout.
    current = current.parent
      ? (routes.get(`@${current.parent}`) ?? routes.get(`@${current.parent}Import`))
      : undefined
  }
  return chain
}

/** Segment-wise match where a `$param` in the route tree eats any one segment. */
function routeMatches(pattern: string, route: string): boolean {
  const a = pattern.split(`/`)
  const b = route.split(`/`)
  if (a.length !== b.length) return false
  return a.every((segment, index) => segment.startsWith(`$`) || segment === b[index])
}

/* ---------------------------------------------------------------------- git */

/** The last commit that touched the store — the automation's natural baseline. */
export async function lastStoreCommit(): Promise<string | undefined> {
  const result = await run({
    cmd: [`git`, `log`, `-1`, `--format=%H`, `--`, `shots/`],
    cwd: repoRoot(),
    timeoutMs: 60_000,
  })
  const sha = result.stdout.trim()
  return sha === `` ? undefined : sha
}

/**
 * Files that changed since `ref`, working tree included. `--no-renames` on
 * purpose: a rename must list BOTH paths, or the view that rendered the old
 * file is silently dropped from the scope.
 */
export async function changedSince(ref: string): Promise<RepoPath[]> {
  const result = await run({
    cmd: [`git`, `diff`, `--name-only`, `--no-renames`, ref],
    cwd: repoRoot(),
    timeoutMs: 120_000,
  })
  if (result.code !== 0) {
    throw new Error(`git diff against ${ref} failed: ${result.stderr.trim() || `exit ${result.code}`}`)
  }
  return result.stdout
    .split(`\n`)
    .map((line) => line.trim())
    .filter(Boolean)
}

/**
 * View ids whose catalog entry differs from `ref`. A view the base revision
 * never had counts as changed, which is how a merged PR's new view gets its
 * first shot.
 */
export async function catalogChangesSince(ref: string): Promise<string[]> {
  const result = await run({
    cmd: [`git`, `show`, `${ref}:${CATALOG_FILE}`],
    cwd: repoRoot(),
    timeoutMs: 60_000,
  })
  const current = VIEWS
  if (result.code !== 0) return current.map((view) => view.id) // no baseline: take all
  let before: View[]
  try {
    before = (JSON.parse(result.stdout) as { views: View[] }).views ?? []
  } catch {
    return current.map((view) => view.id)
  }
  const previous = new Map(before.map((view) => [view.id, JSON.stringify(view)]))
  return current
    .filter((view) => previous.get(view.id) !== JSON.stringify(view))
    .map((view) => view.id)
}

/** Everything the CLI does, so `capture-all` can reuse it in-process. */
export async function scopeSince(
  ref: string,
  platforms: readonly Platform[]
): Promise<AffectedScope> {
  const [changedFiles, catalogChanges] = await Promise.all([
    changedSince(ref),
    catalogChangesSince(ref),
  ])
  return affectedScope({ changedFiles, platforms, catalogChanges })
}

/* ---------------------------------------------------------------------- main */

async function main(): Promise<number> {
  const argv = process.argv.slice(2)
  const flag = (name: string): string | undefined => {
    const index = argv.indexOf(`--${name}`)
    return index === -1 ? undefined : argv[index + 1]
  }
  const requested = flag(`platform`)
    ?.split(`,`)
    .map((value) => value.trim())
    .filter(Boolean) as Platform[] | undefined
  const platforms = requested ?? [...PLATFORMS]
  const unknown = platforms.filter((platform) => !PLATFORMS.includes(platform))
  if (unknown.length > 0) throw new Error(`unknown platform(s): ${unknown.join(`, `)}`)

  const sinceFlag = flag(`since`)
  const ref = sinceFlag && sinceFlag !== `auto` ? sinceFlag : await lastStoreCommit()
  if (!ref) {
    throw new Error(`no --since given and nothing has ever been committed under shots/`)
  }
  const scope = await scopeSince(ref, platforms)

  if (argv.includes(`--json`)) {
    console.log(
      JSON.stringify(
        {
          since: ref,
          platforms: Object.fromEntries(scope.byPlatform),
          views: [...new Set([...scope.byPlatform.values()].flat())].sort(),
          changed: scope.changed.length,
          broad: scope.broad.map((entry) => ({
            path: entry.path,
            platforms: entry.platforms,
            why: entry.why,
          })),
        },
        undefined,
        2
      )
    )
    return 0
  }

  console.log(
    `shots: ${scope.changed.length} changed file(s) since ${ref.slice(0, 8)}` +
      ` (${scope.ignored.length} ignored)`
  )
  for (const [platform, views] of scope.byPlatform) {
    console.log(
      `  ${platform.padEnd(12)}${views.length === 0 ? `(nothing — skip this lane)` : `${views.length}/${viewsFor(platform).length}: ${views.join(`, `)}`}`
    )
  }
  const widened = dedupeBroad(scope.broad)
  if (widened.length > 0) {
    console.log(`\nwidened to whole lanes:`)
    for (const entry of widened) {
      console.log(`  ${entry.path} → ${entry.platforms.join(`, `)} (${entry.why})`)
    }
  }
  return 0
}

function dedupeBroad(broad: AffectedScope[`broad`]): AffectedScope[`broad`] {
  const seen = new Set<string>()
  return broad.filter((entry) => {
    const key = `${entry.path}|${entry.platforms.join(`,`)}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

if (import.meta.main) {
  try {
    process.exit(await main())
  } catch (error) {
    console.error(`shots:affected: ${error instanceof Error ? error.message : String(error)}`)
    process.exit(1)
  }
}

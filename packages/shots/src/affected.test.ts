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
import { PLATFORMS, viewById, viewsFor } from "@exp/view-catalog"
import {
  affectedScope,
  baselineSkipNote,
  CAPTURE_COMMIT_SUBJECT,
  inlineTestRegion,
  parseUnifiedHunks,
  pickStoreBaseline,
  rustInlineTestOnly,
  versionOnlyChange,
} from "./affected.ts"

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

  test(`a wrapper suffix still narrows to the pane it draws`, () => {
    // `notifications_prefs` is the notifications pane. Before the suffix strip
    // it matched no view id and widened all ~48 desktop shots for a one-row
    // settings change — the EXP-638 refresh paid exactly that.
    const result = scope(`apps/desktop/crates/ui/src/settings/notifications_prefs.rs`)
    expect(views(result, `desktop`)).toEqual([`settings-notifications`])
  })

  test(`a tabbed surface claims every one of its tabs`, () => {
    // The ONE start-coding dialog draws all three tabs, so matching only the
    // exact stem would leave two of them stale.
    const result = scope(`apps/desktop/crates/ui/src/start_coding_dialog.rs`)
    expect(views(result, `desktop`).sort()).toEqual([
      `start-coding`,
      `start-coding-actions`,
      `start-coding-chat`,
    ])
  })

  test(`a component reused across screens still widens`, () => {
    // `_list` is NOT a wrapper suffix: issue_list.rs is pulled in by board.rs,
    // search_sheet.rs and sidebar.rs, so narrowing it to the issue views would
    // silently commit a stale board shot.
    const result = scope(`apps/desktop/crates/ui/src/issue_list.rs`)
    expect(views(result, `desktop`)).toHaveLength(viewsFor(`desktop`).length)
  })

  test(`the markdown modules narrow to the views that show a body`, () => {
    // EXP-657: `markdown/editor.rs` names no view, so it fell through to the
    // whole-lane widen for a diff that can only move a description or comment.
    const result = scope(
      `apps/desktop/crates/ui/src/markdown/editor.rs`,
      `apps/desktop/crates/ui/src/wysiwyg/toolbar.rs`,
      `apps/desktop/crates/gpui-markdown-editor/src/lib.rs`
    )
    expect(result.broad).toEqual([])
    expect(views(result, `desktop`)).toContain(`issue-detail`)
    expect(views(result, `desktop`)).toContain(`issue-create`)
    expect(views(result, `desktop`)).not.toContain(`settings-general`)
    expect(views(result, `web`)).toEqual([])
  })

  test(`ui-crate plumbing draws nothing; the app entry point still widens`, () => {
    // EXP-645: `lib.rs` is the module list, `window_hooks.rs` the host
    // callbacks — both widened all 48 views for a `mod` line. `main.rs` loads
    // fonts and runs theme::init, so it keeps widening on purpose.
    const plumbing = scope(
      `apps/desktop/crates/ui/src/lib.rs`,
      `apps/desktop/crates/ui/src/window_hooks.rs`
    )
    expect(views(plumbing, `desktop`)).toEqual([])
    expect(plumbing.ignored).toHaveLength(2)

    const entry = scope(`apps/desktop/crates/app/src/main.rs`)
    expect(views(entry, `desktop`)).toHaveLength(viewsFor(`desktop`).length)
  })

  test(`a bare directory segment never becomes a family prefix`, () => {
    // Otherwise `settings/account.rs` would claim all thirteen settings views.
    const result = scope(`apps/desktop/crates/ui/src/settings/account.rs`)
    expect(views(result, `desktop`)).toEqual([`settings-account`])
  })
})

describe(`native attribution`, () => {
  const NATIVE = [`ios`, `android`] as const

  function nativeScope(...changedFiles: string[]) {
    return affectedScope({ changedFiles, platforms: NATIVE, includeMissing: false })
  }

  function nativeViews(
    result: ReturnType<typeof nativeScope>,
    platform: (typeof NATIVE)[number]
  ): string[] {
    return result.byPlatform.get(platform) ?? []
  }

  test(`a screen file narrows to the view it is named after`, () => {
    const result = nativeScope(`apps/ios/Exponential/UI/Issue/IssueDetailView.swift`)
    expect(nativeViews(result, `ios`)).toEqual([`issue-detail`])
    expect(nativeViews(result, `android`)).toEqual([])
  })

  test(`a Kotlin screen narrows the same way`, () => {
    const result = nativeScope(
      `apps/android/app/src/main/java/com/exponential/app/ui/onboarding/OnboardingScreen.kt`
    )
    expect(nativeViews(result, `android`)).toEqual([`onboarding`])
    expect(nativeViews(result, `ios`)).toEqual([])
  })

  test(`an unnamed file falls back to its directory's view family`, () => {
    // `SupportInboxListContent` is nobody's view id, but nothing in UI/Support
    // draws anything except the two support views.
    const result = nativeScope(`apps/ios/Exponential/UI/Support/SupportInboxListContent.swift`)
    expect(nativeViews(result, `ios`).sort()).toEqual([`support-inbox`, `support-thread`])
    expect(nativeViews(result, `ios`)).not.toContain(`board`)
  })

  test(`the auth family is the sign-in view`, () => {
    const result = nativeScope(`apps/ios/Exponential/UI/Auth/LoginView.swift`)
    expect(nativeViews(result, `ios`)).toEqual([`sign-in`])
  })

  test(`shared native code widens its platform and only its platform`, () => {
    for (const path of [
      `apps/ios/ExpCore/Sources/Session.swift`,
      `apps/ios/Exponential/UI/Components/AppIcon.swift`,
      `apps/ios/ExponentialUITests/ScreenshotFlow.swift`,
    ]) {
      const result = nativeScope(path)
      expect(nativeViews(result, `ios`)).toHaveLength(viewsFor(`ios`).length)
      expect(nativeViews(result, `android`)).toEqual([])
    }
    for (const path of [
      `apps/android/app/src/main/java/com/exponential/app/ui/theme/Color.kt`,
      `apps/android/app/src/main/java/com/exponential/app/data/api/Client.kt`,
      `apps/android/app/src/main/res/values/strings.xml`,
    ]) {
      const result = nativeScope(path)
      expect(nativeViews(result, `android`)).toHaveLength(viewsFor(`android`).length)
      expect(nativeViews(result, `ios`)).toEqual([])
    }
  })

  test(`the markdown family narrows to the views that show a body`, () => {
    // EXP-657: EXP-653 and EXP-655 both lived entirely in the editor/renderer
    // files and each widened every native lane to 100%. Their pixels land in
    // the views that render a description, a comment or a body — not in the
    // settings screens.
    const ios = nativeScope(
      `apps/ios/Exponential/UI/Markdown/MarkdownEditor.swift`,
      `apps/ios/ExpUI/Sources/IssueEditorModel.swift`,
      `apps/ios/ExpUI/Sources/MarkdownChips.swift`,
      `apps/ios/ExpUI/Sources/IssueRefs.swift`
    )
    expect(ios.broad).toEqual([])
    expect(nativeViews(ios, `ios`)).toContain(`issue-detail`)
    expect(nativeViews(ios, `ios`)).toContain(`issue-create`)
    expect(nativeViews(ios, `ios`)).not.toContain(`settings-root`)
    expect(nativeViews(ios, `android`)).toEqual([])

    const android = nativeScope(
      `apps/android/app/src/main/java/com/exponential/app/ui/markdown/IssueRefChips.kt`,
      `apps/android/app/src/main/java/com/exponential/app/ui/markdown/MarkdownView.kt`
    )
    expect(android.broad).toEqual([])
    expect(nativeViews(android, `android`)).toContain(`issue-detail`)
    expect(nativeViews(android, `android`)).not.toContain(`settings-root`)
    expect(nativeViews(android, `ios`)).toEqual([])

    // The family rule is exact about its files: the rest of ExpUI and the
    // shared components still widen.
    const shared = nativeScope(
      `apps/ios/ExpUI/Sources/GlassSheet.swift`,
      `apps/ios/Exponential/UI/Components/AppIcon.swift`
    )
    expect(nativeViews(shared, `ios`)).toHaveLength(viewsFor(`ios`).length)
  })

  test(`an unmapped native file still widens — never silently drops`, () => {
    const result = nativeScope(
      `apps/android/app/src/main/java/com/exponential/app/ui/issue/IssueListScreen.kt`
    )
    expect(nativeViews(result, `android`)).toHaveLength(viewsFor(`android`).length)
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

  test(`the demo identity widens every lane, the relay stub the live ones`, () => {
    // Both used to be web-only rules. The stub is what puts a device ONLINE, so
    // without it the desktop app photographs "no desktop online" too — a
    // fail-safe bug, not a narrowing.
    const identity = affectedScope({
      changedFiles: [`apps/web/scripts/screenshot-demo.ts`],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    for (const platform of PLATFORMS) {
      expect(identity.byPlatform.get(platform)).toHaveLength(viewsFor(platform).length)
    }

    const stub = scope(`apps/web/scripts/screenshot-prune-devices.ts`)
    for (const platform of WEB_LANES) {
      expect(views(stub, platform)).toHaveLength(viewsFor(platform).length)
    }
  })

  test(`server surfaces and assets no shot renders are dropped, not widened`, () => {
    const result = scope(
      `apps/web/src/lib/changelog.ts`,
      `apps/web/src/components/whats-new.tsx`,
      `apps/web/public/sw.js`,
      `apps/web/src/routes/api/webhooks/github.ts`,
      `apps/web/src/routes/api/mcp.ts`,
      `apps/web/src/lib/metrics/server-timing.ts`,
      `apps/web/scripts/backfill-default-branches.ts`
    )
    for (const platform of WEB_LANES) expect(views(result, platform)).toEqual([])
    expect(result.ignored).toHaveLength(7)
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

  test(`licence bookkeeping photographs nothing on any platform`, () => {
    // No catalog view holds a licence surface: `/about` is an excludedRoute and
    // NOTICES.txt is ignored. `inventory/android.json` matches no SOURCE_ROOT,
    // so before this rule an Android dependency bump widened ALL SIX lanes —
    // it is what turned the EXP-625/638 refresh into 87 pointless web shots.
    const result = affectedScope({
      changedFiles: [
        `packages/licenses/inventory/android.json`,
        `packages/licenses/curated/npm.json`,
      ],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    for (const platform of PLATFORMS) expect(result.byPlatform.get(platform)).toEqual([])
    expect(result.broad).toEqual([])
  })

  test(`native unit tests render nothing`, () => {
    // They carry a `Tests` SUFFIX rather than a `.test.` infix, so the generic
    // rule missed them and one ExpCore test file widened both iOS lanes.
    const result = affectedScope({
      changedFiles: [
        `apps/ios/ExpCore/Tests/SteerReconnectPolicyTests.swift`,
        `apps/android/app/src/test/java/com/exponential/app/data/steer/SteerConnectionTest.kt`,
      ],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    for (const platform of PLATFORMS) expect(result.byPlatform.get(platform)).toEqual([])
    expect(result.broad).toEqual([])
  })

  test(`the terminal crate narrows to the terminal views, not the whole lane`, () => {
    // EXP-652: the rio-vt migration rewrote 13 files under `crates/terminal/`
    // and, because only `crates/ui/src` names screens, widened all 48 desktop
    // views. Every pixel the crate draws is inside a terminal grid.
    const result = affectedScope({
      changedFiles: [
        `apps/desktop/crates/terminal/src/element.rs`,
        `apps/desktop/crates/terminal/src/emulator.rs`,
        `apps/desktop/crates/terminal/Cargo.toml`,
      ],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    expect(result.broad).toEqual([])
    expect(result.byPlatform.get(`desktop`)).toContain(`terminal`)
    // Every view it claims renders a terminal grid; the board behind the dock
    // is NOT one of them.
    for (const viewId of result.byPlatform.get(`desktop`) ?? []) {
      expect([`terminal`, `steering`]).toContain(viewId)
    }
    for (const platform of PLATFORMS) {
      if (platform === `desktop`) continue
      expect(result.byPlatform.get(platform)).toEqual([])
    }
  })

  test(`the dock CHROME keeps widening — its collapsed strip is in every shot`, () => {
    // The narrowing above stops at the crate boundary on purpose.
    // `crates/ui/src/terminal_dock.rs` draws the 29px collapsed "Terminal"
    // toggle strip that sits at the bottom of EVERY desktop view (look at the
    // foot of `shots/board/desktop.webp`), plus the tab bar and the open/close
    // slide. `_dock` is deliberately absent from DESKTOP_WRAPPER for exactly
    // this reason, and this test is what stops someone adding it.
    const result = affectedScope({
      changedFiles: [`apps/desktop/crates/ui/src/terminal_dock.rs`],
      platforms: [`desktop`],
      includeMissing: false,
    })
    expect(result.byPlatform.get(`desktop`)).toHaveLength(viewsFor(`desktop`).length)
    expect(result.broad.map((entry) => entry.path)).toEqual([
      `apps/desktop/crates/ui/src/terminal_dock.rs`,
    ])
  })

  test(`desktop paths that draw nothing are dropped, not widened`, () => {
    const result = affectedScope({
      changedFiles: [
        // OS notification banners: drawn by the notification centre.
        `apps/desktop/crates/ui/src/os_notifications.rs`,
        // The scrubbed feed the OTHER clients render, off the relay.
        `apps/desktop/crates/steer/src/activity.rs`,
        `apps/desktop/crates/steer/src/codex_activity.rs`,
        `apps/desktop/crates/steer/src/publisher.rs`,
        // Cargo's own test convention — no `.test.` infix to match.
        `apps/desktop/crates/terminal/tests/headless.rs`,
        // Licence notices and the lockfile.
        `apps/desktop/assets/licenses/NOTICES.txt`,
        `apps/desktop/Cargo.lock`,
      ],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    for (const platform of PLATFORMS) expect(result.byPlatform.get(platform)).toEqual([])
    expect(result.broad).toEqual([])
    expect(result.ignored).toHaveLength(7)
  })

  test(`generated migrations and cargo examples are dropped, not widened`, () => {
    // EXP-664: drizzle output widened both web lanes for migration DDL, and
    // the steer crate's example binaries widened the desktop lane for targets
    // the app never links.
    const result = affectedScope({
      changedFiles: [
        `apps/web/src/db/out/0087_fantastic_morbius.sql`,
        `apps/web/src/db/out/meta/0087_snapshot.json`,
        `apps/web/src/db/out/meta/_journal.json`,
        `apps/desktop/crates/steer/examples/exp611_plan_approval.rs`,
        `apps/desktop/crates/coding/tests/dry_run.rs`,
      ],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    for (const platform of PLATFORMS) expect(result.byPlatform.get(platform)).toEqual([])
    expect(result.broad).toEqual([])
    expect(result.ignored).toHaveLength(5)
  })

  test(`the steer crate keeps widening where it can be seen`, () => {
    // Only the three feed/publisher files are dropped. `lib.rs` owns
    // `persistent_device_id`, which decides which synced `devices` row the
    // machine-settings view calls "this machine".
    const result = affectedScope({
      changedFiles: [`apps/desktop/crates/steer/src/lib.rs`],
      platforms: [`desktop`],
      includeMissing: false,
    })
    expect(result.ignored).toEqual([])
    expect(result.byPlatform.get(`desktop`)).toHaveLength(viewsFor(`desktop`).length)
  })

  test(`the iOS reconnect policy draws nothing; its web/Android peers still do`, () => {
    // EXP-652: pure policy (a phase plus two booleans in, an enum out), but it
    // lives in ExpCore, so NATIVE_SHARED widened both simulator lanes.
    const policy = affectedScope({
      changedFiles: [`apps/ios/ExpCore/Sources/SteerReconnectPolicy.swift`],
      platforms: PLATFORMS,
      includeMissing: false,
    })
    for (const platform of PLATFORMS) expect(policy.byPlatform.get(platform)).toEqual([])
    expect(policy.broad).toEqual([])

    // Their peers hold user-visible phase `detail` strings next to the socket
    // lifecycle, so they must NOT be ignored alongside it.
    const android = affectedScope({
      changedFiles: [
        `apps/android/app/src/main/java/com/exponential/app/data/steer/SteerConnection.kt`,
      ],
      platforms: [`android`],
      includeMissing: false,
    })
    expect(android.ignored).toEqual([])
    expect(android.byPlatform.get(`android`)).toHaveLength(viewsFor(`android`).length)

    const web = affectedScope({
      changedFiles: [`apps/web/src/lib/steer-session-store.ts`],
      platforms: [`web`],
      includeMissing: false,
    })
    expect(web.ignored).toEqual([])
    expect((web.byPlatform.get(`web`) ?? []).length).toBeGreaterThan(0)
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

  test(`a manual-drive view is never pulled in by the missing-image rule`, () => {
    // EXP-647: `steering` on the desktop is `drive: manual` (the app IS the
    // session host; there is no dock to open), so no automated run can ever
    // store it — and it was listed on every refresh, forever.
    expect(viewById(`steering`)?.desktop?.drive.kind).toBe(`manual`)
    const result = affectedScope({ changedFiles: [], platforms: [`desktop`] })
    expect(result.byPlatform.get(`desktop`)).not.toContain(`steering`)
    // A diff that names it still attributes to it — only the missing rule skips.
    const named = affectedScope({
      changedFiles: [`apps/desktop/crates/terminal/src/element.rs`],
      platforms: [`desktop`],
      includeMissing: false,
    })
    expect(named.byPlatform.get(`desktop`)).toContain(`steering`)
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

describe(`content drops`, () => {
  test(`a path a content check dropped is reported as ignored, not widened`, () => {
    const result = affectedScope({
      changedFiles: [`apps/ios/Project.swift`, `apps/desktop/crates/ui/src/sidebar.rs`],
      platforms: PLATFORMS,
      includeMissing: false,
      contentIgnored: [{ path: `apps/ios/Project.swift`, why: `version-only bump` }],
    })
    expect(result.ignored).toEqual([`apps/ios/Project.swift`])
    expect(result.contentIgnored).toEqual([{ path: `apps/ios/Project.swift`, why: `version-only bump` }])
    expect(result.byPlatform.get(`ios`)).toEqual([])
    // The other path still takes the normal route.
    expect(result.byPlatform.get(`desktop`)).toHaveLength(viewsFor(`desktop`).length)
  })

  describe(`version-only bumps (EXP-649)`, () => {
    const gradle = (code: number, name: string, extra = `implementation("a:b:1.0")`) =>
      [`android {`, `    defaultConfig {`, `        versionCode = ${code}`, `        versionName = "${name}"`, `    }`, `    ${extra}`, `}`, ``].join(`\n`)

    test(`a gradle bump that moves only the version lines is dropped`, () => {
      expect(
        versionOnlyChange(`apps/android/app/build.gradle.kts`, gradle(101, `0.14.18`), gradle(102, `0.14.19`))
      ).toBe(true)
    })

    test(`a gradle bump next to a dependency change is kept`, () => {
      expect(
        versionOnlyChange(
          `apps/android/app/build.gradle.kts`,
          gradle(101, `0.14.18`),
          gradle(102, `0.14.19`, `implementation("a:b:2.0")`)
        )
      ).toBe(false)
    })

    test(`Project.swift and Cargo.toml bumps are dropped`, () => {
      const swift = (marketing: string, build: string) =>
        `import ProjectDescription\nlet appMarketingVersion = "${marketing}"\nlet appBuildVersion = "${build}"\nlet project = Project(name: "Exponential")\n`
      expect(versionOnlyChange(`apps/ios/Project.swift`, swift(`0.14.15`, `110`), swift(`0.14.16`, `112`))).toBe(true)
      const cargo = (v: string) => `[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nedition = "2021"\nversion = "${v}"\n\n[workspace.dependencies]\ngpui = { git = "https://github.com/zed-industries/zed", rev = "abc" }\n`
      expect(versionOnlyChange(`apps/desktop/Cargo.toml`, cargo(`0.14.23`), cargo(`0.14.24`))).toBe(true)
    })

    test(`a line-count change, an unchanged file and an unknown path are never version-only`, () => {
      const swift = `let appMarketingVersion = "0.14.15"\nlet appBuildVersion = "110"\n`
      expect(versionOnlyChange(`apps/ios/Project.swift`, swift, `${swift}let extra = 1\n`)).toBe(false)
      expect(versionOnlyChange(`apps/ios/Project.swift`, swift, swift)).toBe(false)
      expect(versionOnlyChange(`apps/desktop/Cargo.lock`, `version = "1"`, `version = "2"`)).toBe(false)
    })
  })

  describe(`Rust inline tests (EXP-654)`, () => {
    const PROD = [`use gpui::App;`, ``, `pub fn render() -> u32 {`, `    1`, `}`, ``].join(`\n`)
    const TESTS = (body: string) =>
      [`#[cfg(test)]`, `mod tests {`, `    use super::*;`, ``, `    #[test]`, `    fn renders() {`, `        ${body}`, `    }`, `}`, ``].join(`\n`)
    const FILE = (body = `assert_eq!(render(), 1);`) => `${PROD}${TESTS(body)}`

    test(`the region starts at the last cfg(test) that opens the trailing module`, () => {
      // 5 production lines, so the marker is line 6.
      expect(inlineTestRegion(FILE())).toBe(6)
      // A `#[cfg(test)] use` at the top does not move it.
      expect(inlineTestRegion(`#[cfg(test)]\nuse std::fmt;\n${FILE()}`)).toBe(8)
      // Braces inside strings and comments do not break the count.
      expect(inlineTestRegion(FILE(`assert_eq!(format!("{{"), "{"); // }`))).toBe(6)
    })

    test(`no marker, an external tests module, or code after the module is ambiguous`, () => {
      expect(inlineTestRegion(PROD)).toBeUndefined()
      expect(inlineTestRegion(`${PROD}#[cfg(test)]\nmod tests;\n`)).toBeUndefined()
      expect(inlineTestRegion(`${FILE()}\npub fn later() {}\n`)).toBeUndefined()
    })

    test(`hunk headers parse with and without counts`, () => {
      expect(parseUnifiedHunks(`@@ -12 +12 @@\n-a\n+b\n@@ -20,0 +21,3 @@\n+x\n@@ -30,2 +33,0 @@\n-y\n-z\n`)).toEqual([
        { oldStart: 12, oldCount: 1, newStart: 12, newCount: 1 },
        { oldStart: 20, oldCount: 0, newStart: 21, newCount: 3 },
        { oldStart: 30, oldCount: 2, newStart: 33, newCount: 0 },
      ])
    })

    test(`an edit confined to the inline test module is dropped`, () => {
      const before = FILE(`assert_eq!(render(), 1);`)
      const after = FILE(`assert_eq!(render(), 2);`)
      // Line 12 is the assertion on both sides.
      expect(rustInlineTestOnly(before, after, [{ oldStart: 12, oldCount: 1, newStart: 12, newCount: 1 }])).toBe(true)
    })

    test(`a mixed diff — production and test lines — is kept`, () => {
      const hunks = [
        { oldStart: 4, oldCount: 1, newStart: 4, newCount: 1 },
        { oldStart: 12, oldCount: 1, newStart: 12, newCount: 1 },
      ]
      expect(rustInlineTestOnly(FILE(), FILE(`assert!(true);`), hunks)).toBe(false)
    })

    test(`a file with no marker is kept; a newly added trailing module is test-only`, () => {
      expect(rustInlineTestOnly(PROD, PROD, [{ oldStart: 4, oldCount: 1, newStart: 4, newCount: 1 }])).toBe(false)
      expect(rustInlineTestOnly(PROD, FILE(), [{ oldStart: 5, oldCount: 0, newStart: 6, newCount: 9 }])).toBe(true)
      expect(rustInlineTestOnly(PROD, PROD, [])).toBe(false)
    })

    test(`a deletion-only hunk inside the old file's test module is dropped`, () => {
      const before = FILE()
      const after = `${PROD}${[`#[cfg(test)]`, `mod tests {`, `    use super::*;`, `}`, ``].join(`\n`)}`
      // Lines 9-13 of the old file (the blank, the attribute and the fn) go.
      expect(rustInlineTestOnly(before, after, [{ oldStart: 9, oldCount: 5, newStart: 8, newCount: 0 }])).toBe(true)
    })

    test(`an insertion just above the marker is production code`, () => {
      const after = `${PROD}pub fn later() {}\n${TESTS(`assert_eq!(render(), 1);`)}`
      expect(rustInlineTestOnly(FILE(), after, [{ oldStart: 5, oldCount: 0, newStart: 6, newCount: 1 }])).toBe(false)
    })
  })
})

describe(`--since auto baseline (EXP-667)`, () => {
  const capture = (ref: string) => ({ ref, subject: CAPTURE_COMMIT_SUBJECT })

  test(`is the newest capture when nothing newer touched the store`, () => {
    const picked = pickStoreBaseline([capture(`aaa`), capture(`bbb`)])
    expect(picked?.ref).toBe(`aaa`)
    expect(picked?.skipped).toBeUndefined()
  })

  test(`steps OVER a feature PR that touched shots/ without capturing it`, () => {
    // The exact shape that blinded the automation: EXP-654 dropped the ipad
    // webps and landed `recent-runs/*`, so it touched shots/ — but it did not
    // re-photograph the store, and taking it as the baseline reported every
    // lane empty while the iOS create-sheet shot was stale.
    const picked = pickStoreBaseline([
      { ref: `6a6d3fe2`, subject: `EXP-654: a Rust inline mod tests change (#539)` },
      { ref: `9c10f3da`, subject: `readme` },
      capture(`9b352018`),
    ])
    expect(picked?.ref).toBe(`9b352018`)
    expect(picked?.skipped?.ref).toBe(`6a6d3fe2`)
  })

  test(`the skip is reported, so an empty scope is never silently trusted`, () => {
    const picked = pickStoreBaseline([
      { ref: `6a6d3fe2`, subject: `EXP-654: something else entirely` },
      capture(`9b352018`),
    ])
    expect(baselineSkipNote(picked!.skipped!)).toContain(`6a6d3fe2`)
    expect(baselineSkipNote(picked!.skipped!)).toContain(`EXP-654`)
  })

  test(`falls back to the last touch when the store has never been refreshed`, () => {
    const picked = pickStoreBaseline([{ ref: `aaa`, subject: `EXP-566: seed the store` }])
    expect(picked?.ref).toBe(`aaa`)
    expect(picked?.skipped).toBeUndefined()
  })

  test(`is undefined when nothing has ever been committed under shots/`, () => {
    expect(pickStoreBaseline([])).toBeUndefined()
  })
})

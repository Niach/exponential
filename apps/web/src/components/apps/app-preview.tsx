import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ComponentType, type ReactNode } from "react"
import type { LucideIcon } from "lucide-react"
import { useRouter } from "@tanstack/react-router"
import { A2UI_VERSION, DEFAULT_THEME_ID, navScreens, resolveScreen, screenData, type AppPackage, type ResolvedScreen } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-react"
import { swipeBlocked } from "@exponential_app/eui/web"
import { APPS_COPY, APPS_PREVIEW, APPS_SCREENS, APPS_TEST_IDS, previewTemplate, screenShell, surfaceState } from "@exp/domain-contract/apps"
import { appReactExtension, conceptIcon, EmptyState, PhoneTab, PhoneTabBar, SidebarMenu, SidebarMenuButton, SidebarMenuItem } from "@exp/ui"
import { AppSurfaceBoundary } from "@/components/apps/app-surface-boundary"
import { ConsentCard, HostBanner, createAppHost, createConsentGate, type AppHostOptions } from "@/lib/exponential-ui-host"
import { useIssueRefs } from "@/components/issue-ref-provider"
import type { ScreenRequest } from "@/lib/app-screens"
import { FACE_BODY_TOUCH_CLASS, primeTabEnter, useFaceSwipe } from "@/hooks/use-face-swipe"

/** VAPP-198: who the preview runs as. */
export type AppRunAs = NonNullable<AppHostOptions[`runAs`]>
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"

// VAPP-110: the editor's PREVIEW: the package's first template rendered
// through the app host (the same public host API a third party uses:
// `exp:` + `records:` bindings, `harness_mcp` behind the consent card, the URL
// policy). A new package = a new host (the key), so every synced upsert
// repaints; no package yet = the empty state in the frame. VAPP-179: the
// template to apply comes from the editor's toolbar (`previewTemplate`), and
// `frame` = the frame's width (phone = `APPS_PREVIEW.phoneWidth`, centred).
// VAPP-218: a package's `screens` = the host's shell inside the frame
// (apps.json `screens`): the sidebar rows on a wide frame, the phone tab
// bar on a phone or the Phone frame; the shown screen's template opens with
// its params (`screenData`), and `app_navigate` asks the editor for another.
// #VAPP-251: a package the host refuses, or any throw in here, = the frame
// with `copy.needsUpdate` alone (`AppSurfaceBoundary`).

const NavAppsIcon = conceptIcon(`nav-apps`)
const PlaceholderIcon = conceptIcon(`ui-icon-placeholder`)
const EXTENSIONS = [appReactExtension]

/** A screen's glyph: its eUI icon by name, none/unknown = the placeholder. */
function ScreenIcon({ name, className }: { name: string | null; className: string }) {
  const Icon = (name ? (exponentialUiIcons as (n: string) => ComponentType<{ className?: string }> | undefined)(name) : undefined) ?? PlaceholderIcon
  return <Icon className={className} />
}

type Navigate = NonNullable<AppHostOptions[`navigateScreen`]>

function PackageSurface({ pkg, templateId, screen, onNavigate, mode, appId, teamId, me, grantedTools, runAs, readOnly, migrateFrom }: { pkg: AppPackage; templateId: string | null; screen: ResolvedScreen | null; onNavigate: Navigate | null; mode: `light` | `dark`; appId: string | null; teamId: string | null; me: string | null; grantedTools: readonly string[]; runAs: AppRunAs | null; readOnly: boolean; migrateFrom: string | null }) {
  const router = useRouter()
  const consent = useMemo(createConsentGate, [])
  // VAPP-120: the grants follow the synced row without remounting the host.
  const grants = useRef(grantedTools)
  grants.current = grantedTools
  // VAPP-218: the shell's navigate, read at call time (no screens = refused).
  const navigateRef = useRef(onNavigate)
  navigateRef.current = onNavigate
  const hasShell = onNavigate !== null
  const host = useMemo(
    () =>
      createAppHost({
        consent,
        navigate: (href) => router.history.push(href),
        navigateScreen: hasShell ? (target) => navigateRef.current?.(target) ?? false : undefined,
        packages: [pkg],
        appId,
        teamId,
        me,
        grantedTools: () => grants.current,
        runAs,
        access: readOnly ? `read` : `write`,
        migrateFrom,
      }),
    [consent, router, pkg, appId, teamId, me, runAs, readOnly, hasShell, migrateFrom]
  )
  // VAPP-221: a FileUpload's bytes go to the app host (apps.json `appFiles`).
  // #EXP-1674: `textLinks` reads the issues at paint time and nothing else
  // repaints a surface when they sync, so a new plugin repaints it (the team
  // layout's IssueRefProvider already holds the query; no surface state resets).
  const issueCount = useIssueRefs()?.rows.length ?? 0
  const plugin = useMemo(
    () => ({ icons: exponentialUiIcons, onUpload: (files: File[], target: { name: string }) => host.upload(files, target) }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [host, issueCount]
  )
  // Round 9 §1.4: the painted surface is state: a new screen = a minted id on
  // the same host.
  const [applied, setApplied] = useState<{ host: typeof host; surfaceId: string } | null>(null)
  const screenKey = screen ? JSON.stringify(screen.params) : ``
  useEffect(() => {
    const surfaceId = host.freshSurfaceId(APPS_PREVIEW.surfaceId)
    if (templateId) host.applyTemplate({ surfaceId, templateId, packageId: pkg.id, ...(screen ? { data: screenData(screen) } : {}) })
    setApplied({ host, surfaceId })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host, pkg.id, templateId, screenKey])
  useEffect(() => () => host.close(), [host])
  const surfaceId = applied?.host === host ? applied.surfaceId : APPS_PREVIEW.surfaceId
  // The old surface goes once the new one is committed, so it paints until then.
  useEffect(() => {
    if (applied?.host !== host) return
    for (const id of host.surfaceIds()) if (id !== applied.surfaceId) host.receive({ version: A2UI_VERSION, deleteSurface: { surfaceId: id } })
  }, [applied, host])
  // #VAPP-251: applied and no surface = refused, as a throw for the boundary.
  const hasSurface = useSyncExternalStore(useCallback((cb: () => void) => host.subscribe(cb), [host]), () => host.surface(surfaceId) !== undefined)
  if (surfaceState(true, applied?.host === host, hasSurface) === `needsUpdate`) throw new Error(`the template ${templateId} of ${pkg.id} made no surface: ${host.issues.map((i) => i.message).join(`; `)}`)
  return (
    <>
      <HostBanner host={host} />
      <HostSurface
        host={host}
        surfaceId={surfaceId}
        plugin={plugin}
        extensions={EXTENSIONS}
        theme={typeof pkg.theme === `string` ? pkg.theme : DEFAULT_THEME_ID}
        mode={mode}
        readOnly={readOnly}
      />
      <ConsentCard gate={consent} />
    </>
  )
}

/** The frame's classes and the outer centring (the run view has neither
 *  border nor radius; the Phone frame is `phoneWidth` px, centred). */
function previewFrame(variant: `frame` | `run`, frame: `desktop` | `phone`, className: string | undefined, phone = false) {
  const run = variant === `run`
  return {
    frameClass: `relative min-h-0 ${run && phone ? `` : `bg-background`} ${run ? `` : `rounded-[12px] border border-border`} ${frame === `desktop` ? `flex-1 ${className ?? ``}` : `shrink-0`}`,
    frameStyle: {
      ...(run ? {} : { borderRadius: APPS_PREVIEW.radius }),
      ...(frame === `phone` ? { width: APPS_PREVIEW.phoneWidth, maxWidth: `100%` } : {}),
    },
    testId: run ? APPS_TEST_IDS.run : APPS_TEST_IDS.preview,
    outer: (surface: ReactNode) => (frame === `desktop` ? surface : <div className={`flex min-h-0 flex-1 justify-center ${className ?? ``}`}>{surface}</div>),
  }
}

type AppPreviewProps = {
  pkg: Record<string, unknown> | null
  /** VAPP-179: the picked template (null or missing = the first by id). */
  template?: string | null
  /** VAPP-179: the frame's width: the pane's (desktop) or `phoneWidth` centred. */
  frame?: `desktop` | `phone`
  mode?: `light` | `dark`
  className?: string
  /** VAPP-122: the app whose records `records:<kind>` reads, and the viewer. */
  appId?: string | null
  teamId?: string | null
  me?: string | null
  /** VAPP-120: the typed actions the app may run without asking. */
  grantedTools?: readonly string[]
  /** VAPP-198: preview as another member (owners; null = me). */
  runAs?: AppRunAs | null
  /** eApps R8 (apps.json `migrationCases`): previewing a draft with
   *  `migrations`: the published package's `version` its records are read
   *  through (migrated copies, nothing written); null = as stored. */
  migrateFrom?: string | null
  /** VAPP-200: the viewer's app access is `read`: the app's own writing
   *  controls paint disabled (eUI `readOnly`). */
  readOnly?: boolean
  /** VAPP-218: a phone (web < md): the screens' shell is the bar. */
  phone?: boolean
  /** VAPP-218: the requested screen (null = the first). */
  screen?: ScreenRequest | null
  /** VAPP-218: a row/tab tap or `app_navigate` asks for a screen. */
  onScreen?: (request: ScreenRequest) => void
  /** VAPP-242 (apps.json THE RUN VIEW): `run` = the same surface edge to
   *  edge (no frame border or radius), the run view's full screen. */
  variant?: `frame` | `run`
  /** #VAPP-251: the app's glyph (`copy.needsUpdate`). */
  glyph?: LucideIcon
}

export function AppPreview(props: AppPreviewProps) {
  const { pkg, template = null, screen = null, frame = `desktop`, variant = `frame`, phone = false, className, glyph = NavAppsIcon } = props
  const resetKey = useMemo(() => JSON.stringify([pkg, template, screen]), [pkg, template, screen])
  const { frameClass, frameStyle, testId, outer } = previewFrame(variant, frame, className, phone)
  return (
    <AppSurfaceBoundary
      resetKey={resetKey}
      fallback={outer(
        <div className={`${frameClass} flex items-center justify-center`} style={frameStyle} data-testid={testId}>
          <EmptyState icon={glyph} title={APPS_COPY.needsUpdate} size="page" />
        </div>
      )}
    >
      <AppPreviewBody {...props} />
    </AppSurfaceBoundary>
  )
}

function AppPreviewBody({
  pkg,
  template = null,
  frame = `desktop`,
  mode = `dark`,
  className,
  appId = null,
  teamId = null,
  me = null,
  grantedTools = [],
  runAs = null,
  readOnly = false,
  migrateFrom = null,
  phone = false,
  screen = null,
  onScreen,
  variant = `frame`,
}: AppPreviewProps) {
  // VAPP-218: with screens the shown one picks the template (the Select is hidden).
  // #EXP-1674: the shell paints the navigation's; `nav: false` opens by link.
  const screens = useMemo(() => (pkg ? navScreens(pkg) : []), [pkg])
  const resolved = pkg && screens.length ? resolveScreen(pkg, screen?.screen ?? null, screen?.params ?? null) : null
  const shell = screenShell(screens.length, phone || frame === `phone`)
  const templateId = resolved ? resolved.screen : previewTemplate(pkg as { templates?: Record<string, unknown> } | null, template)
  // The key = the package's identity: a new one mounts a fresh host; a new template or screen re-opens the surface on it.
  const key = useMemo(() => (pkg ? JSON.stringify(pkg) : ``), [pkg])
  const onScreenRef = useRef(onScreen)
  onScreenRef.current = onScreen
  const onNavigate = useMemo<Navigate | null>(
    () =>
      shell
        ? (target) => {
            onScreenRef.current?.({ screen: target.screen, params: target.params })
            return true
          }
        : null,
    [shell]
  )
  const content = pkg ? (
    <PackageSurface key={`${runAs?.id ?? ``}:${key}`} pkg={pkg as unknown as AppPackage} templateId={templateId} screen={resolved} onNavigate={onNavigate} mode={mode} appId={appId} teamId={teamId} me={me} grantedTools={grantedTools} runAs={runAs} readOnly={readOnly} migrateFrom={migrateFrom} />
  ) : (
    <div className="flex h-full min-h-48 items-center justify-center">
      <EmptyState icon={NavAppsIcon} title={APPS_COPY.previewEmpty} size="page" />
    </div>
  )
  const open = (id: string) => onScreen?.({ screen: id, params: {} })
  // #FEED-201: the bar's screens page like every phone tab strip (THE pager,
  // `useFaceSwipe`); a touch inside eUI content that pans sideways keeps it.
  const ids = useMemo(() => screens.map((s) => s.template), [screens])
  const shown = resolved?.screen ?? null
  const swipe = useFaceSwipe(ids, shown, (id) => id && open(id), swipeBlocked)
  const tap = (id: string) => {
    if (shown) primeTabEnter(ids, shown, id)
    open(id)
  }
  const { frameClass, frameStyle, testId, outer } = previewFrame(variant, frame, className, phone)
  const surface = !shell || !resolved ? (
    <div className={`${frameClass} overflow-y-auto`} style={{ ...frameStyle, padding: APPS_PREVIEW.padding }} data-testid={testId}>
      {content}
    </div>
  ) : shell === `sidebar` ? (
    // The sidebar rows at the frame's start, a hairline, then the surface.
    <div className={`${frameClass} flex overflow-hidden`} style={frameStyle} data-testid={testId}>
      <nav className="shrink-0 overflow-y-auto p-2" style={{ width: APPS_SCREENS.sidebarWidth }} data-testid={APPS_TEST_IDS.previewScreens}>
        <SidebarMenu>
          {screens.map((s) => (
            <SidebarMenuItem key={s.template}>
              <SidebarMenuButton density="compact" isActive={s.template === resolved.screen} onClick={() => open(s.template)}>
                <ScreenIcon name={s.icon ?? null} className="h-4 w-4" />
                <span>{s.title}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </nav>
      <div className="relative min-h-0 min-w-0 flex-1 overflow-y-auto border-l border-border" style={{ padding: APPS_PREVIEW.padding }}>
        {content}
      </div>
    </div>
  ) : (
    // The phone tab bar under the surface, inside the frame.
    <div
      className={`${frameClass} flex flex-col overflow-hidden`}
      style={{ ...frameStyle, ...swipe.style }}
      data-testid={testId}
      ref={swipe.ref}
      onTouchStart={swipe.onTouchStart}
      onTouchMove={swipe.onTouchMove}
      onTouchEnd={swipe.onTouchEnd}
      onTouchCancel={swipe.onTouchCancel}
    >
      <div className={`relative min-h-0 flex-1 overflow-y-auto ${FACE_BODY_TOUCH_CLASS}`} style={{ padding: APPS_PREVIEW.padding }} data-face-body="">
        {content}
      </div>
      <div className="flex shrink-0 p-2" data-testid={APPS_TEST_IDS.previewScreens}>
        <PhoneTabBar className="flex-1" aria-label="Screens">
          {screens.map((s) => (
            <PhoneTab key={s.template} label={s.title} icon={<ScreenIcon name={s.icon ?? null} className="size-5" />} selected={s.template === resolved.screen} onClick={() => tap(s.template)} />
          ))}
        </PhoneTabBar>
      </div>
    </div>
  )
  return outer(surface)
}

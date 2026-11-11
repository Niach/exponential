import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type ComponentType } from "react"
import { A2UI_VERSION, DEFAULT_THEME_ID, navScreens, resolveScreen, screenData, type AppPackage } from "@exponential_app/eui"
import { HostSurface, type IconMap } from "@exponential_app/eui-react"
import { APPS_COPY, APPS_PREVIEW, APPS_SCREENS, APPS_TEST_IDS, previewTemplate, screenShell, surfaceState } from "@exp/domain-contract/apps"
import { SEMANTIC_ICONS, type IconConcept, type IconName } from "@exp/icons"
import promptsFixture from "@exp/domain-contract/fixtures/prompts.json"
import {
  CardLinksProvider,
  CodeBlock,
  CodeBlockContent,
  EmptyState,
  ICON_COMPONENTS,
  ListEmpty,
  PhoneTab,
  PhoneTabBar,
  Prompt,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  Skeleton,
  SurfaceCard,
  Toaster,
  appReactExtension,
  conceptIcon,
  getActionIcon,
  toast,
  type CardLinks,
  type PromptActionRole,
} from "@exp/ui"
import { actionConsent } from "@exp/domain-contract/app-actions"
import { AppBridge } from "../bridge"
import { createConsentGate, createMcpAppHost, type CallTool, type ConsentGate } from "./mcp-host"
import { decodeGenuiResult, type GenuiAppRead, type GenuiAppRow, type GenuiScreen } from "./result"

// #VAPP-292 — the genUI view (`ui://exponential/genui`): THE web renderer
// (@exponential_app/eui-react + the @exp/ui primitives) inside an MCP Apps
// host. Three things draw:
//   - an inline `exponential_show` card: the chat's SurfaceCard; a `reply`
//     press is the person's next message (`ui/message`);
//   - `exponential_apps {op: "get"}`: the app RUNS (the run view's surface and
//     screens shell, apps.json), its bindings and writes over tools/call;
//   - `exponential_apps {op: "list"}`: the apps as rows; one opens in place.
// Any other apps op draws nothing (zero height: the host shows the call only).

const NavAppsIcon = conceptIcon(`nav-apps`)
const PlaceholderIcon = conceptIcon(`ui-icon-placeholder`)
const EXTENSIONS = [appReactExtension]

/** The app's icon registry (apps/web lib/exponential-ui-icons.ts). */
export const genuiIcons: IconMap = (name: string) => {
  const concept = SEMANTIC_ICONS[name as IconConcept] as IconName | undefined
  return ICON_COMPONENTS[(concept ?? name) as IconName]
}

function ScreenIcon({ name, className }: { name: string | null; className: string }) {
  const Icon = (name ? (genuiIcons as (n: string) => ComponentType<{ className?: string }> | undefined)(name) : undefined) ?? PlaceholderIcon
  return <Icon className={className} />
}

/** The instance origin the server stamped (`exp-origin`), for links. */
export function instanceOrigin(doc: Document = document): string {
  const stamped = doc.querySelector(`meta[name="exp-origin"]`)?.getAttribute(`content`)
  return stamped && /^https?:\/\//.test(stamped) ? stamped : `https://app.exponential.at`
}

// ── The consent card (fixtures/prompts.json `exponential-ui-consent`) ──────

type PromptEntry = { title: string; body: string; actions: { id: string; label: string; role: string }[]; focus: string }
const PROMPTS = (promptsFixture as unknown as { prompts: Record<string, PromptEntry> }).prompts
const DESTRUCTIVE_WORDS = [`delete`, `merge`, `close`, `end`, `remove`, `trash`]
/** Desktop / iOS / Android: the first 160 characters of the arguments. */
const CONSENT_ARGS_PREVIEW = 160

export function consentCopy(name: string, args: Record<string, unknown>) {
  const typed = actionConsent(name, args)
  const mcp = name === `harness_mcp`
  const tool = typed?.tool ?? (mcp ? String(args.tool ?? ``) : name)
  const raw = typed ? null : mcp ? args.arguments : args
  const op = raw && typeof raw === `object` ? (raw as Record<string, unknown>).op : undefined
  const destructive = typed ? typed.destructive : `${tool} ${typeof op === `string` ? op : ``}`.toLowerCase().split(/[^a-z]+/).some((w) => DESTRUCTIVE_WORDS.includes(w))
  const json = typed ? typed.args : raw && typeof raw === `object` ? JSON.stringify(raw) : null
  const entry = PROMPTS[destructive ? `exponential-ui-consent-destructive` : `exponential-ui-consent`]!
  return {
    title: typed?.title ?? entry.title.replace(`{tool}`, tool),
    body: entry.body,
    args: json && json !== `{}` ? json : null,
    actions: entry.actions.map((a) => ({ id: a.id, label: a.label, role: a.role as PromptActionRole, ...(a.id === entry.focus ? { autoFocus: true } : {}) })),
  }
}

function ConsentCard({ gate }: { gate: ConsentGate }) {
  const call = useSyncExternalStore(gate.subscribe, gate.current, gate.current)
  if (!call) return null
  const copy = consentCopy(call.name, call.args)
  return (
    <Prompt
      open
      onOpenChange={(open) => !open && gate.answer(false)}
      data-testid="exponential-ui-consent"
      className="z-[52]"
      title={copy.title}
      body={copy.body}
      onDismiss={() => gate.answer(false)}
      actions={copy.actions.map(({ id, ...action }) => ({ ...action, onSelect: () => gate.answer(id === `allow`) }))}
    >
      {copy.args === null ? null : (
        <CodeBlock size="sm" data-testid="exponential-ui-consent-arguments">
          <CodeBlockContent language="json">{copy.args.length > CONSENT_ARGS_PREVIEW ? `${copy.args.slice(0, CONSENT_ARGS_PREVIEW)}…` : copy.args}</CodeBlockContent>
        </CodeBlock>
      )}
    </Prompt>
  )
}

// ── A running app ───────────────────────────────────────────────────────────

type ScreenRequest = { screen: string; params: Record<string, string> }

function AppSurface({ app, call, origin, openLink, screen, onScreen }: { app: GenuiAppRead; call: CallTool; origin: string; openLink: (url: string) => void; screen: ReturnType<typeof resolveScreen> | null; onScreen: ((request: ScreenRequest) => void) | null }) {
  const consent = useMemo(createConsentGate, [])
  const onScreenRef = useRef(onScreen)
  onScreenRef.current = onScreen
  const hasShell = onScreen !== null
  const host = useMemo(
    () =>
      createMcpAppHost({
        app,
        call,
        origin,
        consent,
        openLink,
        notify: (text, tone) => (tone === `error` ? toast.error(text) : toast(text)),
        navigateScreen: hasShell
          ? (target) => {
              onScreenRef.current?.({ screen: target.screen, params: target.params })
              return true
            }
          : undefined,
      }),
    [app, call, origin, consent, openLink, hasShell]
  )
  const pkg = app.package as unknown as AppPackage
  const templateId = screen ? screen.screen : previewTemplate(app.package as { templates?: Record<string, unknown> } | null, null)
  const plugin = useMemo(() => ({ icons: genuiIcons }), [])
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
  useEffect(() => {
    if (applied?.host !== host) return
    for (const id of host.surfaceIds()) if (id !== applied.surfaceId) host.receive({ version: A2UI_VERSION, deleteSurface: { surfaceId: id } })
  }, [applied, host])
  const hasSurface = useSyncExternalStore(useCallback((cb: () => void) => host.subscribe(cb), [host]), () => host.surface(surfaceId) !== undefined)
  if (surfaceState(true, applied?.host === host, hasSurface) === `needsUpdate`) {
    return <EmptyState icon={getActionIcon({ icon: null })} title={APPS_COPY.needsUpdate} size="page" />
  }
  return (
    <>
      <HostSurface host={host} surfaceId={surfaceId} plugin={plugin} extensions={EXTENSIONS} theme={typeof pkg.theme === `string` ? pkg.theme : DEFAULT_THEME_ID} mode="dark" readOnly={app.viewer?.access === `read`} />
      <ConsentCard gate={consent} />
    </>
  )
}

/** THE run view (apps.json): the published surface edge to edge, with the
 *  screens shell (sidebar rows on a wide frame, the phone tab bar narrow). */
export function AppRun({ app, call, origin, openLink, phone }: { app: GenuiAppRead; call: CallTool; origin: string; openLink: (url: string) => void; phone: boolean }) {
  const pkg = app.package
  const [request, setRequest] = useState<ScreenRequest | null>(null)
  const screens = useMemo(() => (pkg ? navScreens(pkg as unknown as AppPackage) : []), [pkg])
  const resolved = pkg && screens.length ? resolveScreen(pkg as unknown as AppPackage, request?.screen ?? null, request?.params ?? null) : null
  const shell = screenShell(screens.length, phone)
  if (!pkg) return <EmptyState icon={NavAppsIcon} title={APPS_COPY.runEmpty} size="page" />
  const content = <AppSurface key={app.id} app={app} call={call} origin={origin} openLink={openLink} screen={resolved} onScreen={shell ? setRequest : null} />
  const open = (id: string) => setRequest({ screen: id, params: {} })
  if (!shell || !resolved) {
    return (
      <div className="relative min-h-0" style={{ padding: APPS_PREVIEW.padding }} data-testid={APPS_TEST_IDS.run}>
        {content}
      </div>
    )
  }
  if (shell === `sidebar`) {
    return (
      <div className="relative flex min-h-0" data-testid={APPS_TEST_IDS.run}>
        <nav className="shrink-0 p-2" style={{ width: APPS_SCREENS.sidebarWidth }} data-testid={APPS_TEST_IDS.previewScreens}>
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
        <div className="relative min-h-0 min-w-0 flex-1 border-l border-border" style={{ padding: APPS_PREVIEW.padding }}>
          {content}
        </div>
      </div>
    )
  }
  return (
    <div className="relative flex min-h-0 flex-col" data-testid={APPS_TEST_IDS.run}>
      <div className="relative min-h-0 flex-1" style={{ padding: APPS_PREVIEW.padding }}>
        {content}
      </div>
      <div className="flex shrink-0 p-2" data-testid={APPS_TEST_IDS.previewScreens}>
        <PhoneTabBar className="flex-1" aria-label="Screens">
          {screens.map((s) => (
            <PhoneTab key={s.template} label={s.title} icon={<ScreenIcon name={s.icon ?? null} className="size-5" />} selected={s.template === resolved.screen} onClick={() => open(s.template)} />
          ))}
        </PhoneTabBar>
      </div>
    </div>
  )
}

/** The apps of `op: "list"`: the sidebar's app rows (glyph + name). */
function AppsList({ apps, onOpen }: { apps: GenuiAppRow[]; onOpen: (app: GenuiAppRow) => void }) {
  if (apps.length === 0) return <EmptyState icon={NavAppsIcon} title={APPS_COPY.runEmpty} size="page" />
  return (
    <nav className="p-2" data-testid="genui-apps">
      <SidebarMenu>
        {apps.map((app) => {
          const Icon = getActionIcon({ icon: app.icon ?? null })
          return (
            <SidebarMenuItem key={app.id}>
              <SidebarMenuButton onClick={() => onOpen(app)}>
                <Icon className="h-4 w-4" style={app.color ? { color: app.color } : undefined} />
                <span>{app.name}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          )
        })}
      </SidebarMenu>
    </nav>
  )
}

/** The height an open dialog / drawer needs (its content + a margin above
 *  and below), 0 when none is open. */
export function overlayRoom(doc: Document): number {
  let need = 0
  for (const el of Array.from(doc.querySelectorAll<HTMLElement>(`[role="dialog"], [role="alertdialog"]`))) {
    // A dialog scrolls inside itself at the frame's height: its tallest
    // scroller's content (plus what sits around it) is what it needs.
    let inner = 0
    for (const child of Array.from(el.querySelectorAll<HTMLElement>(`*`))) inner = Math.max(inner, child.scrollHeight - child.clientHeight)
    need = Math.max(need, el.scrollHeight + inner + OVERLAY_MARGIN * 2)
  }
  return need
}
const OVERLAY_MARGIN = 32

/** Phones (web < md): the screens' shell is the bar. */
const PHONE_MAX = 767

export function GenuiApp() {
  const [screen, setScreen] = useState<GenuiScreen | { kind: `waiting` }>({ kind: `waiting` })
  const bridge = useRef<AppBridge | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const origin = useMemo(() => instanceOrigin(), [])
  const [width, setWidth] = useState(() => (typeof window === `undefined` ? 800 : window.innerWidth))

  useEffect(() => {
    const instance = new AppBridge({ onToolResult: (result) => setScreen(decodeGenuiResult(result)) })
    bridge.current = instance
    instance.connect().catch(() => {
      // No host answered: the view stays on its waiting state.
    })
    return () => {
      instance.dispose()
      bridge.current = null
    }
  }, [])

  useLayoutEffect(() => {
    document.documentElement.classList.add(`dark`)
  }, [])

  // The host sizes the frame to the content. A fixed dialog or drawer (an
  // app's entry form, the consent card) is out of the flow, so while one is
  // open the root reserves its height (`overlayRoom`) and the frame grows.
  const [overlay, setOverlay] = useState(0)
  useEffect(() => {
    const node = root.current
    if (!node || typeof ResizeObserver === `undefined`) return
    const report = () => bridge.current?.sizeChanged(Math.ceil(node.getBoundingClientRect().height))
    const observer = new ResizeObserver(() => {
      report()
      setWidth(window.innerWidth)
    })
    observer.observe(node)
    // Ratcheted while a dialog stays open: once the frame grew, the dialog's
    // inner scroll is gone and a fresh measure would shrink it back (a loop).
    const watch = new MutationObserver(() => {
      const need = overlayRoom(document)
      setOverlay((had) => (need === 0 ? 0 : Math.max(had, need)))
    })
    watch.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: [`data-state`, `open`] })
    return () => {
      observer.disconnect()
      watch.disconnect()
    }
  }, [])

  const call = useCallback<CallTool>((name, args) => (bridge.current ? bridge.current.callTool(name, args) : Promise.reject(new Error(`No MCP Apps host.`))), [])
  const openLink = useCallback((url: string) => bridge.current?.openLink(url), [])
  const links = useMemo<CardLinks>(() => ({ openUrl: (url) => openLink(new URL(url, origin).href), textLinks: () => null }), [openLink, origin])

  const openApp = async (row: GenuiAppRow) => {
    try {
      setScreen(decodeGenuiResult(await call(`exponential_apps`, { op: `get`, slug: row.slug, teamId: row.teamId })))
    } catch (error) {
      setScreen({ kind: `error`, message: error instanceof Error ? error.message : String(error) })
    }
  }

  return (
    <CardLinksProvider value={links}>
      <div ref={root} className="font-sans text-foreground antialiased" style={overlay ? { minHeight: overlay } : undefined} data-testid="genui-view" data-screen={screen.kind}>
        {screen.kind === `waiting` && (
          <div className="flex flex-col gap-2 p-4">
            <Skeleton className="h-6 w-1/3" />
            <Skeleton className="h-10 w-full" />
          </div>
        )}
        {screen.kind === `error` && <ListEmpty>{screen.message}</ListEmpty>}
        {screen.kind === `card` && <div className="p-3"><SurfaceCard surface={screen.card.surface as never} title={screen.card.title ?? null} testKey="inline" onReply={(text) => void bridge.current?.sendMessage(text).catch(() => undefined)} /></div>}
        {screen.kind === `apps` && <AppsList apps={screen.apps} onOpen={(row) => void openApp(row)} />}
        {screen.kind === `app` && <AppRun key={`${screen.app.id}:${screen.app.version ?? 0}`} app={screen.app} call={call} origin={origin} openLink={openLink} phone={width <= PHONE_MAX} />}
        <Toaster />
      </div>
    </CardLinksProvider>
  )
}

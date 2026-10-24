import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  APPS_COPY,
  APPS_DOCK_CARD,
  APPS_LAYOUT,
  APPS_PANES,
  APPS_TEST_IDS,
  chatCard,
  editorLayout,
  versionLabel,
  type AppEditorLayout,
} from "@exp/domain-contract/apps"
import { APP_EXPORT_COPY } from "@exp/domain-contract/app-export"
import {
  BoardGlyph,
  Button,
  conceptIcon,
  Dock,
  DockSheetBody,
  EmptyState,
  JumpToBottomButton,
  ListRow,
  Pill,
  SegmentedControl,
  SETTINGS_LIST_CLASS,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  toast,
  useIsMobile,
  UserAvatar,
  useDockSheetMeta,
  userMessageCaption,
} from "@exp/ui"
import type { SyncedAppVersion, User } from "@/db/schema"
import { AppPreview } from "@/components/apps/app-preview"
import { SessionComposer } from "@/components/session-composer"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import { AgentChannel, AgentComposer } from "@/components/thread/agent-channel"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useAppVersions, useEditorSession, type AppListEntry } from "@/hooks/use-apps"
import { useTreeMessages } from "@/hooks/use-session-tree"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { resolveLaunchDeviceId } from "@/lib/launch-device"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

// VAPP-110 (fixtures/apps.json): THE EDITOR. Header = glyph + name + the
// version pill + Versions (+ count) + …; body on md+ = two panes, the chat
// (the member's editor session: AgentChannel rows + the composer) beside
// the live preview; on phones a Chat | Preview | Versions segmented control
// and one pane. Leaving the editor ENDS the member's live editor session
// (the tree); the next send revives it. Versions preview read-only with
// a Restore banner; Restore = a new head.
//
// VAPP-129 (md+, apps.json `layout`): FLOATING = the preview fills the body
// and the chat lives in THE Dock (N-12) over its bottom edge: ONE chat
// DockCard (`chatCard`) over the composer; the card opens the chat as the
// Dock sheet; its action (and the sheet's header button) docks it to the
// side = DOCKED, today's two panes, whose Minimize floats it back. The
// choice is kept per browser.

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiHistoryIcon = conceptIcon(`ui-history`)
const UiShareIcon = conceptIcon(`ui-share`)
const DockSideIcon = conceptIcon(APPS_DOCK_CARD.dockIcon as Parameters<typeof conceptIcon>[0])
const FloatIcon = conceptIcon(APPS_DOCK_CARD.floatIcon as Parameters<typeof conceptIcon>[0])

/** The stored layout (apps.json `layout.storageKey`), per browser. */
function useEditorLayout(): [AppEditorLayout, (next: AppEditorLayout) => void] {
  const [layout, setLayout] = useState<AppEditorLayout>(() =>
    editorLayout(typeof window === `undefined` ? null : window.localStorage.getItem(APPS_LAYOUT.storageKey))
  )
  const set = useCallback((next: AppEditorLayout) => {
    setLayout(next)
    try {
      window.localStorage.setItem(APPS_LAYOUT.storageKey, next)
    } catch {
      // Private mode: the choice lasts this visit.
    }
  }, [])
  return [layout, set]
}

/** The "Move to the side" button: the chat card's action and the sheet's
 *  header meta. */
function DockToSideButton({ onClick }: { onClick: () => void }) {
  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-7 gap-1.5 px-2 text-xs"
      onClick={(event) => {
        event.stopPropagation()
        onClick()
      }}
      data-testid={APPS_TEST_IDS.dockToSide}
    >
      <DockSideIcon className="size-3.5" />
      {APPS_COPY.dockToSide}
    </Button>
  )
}

/** The chat card's sheet: the channel rows as the ONE scrolling body; its
 *  header's meta slot carries "Move to the side". */
function ChatSheet({ onDock, children }: { onDock: () => void; children: ReactNode }) {
  const meta = useMemo(() => <DockToSideButton onClick={onDock} />, [onDock])
  useDockSheetMeta(meta)
  return <DockSheetBody>{children}</DockSheetBody>
}

type Tab = `chat` | `preview` | `versions`

/** `sessions.end` on the editor root, surviving navigation and tab close
 *  (keepalive). */
function endEditorSession(sessionId: string) {
  void fetch(`/api/trpc/sessions.end`, {
    method: `POST`,
    credentials: `include`,
    keepalive: true,
    headers: { "content-type": `application/json` },
    body: JSON.stringify({ sessionId }),
  }).catch(() => undefined)
}

function VersionRows({
  versions,
  head,
  viewing,
  users,
  onPick,
}: {
  versions: readonly SyncedAppVersion[]
  head: number
  viewing: number | null
  users: readonly User[]
  onPick: (number: number) => void
}) {
  const usersById = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  if (versions.length === 0) {
    return <EmptyState icon={UiHistoryIcon} title={APPS_COPY.noVersion} size="list" />
  }
  return (
    <div className={SETTINGS_LIST_CLASS} data-testid={APPS_TEST_IDS.versions}>
      {versions.map((version) => {
        const author = version.createdBy ? usersById.get(version.createdBy) : undefined
        const active = viewing === null ? version.number === head : viewing === version.number
        return (
          <ListRow
            key={version.id}
            interactive
            onClick={() => onPick(version.number)}
            className={cn(`gap-3 px-3 py-2 text-sm`, active && `bg-accent/40`)}
            data-testid={`${APPS_TEST_IDS.versionRow}-${version.number}`}
          >
            <span className="w-10 shrink-0 font-medium tabular-nums">{versionLabel(version.number)}</span>
            <span className="min-w-0 flex-1 truncate text-muted-foreground">{version.note?.trim() || APPS_COPY.noNote}</span>
            {version.number === head && <Pill size="sm">{APPS_COPY.head}</Pill>}
            <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
              {author ? <UserAvatar user={author} size={16} /> : null}
              <span className="max-w-24 truncate">{author?.name ?? APPS_COPY.byAgent}</span>
              <span>·</span>
              <span>{userMessageCaption({ at: new Date(version.createdAt) })}</span>
            </span>
          </ListRow>
        )
      })}
    </div>
  )
}

export function AppEditor({
  app,
  teamId,
  teamSlug,
  me,
  isMember,
}: {
  app: AppListEntry
  teamId: string
  teamSlug: string
  me: string | null
  isMember: boolean
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const [tab, setTab] = useState<Tab>(`chat`)
  const [versionsOpen, setVersionsOpen] = useState(false)
  const [viewing, setViewing] = useState<number | null>(null)
  const [viewingPackage, setViewingPackage] = useState<Record<string, unknown> | null>(null)
  const [restoring, setRestoring] = useState(false)
  const [layout, setLayout] = useEditorLayout()
  const [chatOpen, setChatOpen] = useState(false)
  const [bandHeight, setBandHeight] = useState(0)

  const { users } = useTeamUsers(teamId)
  const { root, teamSessions } = useEditorSession(teamId, app.id, me)
  const messages = useTreeMessages(root?.id)
  const follow = useFollowScroll([messages, root?.id])
  const versions = useAppVersions(app.id)
  const head = app.headVersion
  const headPackage = (app.row?.package as Record<string, unknown> | null) ?? null

  // The device the FIRST send runs on: the member's default, else the first
  // online one of theirs.
  const devices = useSteerDevices({ enabled: isMember, currentUserId: me ?? undefined, teamId })
  const deviceId = useMemo(
    () => resolveLaunchDeviceId((devices.devices ?? []).filter((d) => d.online !== false)),
    [devices.devices]
  )

  // Leaving the editor ends the live editor session (its whole tree).
  const rootRef = useRef<SessionRow | null>(null)
  rootRef.current = root
  useEffect(() => {
    const onPageHide = () => {
      const live = rootRef.current
      if (live && live.status !== `ended`) endEditorSession(live.id)
    }
    window.addEventListener(`pagehide`, onPageHide)
    return () => {
      window.removeEventListener(`pagehide`, onPageHide)
      onPageHide()
    }
  }, [app.id])

  // A picked version loads its package on demand; the head clears it.
  useEffect(() => {
    if (viewing === null || viewing === head) {
      setViewingPackage(null)
      return
    }
    let cancelled = false
    void trpc.apps.get
      .query({ id: app.id, version: viewing })
      .then((result) => {
        if (!cancelled) setViewingPackage((result.package as Record<string, unknown> | null) ?? null)
      })
      .catch(() => {
        if (!cancelled) setViewing(null)
      })
    return () => {
      cancelled = true
    }
  }, [viewing, head, app.id])

  const restore = async () => {
    if (viewing === null || restoring) return
    setRestoring(true)
    try {
      const result = await trpc.apps.restore.mutate({ id: app.id, number: viewing }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      toast(APPS_COPY.restored.replace(`{n}`, String(result.restored)).replace(`{m}`, String(result.version)))
      setViewing(null)
    } catch (error) {
      failedAction(`restore`, `the version`, error)
    } finally {
      setRestoring(false)
    }
  }

  const openSession = (session: SessionRow) =>
    void (session.parentId === null
      ? navigate({ to: `/t/$teamSlug/threads/$threadId`, params: { teamSlug, threadId: session.id } })
      : navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: session.id } }))

  const glyph = <BoardGlyph board={{ icon: app.icon, color: app.color }} className="size-5" />
  const versionPill = (
    <Pill size="sm" className={head > 0 ? `tabular-nums` : `text-muted-foreground`}>
      {versionLabel(head)}
    </Pill>
  )
  // VAPP-121: Export beside Versions (desktop) / the header's trailing button (phones).
  const [exportOpen, setExportOpen] = useState(false)
  const exportButton =
    app.headVersion > 0 ? (
      <Button variant="outline" size={isMobile ? `icon-sm` : `sm`} onClick={() => setExportOpen(true)} aria-label={APP_EXPORT_COPY.export} data-testid="app-editor-export">
        <UiShareIcon className="size-4" />
        {!isMobile && APP_EXPORT_COPY.export}
      </Button>
    ) : null
  const exportDialog = <AppExportDialog app={exportOpen ? app : null} onOpenChange={setExportOpen} />

  const versionsButton = (
    <Button variant="outline" size="sm" onClick={() => setVersionsOpen(true)} data-testid="app-editor-versions-button">
      <UiHistoryIcon className="size-4" />
      {APPS_COPY.versions}
      {versions.length > 0 && <Pill size="sm" className="tabular-nums">{versions.length}</Pill>}
    </Button>
  )

  const pickVersion = (number: number) => {
    setViewing(number === head ? null : number)
    setVersionsOpen(false)
    if (isMobile) setTab(`preview`)
  }

  // VAPP-129: md+: the chat floats in THE Dock.
  const floating = !isMobile && layout === `floating`

  const channel =
    root && messages.length > 0 ? (
      <AgentChannel
        agent={{ name: app.name, icon: app.icon, color: app.color }}
        conversation={root}
        messages={messages}
        teamSessions={teamSessions}
        users={users as User[]}
        canAnswer={isMember}
        onOpenSession={openSession}
      />
    ) : (
      <div className="flex flex-1 items-center justify-center py-12">
        <EmptyState icon={NavAppsIcon} title={APPS_COPY.chatEmpty} size="list" />
      </div>
    )

  const composer = isMember ? (
    root ? (
      <SessionComposer
        teamId={teamId}
        sessionId={root.id}
        placeholder={APPS_COPY.composerPlaceholder}
        users={users as User[]}
        busy={root.busy}
        canStop
        testId="app-composer"
        className={floating ? undefined : `mx-auto max-w-3xl`}
      />
    ) : (
      <AgentComposer
        teamId={teamId}
        appId={app.id}
        deviceId={deviceId ?? undefined}
        disabled={!deviceId}
        placeholder={deviceId ? APPS_COPY.composerPlaceholder : APPS_COPY.noDevice}
        users={users as User[]}
        testId="app-composer"
        className={floating ? undefined : `mx-auto max-w-3xl`}
      />
    )
  ) : null

  const chat = (
    <div className="flex h-full min-h-0 flex-col" data-testid={APPS_TEST_IDS.chat}>
      <div className="relative min-h-0 flex-1">
        <div ref={follow.scrollRef} onScroll={follow.onScroll} className="h-full overflow-y-auto overscroll-contain">
          <div ref={follow.contentRef} className="mx-auto flex min-h-full w-full max-w-3xl flex-col justify-end px-4 pt-6 pb-6">
            {channel}
          </div>
        </div>
        <JumpToBottomButton
          visible={!follow.atBottom && messages.length > 0}
          className="absolute bottom-3 left-1/2 z-10 -translate-x-1/2"
          onClick={follow.jumpToBottom}
        />
      </div>
      {composer && (
        <div className="shrink-0 px-4 pt-2 pb-4" data-testid={APPS_TEST_IDS.composer}>
          {composer}
        </div>
      )}
    </div>
  )

  // VAPP-129: the floating chat's ONE DockCard.
  const chatRow = useMemo(
    () => chatCard(messages.map((message) => message.body ?? ``), root?.busy === true),
    [messages, root?.busy]
  )
  const dockToSide = useCallback(() => {
    setChatOpen(false)
    setLayout(`docked`)
  }, [setLayout])

  const preview = (
    <div className="flex h-full min-h-0 flex-col gap-3 p-4" style={floating ? { paddingBottom: bandHeight + 16 } : undefined}>
      {viewing !== null && (
        <div
          className="flex items-center gap-3 rounded-md border border-border px-3 py-2 text-sm"
          data-testid={APPS_TEST_IDS.viewing}
        >
          <span className="flex-1 font-medium">{APPS_COPY.viewing.replace(`{n}`, String(viewing))}</span>
          <Button size="sm" onClick={() => void restore()} pending={restoring}>
            {APPS_COPY.restore}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setViewing(null)}>
            {APPS_COPY.head}
          </Button>
        </div>
      )}
      <AppPreview pkg={viewing !== null ? viewingPackage : headPackage} appId={app.id} teamId={teamId} me={me} grantedTools={app.row?.grantedTools ?? []} />
    </div>
  )

  const versionsList = (
    <VersionRows versions={versions} head={head} viewing={viewing} users={users as User[]} onPick={pickVersion} />
  )

  if (isMobile) {
    return (
      <div className="flex h-dvh min-h-0 flex-col" data-testid={APPS_TEST_IDS.editor}>
        <MobileDetailHeader
          title={
            <span className="flex min-w-0 items-center gap-2">
              {glyph}
              <span className="truncate">{app.name}</span>
              {versionPill}
            </span>
          }
          onBack={() => void navigate({ to: `/t/$teamSlug/apps`, params: { teamSlug } })}
          menu={exportButton ?? undefined}
          below={
            <div className="px-4 pb-2" data-testid={APPS_TEST_IDS.tabs}>
              <SegmentedControl
                value={tab}
                onValueChange={setTab}
                fill
                options={[
                  { value: `chat`, label: APPS_COPY.tabs.chat },
                  { value: `preview`, label: APPS_COPY.tabs.preview },
                  { value: `versions`, label: APPS_COPY.tabs.versions },
                ]}
              />
            </div>
          }
        />
        <div className="min-h-0 flex-1">
          {tab === `chat` && chat}
          {tab === `preview` && preview}
          {tab === `versions` && <div className="h-full overflow-y-auto p-4">{versionsList}</div>}
        </div>
        {exportDialog}
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid={APPS_TEST_IDS.editor}>
      <div className="flex shrink-0 items-center gap-3 border-b border-border px-6 py-4">
        {glyph}
        <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight">{app.name}</h1>
        {versionPill}
        <div className="flex-1" />
        {exportButton}
        {versionsButton}
        {exportDialog}
      </div>
      {floating ? (
        <div className="relative min-h-0 flex-1" data-layout="floating">
          {preview}
          <Dock
            rows={[chatRow]}
            openId={chatOpen ? chatRow.id : null}
            onOpenChange={(id) => setChatOpen(id !== null)}
            renderSheet={() => <ChatSheet onDock={dockToSide}>{channel}</ChatSheet>}
            composer={composer ? <div data-testid={APPS_TEST_IDS.composer}>{composer}</div> : undefined}
            onHeight={setBandHeight}
          />
        </div>
      ) : (
        <div
          className="grid min-h-0 flex-1"
          data-layout="docked"
          style={{ gridTemplateColumns: `minmax(${APPS_PANES.chatMinWidth}px, ${APPS_PANES.chat}fr) ${APPS_PANES.preview}fr` }}
        >
          <div className="flex min-h-0 flex-col border-r border-border">
            <div
              className="flex shrink-0 items-center justify-between border-b border-border px-4 text-sm text-muted-foreground"
              style={{ height: APPS_PANES.headerHeight }}
            >
              <span>{APPS_COPY.tabs.chat}</span>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={APPS_COPY.float}
                title={APPS_COPY.float}
                onClick={() => setLayout(`floating`)}
                data-testid={APPS_TEST_IDS.float}
              >
                <FloatIcon />
              </Button>
            </div>
            <div className="min-h-0 flex-1">{chat}</div>
          </div>
          <div className="min-h-0">{preview}</div>
        </div>
      )}
      <Sheet open={versionsOpen} onOpenChange={setVersionsOpen}>
        <SheetContent side="right" className="w-full sm:max-w-md">
          <SheetHeader>
            <SheetTitle>{APPS_COPY.versions}</SheetTitle>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto p-4">{versionsList}</div>
        </SheetContent>
      </Sheet>
    </div>
  )
}

import { useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  APPS_COPY,
  APPS_EDITOR_TABS,
  APPS_PANES,
  APPS_TEST_IDS,
  editorTab,
  isDraftApp,
  versionLabel,
  type AppEditorSheet,
  type AppEditorTab,
} from "@exp/domain-contract/apps"
import {
  BoardGlyph,
  Button,
  conceptIcon,
  EmptyState,
  JumpToBottomButton,
  ListRow,
  Menu,
  type MenuEntry,
  Pill,
  Prompt,
  SegmentedControl,
  SETTINGS_LIST_CLASS,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  ThreadConversation,
  toast,
  useIsMobile,
  userMessageCaption,
} from "@exp/ui"
import type { SyncedAppVersion, User } from "@/db/schema"
import { AppPreview } from "@/components/apps/app-preview"
import { AppSettingsTab } from "@/components/apps/app-settings-tab"
import { SessionComposer } from "@/components/session-composer"
import { HEADER_BUTTON_CLASS, MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import { relativeTime } from "@/components/comment-rows/format"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useAnswerMessage, useConversationRows } from "@/hooks/use-answer-message"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useAppVersions, useEditorSession, type AppListEntry } from "@/hooks/use-apps"
import { useTreeMessages } from "@/hooks/use-session-tree"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection, sessionCollection } from "@/lib/collections"
import { sendSessionMessage } from "@/lib/enet/send-sealed"
import { resolveLaunchDeviceId } from "@/lib/launch-device"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

// VAPP-110 (fixtures/apps.json): THE EDITOR. Leaving the editor ENDS the
// member's live editor session (the tree); the next send revives it.
//
// VAPP-177 (`editorTabs`): on md+ the header (glyph + name + the version
// pill) ends in the Preview | Versions | Settings segmented control and the
// body = two panes, the chat (the member's editor session: THE thread
// conversation rows + THE session composer, no header strip) beside the
// selected tab. Phones: Chat | Preview under the header, one pane, and the
// header's `…` menu: Versions and Settings open THE SAME bodies as a bottom
// sheet, Export the export sheet, Delete app the delete prompt. A version's
// Preview shows it read-only on the Preview tab with a Restore banner;
// Restore = a new head. VAPP-129's floating layout is gone.

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiHistoryIcon = conceptIcon(`ui-history`)
const UiMoreIcon = conceptIcon(`ui-more`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

/** VAPP-176: `apps.discardDraft` on leaving an EMPTY draft's editor
 *  (keepalive); the server deletes it only while it is still empty. */
function discardDraft(id: string) {
  void fetch(`/api/trpc/apps.discardDraft`, {
    method: `POST`,
    credentials: `include`,
    keepalive: true,
    headers: { "content-type": `application/json` },
    body: JSON.stringify({ id }),
  }).catch(() => undefined)
}

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

/** VAPP-177 (fixtures/apps.json VERSIONS): ONE grouped list, newest first:
 *  the `v{n}` pill + the note over `who · when`; the head row carries
 *  Current, every other row Preview + Restore. The Versions tab's body and
 *  the phone menu's Versions sheet alike. */
function VersionRows({
  versions,
  head,
  viewing,
  users,
  restoring,
  onPreview,
  onRestore,
}: {
  versions: readonly SyncedAppVersion[]
  head: number
  viewing: number | null
  users: readonly User[]
  restoring: number | null
  onPreview: (number: number) => void
  onRestore: (number: number) => void
}) {
  const usersById = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  if (versions.length === 0) {
    return <EmptyState icon={UiHistoryIcon} title={APPS_COPY.versionsEmpty} size="list" />
  }
  return (
    <div className={SETTINGS_LIST_CLASS}>
      {versions.map((version) => {
        const author = version.createdBy ? usersById.get(version.createdBy) : undefined
        const note = version.note?.trim()
        return (
          <ListRow
            key={version.id}
            className={cn(`gap-3 px-3 py-2 text-sm`, viewing === version.number && `bg-accent/40`)}
            data-testid={`${APPS_TEST_IDS.versionRow}-${version.number}`}
          >
            <div className="min-w-0 flex-1">
              <div className="flex min-w-0 items-center gap-2">
                <Pill size="sm" className="shrink-0 tabular-nums">{versionLabel(version.number)}</Pill>
                <span className={cn(`min-w-0 truncate`, note ? `font-medium` : `text-muted-foreground`)}>{note || APPS_COPY.noNote}</span>
              </div>
              <div className="mt-0.5 truncate text-xs text-muted-foreground">
                {author?.name ?? APPS_COPY.byAgent} · {relativeTime(version.createdAt)}
              </div>
            </div>
            {version.number === head ? (
              <Pill size="sm" className="shrink-0">{APPS_COPY.head}</Pill>
            ) : (
              <div className="flex shrink-0 items-center gap-1.5">
                <Button variant="ghost" size="sm" onClick={() => onPreview(version.number)} data-testid={APPS_TEST_IDS.versionPreview}>
                  {APPS_COPY.preview}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => onRestore(version.number)}
                  pending={restoring === version.number}
                  disabled={restoring !== null}
                  data-testid={APPS_TEST_IDS.versionRestore}
                >
                  {APPS_COPY.restore}
                </Button>
              </div>
            )}
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
  canManage,
  canGrant,
  requestedTab,
}: {
  app: AppListEntry
  teamId: string
  teamSlug: string
  me: string | null
  isMember: boolean
  /** The creator or an owner: rename, describe, delete. */
  canManage: boolean
  /** VAPP-120: a team OWNER sets the grants. */
  canGrant: boolean
  /** VAPP-177: the route's `?tab=` (read once: no tab is kept). */
  requestedTab: string | null
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const [initial] = useState(() => editorTab(requestedTab, isMobile))
  const [tab, setTab] = useState<AppEditorTab>(initial.tab)
  const [sheet, setSheet] = useState<AppEditorSheet | null>(initial.sheet)
  const [viewing, setViewing] = useState<number | null>(null)
  const [viewingPackage, setViewingPackage] = useState<Record<string, unknown> | null>(null)
  const [restoring, setRestoring] = useState<number | null>(null)
  const [exportOpen, setExportOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  // Crossing md keeps the pane on screen: a wide Versions/Settings tab
  // becomes the phone's sheet over Chat, and back.
  useEffect(() => {
    if (isMobile) {
      if (tab === `versions` || tab === `settings`) {
        setSheet(tab)
        setTab(`chat`)
      }
    } else if (sheet) {
      setTab(sheet)
      setSheet(null)
    } else if (tab === `chat`) {
      setTab(APPS_EDITOR_TABS.default as AppEditorTab)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isMobile])

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
  // VAPP-176: leaving an EMPTY draft (nothing ever sent) discards it.
  const rootRef = useRef<SessionRow | null>(null)
  rootRef.current = root
  const draftRef = useRef(isDraftApp(app))
  draftRef.current = isDraftApp(app)
  useEffect(() => {
    const onPageHide = () => {
      const live = rootRef.current
      if (live && live.status !== `ended`) endEditorSession(live.id)
      if (!live && draftRef.current) discardDraft(app.id)
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

  const restore = async (number: number | null) => {
    if (number === null || restoring !== null) return
    setRestoring(number)
    try {
      const result = await trpc.apps.restore.mutate({ id: app.id, number }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      toast(APPS_COPY.restored.replace(`{n}`, String(result.restored)).replace(`{m}`, String(result.version)))
      setViewing(null)
    } catch (error) {
      failedAction(`restore`, `the version`, error)
    } finally {
      setRestoring(null)
    }
  }

  // The delete prompt (the Settings tab's button and the phone menu's item):
  // leaves for the Apps list.
  const confirmDelete = async () => {
    setDeleting(true)
    try {
      await trpc.apps.delete.mutate({ id: app.id }, SKIP_ERROR_TOAST)
      setDeleteOpen(false)
      void navigate({ to: `/t/$teamSlug/apps`, params: { teamSlug }, replace: true })
    } catch (error) {
      failedAction(`delete`, `the app`, error)
    } finally {
      setDeleting(false)
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
  const exportDialog = <AppExportDialog app={exportOpen ? app : null} onOpenChange={setExportOpen} />

  // A version's Preview: the Preview tab showing it (phones: the sheet closes).
  const previewVersion = (number: number) => {
    setViewing(number === head ? null : number)
    setSheet(null)
    setTab(`preview`)
  }

  const draft = isDraftApp(app)

  // VAPP-176: a draft's starters: a tap sends that text as the first message.
  const [starting, setStarting] = useState(false)
  const sendStarter = async (text: string) => {
    if (starting || !deviceId) return
    setStarting(true)
    try {
      const result = await sendSessionMessage({ teamId, appId: app.id, deviceId, text })
      await sessionCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch {
      toast.error(`Couldn't send the message`)
    } finally {
      setStarting(false)
    }
  }

  const { rows: channelRows } = useConversationRows({ conversation: root, messages, teamSessions })
  const { answer, pending } = useAnswerMessage()
  const messageAt = (id: string) => {
    const message = messages.find((row) => row.id === id)
    return message ? new Date(message.createdAt) : null
  }
  const channel =
    root && messages.length > 0 ? (
      <ThreadConversation
        rows={channelRows}
        renderText={renderThreadText}
        attachmentSrc={attachmentSrc}
        messageCaption={(row) =>
          userMessageCaption({
            name: row.authorId ? users.find((user) => user.id === row.authorId)?.name : null,
            at: messageAt(row.id),
          })
        }
        onOpenSession={(sessionId) => {
          const target = teamSessions.find((row) => row.id === sessionId)
          if (target) openSession(target)
        }}
        onAnswer={isMember ? (id, input) => void answer(id, input) : undefined}
        onDecide={isMember ? (id, input) => void answer(id, input) : undefined}
        pending={pending}
      />
    ) : draft && !root ? (
      <div className="flex flex-1 items-center justify-center py-12">
        <EmptyState icon={NavAppsIcon} title={APPS_COPY.starterTitle} size="page">
          {isMember &&
            APPS_COPY.starters.map((text) => (
              <Pill
                key={text}
                mode="action"
                size="md"
                onClick={() => void sendStarter(text)}
                disabled={!deviceId || starting}
                data-testid={APPS_TEST_IDS.starter}
              >
                {text}
              </Pill>
            ))}
        </EmptyState>
      </div>
    ) : (
      <div className="flex flex-1 items-center justify-center py-12">
        <EmptyState icon={NavAppsIcon} title={APPS_COPY.chatEmpty} size="list" />
      </div>
    )

  const composer = isMember ? (
    <SessionComposer
      teamId={teamId}
      sessionId={root?.id ?? null}
      appId={app.id}
      deviceId={deviceId ?? undefined}
      disabled={!root && !deviceId}
      placeholder={!root && !deviceId ? APPS_COPY.noDevice : draft ? APPS_COPY.draftPlaceholder : APPS_COPY.composerPlaceholder}
      autoFocus={draft && !root}
      users={users as User[]}
      busy={root?.busy ?? false}
      canStop
      testId="app-composer"
      className="mx-auto max-w-3xl"
    />
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

  const preview = (
    <div className="flex h-full min-h-0 flex-col gap-3 p-4">
      {viewing !== null && (
        <div
          className="flex items-center gap-3 rounded-md border border-border px-3 py-2 text-sm"
          data-testid={APPS_TEST_IDS.viewing}
        >
          <span className="flex-1 font-medium">{APPS_COPY.viewing.replace(`{n}`, String(viewing))}</span>
          <Button size="sm" onClick={() => void restore(viewing)} pending={restoring !== null}>
            {APPS_COPY.restore}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setViewing(null)}>
            {APPS_COPY.head}
          </Button>
        </div>
      )}
      {/* A grant is about the HEAD package: an old version previews asking for everything. */}
      <AppPreview pkg={viewing !== null ? viewingPackage : headPackage} appId={app.id} teamId={teamId} me={me} grantedTools={viewing !== null ? [] : (app.row?.grantedTools ?? [])} />
    </div>
  )

  // VAPP-177: the Versions and Settings bodies, THE SAME on the wide tab and
  // in the phone menu's sheet (the views' anchors live on them).
  const versionsBody = (
    <div data-testid={APPS_TEST_IDS.versions}>
      <VersionRows
        versions={versions}
        head={head}
        viewing={viewing}
        users={users as User[]}
        restoring={restoring}
        onPreview={previewVersion}
        onRestore={(number) => void restore(number)}
      />
    </div>
  )
  const settingsBody = (
    <div className="w-full" style={isMobile ? undefined : { maxWidth: APPS_EDITOR_TABS.settingsMaxWidth }} data-testid={APPS_TEST_IDS.settings}>
      <AppSettingsTab
        teamId={teamId}
        app={app}
        canManage={canManage}
        canGrant={canGrant}
        onExport={() => setExportOpen(true)}
        onDelete={() => setDeleteOpen(true)}
      />
    </div>
  )

  const deletePrompt = (
    <Prompt
      open={deleteOpen}
      onOpenChange={(next) => !deleting && setDeleteOpen(next)}
      busy={deleting}
      title={APPS_COPY.deleteTitle.replace(`{name}`, app.name ?? ``)}
      body={APPS_COPY.deleteBody}
      actions={[
        { label: `Cancel`, role: `cancel` },
        { label: APPS_COPY.delete, role: `destructive`, busy: deleting, onSelect: confirmDelete, testId: `app-delete-confirm` },
      ]}
    />
  )

  const tabOptions = (ids: readonly string[]) =>
    ids.map((id) => ({ value: id as AppEditorTab, label: APPS_COPY.tabs[id as AppEditorTab] }))

  if (isMobile) {
    // The header's `…` (apps.json `editorTabs.menu`).
    const menuEntries: MenuEntry[] = [
      {
        kind: `item`,
        id: `versions`,
        label: APPS_COPY.menu.versions,
        icon: UiHistoryIcon,
        value: versions.length > 0 ? <span className="tabular-nums">{versions.length}</span> : undefined,
        onSelect: () => setSheet(`versions`),
        "data-testid": `${APPS_TEST_IDS.menu}-versions`,
      },
      {
        kind: `item`,
        id: `settings`,
        label: APPS_COPY.menu.settings,
        icon: NavSettingsIcon,
        onSelect: () => setSheet(`settings`),
        "data-testid": `${APPS_TEST_IDS.menu}-settings`,
      },
      ...(head > 0
        ? [
            {
              kind: `item` as const,
              id: `export`,
              label: APPS_COPY.menu.export,
              icon: UiShareIcon,
              onSelect: () => setExportOpen(true),
              "data-testid": `${APPS_TEST_IDS.menu}-export`,
            },
          ]
        : []),
      ...(canManage
        ? [
            { kind: `separator` as const, id: `sep` },
            {
              kind: `item` as const,
              id: `delete`,
              label: APPS_COPY.menu.delete,
              icon: UiDeleteIcon,
              destructive: true,
              onSelect: () => setDeleteOpen(true),
              "data-testid": `${APPS_TEST_IDS.menu}-delete`,
            },
          ]
        : []),
    ]
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
          menu={
            <Menu
              align="end"
              aria-label={`${app.name} actions`}
              title={app.name}
              entries={menuEntries}
              trigger={
                <Button variant="ghost" size="icon-sm" className={HEADER_BUTTON_CLASS} aria-label={`Menu for ${app.name}`} data-testid={APPS_TEST_IDS.menu}>
                  <UiMoreIcon />
                </Button>
              }
            />
          }
          below={
            <div className="px-4 pb-2" data-testid={APPS_TEST_IDS.tabs}>
              <SegmentedControl value={tab} onValueChange={setTab} fill options={tabOptions(APPS_EDITOR_TABS.phone)} />
            </div>
          }
        />
        <div className="min-h-0 flex-1">
          {tab === `chat` && chat}
          {tab === `preview` && preview}
        </div>
        <Sheet open={sheet !== null} onOpenChange={(open) => !open && setSheet(null)}>
          <SheetContent side="bottom" className="gap-0 pb-[max(1rem,env(safe-area-inset-bottom))]">
            <SheetHeader className="px-4 pt-2 pb-1">
              <SheetTitle>{sheet === `settings` ? APPS_COPY.menu.settings : APPS_COPY.menu.versions}</SheetTitle>
            </SheetHeader>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-2 pb-2">
              {sheet === `versions` && versionsBody}
              {sheet === `settings` && settingsBody}
            </div>
          </SheetContent>
        </Sheet>
        {exportDialog}
        {deletePrompt}
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
        <div className="shrink-0" data-testid={APPS_TEST_IDS.tabs}>
          <SegmentedControl value={tab} onValueChange={setTab} options={tabOptions(APPS_EDITOR_TABS.wide)} />
        </div>
      </div>
      <div
        className="grid min-h-0 flex-1"
        style={{ gridTemplateColumns: `minmax(${APPS_PANES.chatMinWidth}px, ${APPS_PANES.chat}fr) ${APPS_PANES.preview}fr` }}
      >
        <div className="flex min-h-0 flex-col border-r border-border">{chat}</div>
        <div className="min-h-0 min-w-0">
          {tab === `preview` && preview}
          {tab === `versions` && <div className="h-full overflow-y-auto p-4">{versionsBody}</div>}
          {tab === `settings` && <div className="h-full overflow-y-auto p-4">{settingsBody}</div>}
        </div>
      </div>
      {exportDialog}
      {deletePrompt}
    </div>
  )
}

const attachmentSrc = (id: string) => `/api/attachments/${id}`

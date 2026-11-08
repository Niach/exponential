import { useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  APPS_COPY,
  APPS_EDITOR_TABS,
  APPS_PANES,
  APPS_PREVIEW,
  APPS_TEST_IDS,
  canPublish,
  editorTab,
  grantVersion,
  isDraftApp,
  listPill,
  previewTemplate,
  previewTemplates,
  runAsOptions,
  shownVersion,
  versionLabel,
  versionRow,
  type AppEditorSheet,
  type AppEditorTab,
  editorDevice,
  editorHeader,
  rollbackBody,
  rollbackTitle,
  rollbackWarns,
  versionMigration,
} from "@exp/domain-contract/apps"
import {
  BoardGlyph,
  Button,
  conceptIcon,
  EmptyState,
  getActionIcon,
  GlassGroup,
  JumpToBottomButton,
  ListRow,
  Menu,
  type MenuEntry,
  PhoneHeaderTitle,
  Pill,
  Prompt,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SETTINGS_LIST_CLASS,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  ThreadConversation,
  toast,
  useIsMobile,
  userMessageCaption,
  type DevicePickerDevice,
} from "@exp/ui"
import type { SyncedAppVersion, User } from "@/db/schema"
import { AppPreview, type AppRunAs } from "@/components/apps/app-preview"
import { AppSettingsTab } from "@/components/apps/app-settings-tab"
import { AppDataTab } from "@/components/apps/app-data-tab"
import { packageScreens, type AppAccessLevel } from "@exponential_app/eui/host"
import type { ScreenRequest } from "@/lib/app-screens"
import { SessionComposer } from "@/components/session-composer"
import { requestDeviceMove } from "@/components/session-device-move"
import { HEADER_BUTTON_CLASS, MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { CHAT_HEADER_TONE_CLASS } from "@/components/thread/chat-header"
import { useEditorRunner } from "@/components/apps/editor-runner"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import { AppImportDialog } from "@/components/apps/app-import-dialog"
import { APP_TEMPLATES_COPY, APP_TEMPLATES_TEST_IDS } from "@exp/domain-contract/app-templates"
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
import { loadEditorDevice, saveEditorDevice } from "@/lib/launch-device"
import { deviceMoveAgent, deviceMoveName, deviceMovePick, deviceMoveTargetReason } from "@/lib/session-device-move"
import { deviceIsOnline } from "@/lib/steer-devices"
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

// VAPP-178: the Data tab (wide + phones); the shared tab type may not name it yet.
type EditorTab = AppEditorTab

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiHistoryIcon = conceptIcon(`ui-history`)
const UiMoreIcon = conceptIcon(`ui-more`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiSubmitIcon = conceptIcon(`ui-submit`)
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
 *  the `v{n}` pill + the note over `who · when`; V1 (`versionRow`): the
 *  published row carries Published, the head row ahead of it Current +
 *  Publish, every other row Preview + Publish. The Versions tab's body and
 *  the phone menu's Versions sheet alike. */
function VersionRows({
  versions,
  pointers,
  viewing,
  users,
  publishing,
  canWrite,
  onPreview,
  onPublish,
  grouped = false,
}: {
  versions: readonly SyncedAppVersion[]
  pointers: { headVersion: number; publishedVersion: number }
  viewing: number | null
  users: readonly User[]
  publishing: number | null
  /** eApps R3: Publish needs write access to the app. */
  canWrite: boolean
  onPreview: (number: number) => void
  onPublish: (number: number) => void
  /** EXP-1554: the phone sheet's face, as the natives draw it: the rows in
   *  ONE grouped card with small pill actions. */
  grouped?: boolean
}) {
  const usersById = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  if (versions.length === 0) {
    return <EmptyState icon={UiHistoryIcon} title={APPS_COPY.versionsEmpty} size="list" />
  }
  const rows = versions.map((version) => {
    const author = version.createdBy ? usersById.get(version.createdBy) : undefined
    const note = version.note?.trim()
    const kind = versionRow(pointers, version.number)
    const migration = versionMigration({ migrationState: version.migrationState ?? null, migrationTotal: version.migrationTotal ?? 0, migrationDone: version.migrationDone ?? 0, migrationError: version.migrationError ?? null })
    return (
      <ListRow
        key={version.id}
        className={cn(`gap-3 px-3 py-2 text-sm`, grouped && `rounded-none`, viewing === version.number && `bg-accent/40`)}
        data-testid={`${APPS_TEST_IDS.versionRow}-${version.number}`}
      >
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <Pill size="sm" className="shrink-0 tabular-nums">{versionLabel(version.number)}</Pill>
            <span className={cn(`min-w-0 truncate`, note ? `font-medium` : `text-muted-foreground`)}>{note || APPS_COPY.noNote}</span>
          </div>
          <div className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">
            {author?.name ?? APPS_COPY.byAgent} · {relativeTime(version.createdAt)}
            {/* eApps R8 (apps.json migrationCases): the record sweep's line. */}
            {migration ? ` · ${migration}` : ``}
          </div>
        </div>
        {kind === `published` ? (
          <Pill size="sm" className="shrink-0">{APPS_COPY.published}</Pill>
        ) : (
          <div className="flex shrink-0 items-center gap-1.5">
            {kind === `head` ? (
              <Pill size="sm" className="shrink-0">{APPS_COPY.head}</Pill>
            ) : (
              <Button variant="ghost" size={grouped ? `xs` : `sm`} onClick={() => onPreview(version.number)} data-testid={APPS_TEST_IDS.versionPreview}>
                {APPS_COPY.preview}
              </Button>
            )}
            {canWrite && (
              <Button
                variant="outline"
                size={grouped ? `xs` : `sm`}
                // EXP-1676: the phone sheet's Publish = the natives' small pill
                // (Pill's chrome: card fill under the card hairline).
                className={grouped ? `rounded-full border-glass-stroke-card bg-glass-card text-foreground/70` : undefined}
                onClick={() => onPublish(version.number)}
                pending={publishing === version.number}
                disabled={publishing !== null}
                data-testid={APPS_TEST_IDS.versionPublish}
              >
                {APPS_COPY.publish}
              </Button>
            )}
          </div>
        )}
      </ListRow>
    )
  })
  return grouped ? <GlassGroup>{rows}</GlassGroup> : <div className={SETTINGS_LIST_CLASS}>{rows}</div>
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
  requestedFrame = null,
  requestedScreen = null,
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
  /** VAPP-179: the route's `?frame=` (read once; screenshots open on Phone). */
  requestedFrame?: PreviewFrame | null
  /** VAPP-218: the preview's first screen (null = the package's first); a
   *  tap or `app.navigate` then moves it in memory (VAPP-242). */
  requestedScreen?: ScreenRequest | null
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const [initial] = useState(() => editorTab(requestedTab, isMobile))
  const [tab, setTab] = useState<EditorTab>(initial.tab)
  const [sheet, setSheet] = useState<AppEditorSheet | null>(initial.sheet)
  const [viewing, setViewing] = useState<number | null>(null)
  // VAPP-179: the preview toolbar's frame + picked template (editor lifetime only).
  const [frame, setFrame] = useState<PreviewFrame>(requestedFrame ?? (APPS_PREVIEW.defaultFrame as PreviewFrame))
  const [pickedTemplate, setPickedTemplate] = useState<string | null>(null)
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
      setTab(APPS_EDITOR_TABS.default as EditorTab)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isMobile])

  // VAPP-218: a screen link while the editor is open shows the Preview tab.
  const requestedScreenName = requestedScreen?.screen ?? null
  useEffect(() => {
    if (requestedScreenName === null) return
    setSheet(null)
    setTab(`preview`)
  }, [requestedScreenName])

  const { users, members } = useTeamUsers(teamId)
  // VAPP-187: the viewer's team role (the Data tab's ownership rule).
  const myRole = members.find((m) => m.userId === me)?.role ?? null
  // eApps R3: the viewer's app access (record-policy.json): the Data tab
  // disables what the server would refuse at `read`.
  const [access, setAccess] = useState<AppAccessLevel>(`write`)
  useEffect(() => {
    if (!app?.id) return
    let live = true
    trpc.appAccess.mine
      .query({ appId: app.id }, SKIP_ERROR_TOAST)
      .then((r) => live && setAccess(r.level))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [app?.id])
  // VAPP-198: Run as <member> (owners; the preview toolbar only).
  const runAsChoices = useMemo(
    () => runAsOptions(members.map((m) => ({ userId: m.userId, role: m.role, name: (users as User[]).find((u) => u.id === m.userId)?.name ?? null })), me ?? null, myRole),
    [members, users, me, myRole]
  )
  const [runAsId, setRunAsId] = useState(``)
  const [levels, setLevels] = useState<Record<string, AppAccessLevel>>({})
  useEffect(() => {
    setRunAsId(``)
    if (!app?.id || myRole !== `owner`) return
    let live = true
    trpc.appAccess.levels
      .query({ appId: app.id }, SKIP_ERROR_TOAST)
      .then((r) => live && setLevels(r.levels))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [app?.id, myRole])
  const runAs = useMemo<AppRunAs | null>(() => {
    const member = runAsId ? members.find((m) => m.userId === runAsId) : undefined
    if (!member) return null
    const name = runAsChoices?.find((o) => o.value === runAsId)?.label ?? runAsId
    return { id: member.userId, name, role: member.role, access: levels[member.userId] ?? `write` }
  }, [runAsId, members, runAsChoices, levels])
  const { root, teamSessions } = useEditorSession(teamId, app.id, me)
  const messages = useTreeMessages(root?.id)
  const follow = useFollowScroll([messages, root?.id])
  const versions = useAppVersions(app.id)
  const head = app.headVersion
  // eApps V1 (#VAPP-210): `package` = the published version's, `draftPackage`
  // = the head's while it is ahead; the preview shows `shownVersion`.
  const publishedPackage = (app.row?.package as Record<string, unknown> | null) ?? null
  const headPackage = (app.row?.draftPackage as Record<string, unknown> | null) ?? publishedPackage
  const editing = Boolean(root && root.status !== `ended`)
  const shown = shownVersion(app, editing, viewing)
  const defaultShown = shownVersion(app, editing, null)
  const canWrite = isMember && access === `write`

  // #FEED-204 (apps.json EDITOR DEVICE): the device the composer shows and
  // sends on: the online devices (with a root: the ones it may move to), the
  // pick remembered per app, a live root's own device (#EXP-1738: a pick moves it).
  const devices = useSteerDevices({ enabled: isMember, currentUserId: me ?? undefined, teamId })
  const candidates = useMemo(() => {
    const all = devices.devices ?? []
    const online = all.filter(deviceIsOnline)
    if (!root) return online
    const agent = deviceMoveAgent(root.cli, all.find((d) => d.deviceId === root.deviceId))
    const session = { teamId: root.teamId, shared: root.shared, agent, userId: root.userId }
    return online.filter((d) => d.deviceId === root.deviceId || deviceMoveTargetReason(d, session, me) === null)
  }, [devices.devices, root, me])
  const [pickedDevice, setPickedDevice] = useState<{ appId: string; deviceId: string } | null>(null)
  const remembered = pickedDevice?.appId === app.id ? pickedDevice.deviceId : loadEditorDevice(app.id)
  const device = editorDevice(root ? { status: root.status, deviceId: root.deviceId } : null, candidates, remembered)
  const deviceId = device.deviceId
  const deviceRows = useMemo<DevicePickerDevice[]>(() => {
    const row = (d: (typeof candidates)[number]) => ({
      id: d.deviceId,
      name: deviceMoveName(d),
      icon: d.icon ?? null,
      kind: d.kind ?? null,
    })
    const rows = candidates.map(row)
    if (!deviceId || rows.some((r) => r.id === deviceId)) return rows
    const current = devices.devices?.find((d) => d.deviceId === deviceId)
    return [current ? row(current) : { id: deviceId, name: deviceId }, ...rows]
  }, [candidates, deviceId, devices.devices])
  // #EXP-1729 (apps.json `editorHeader`): the header's `status · device`,
  // the device = the one the composer shows, named as its Device row names it.
  const shownDevice = deviceId ? devices.devices?.find((d) => d.deviceId === deviceId) : undefined
  const header = editorHeader(
    root ? { status: root.status, busy: root.busy, needsInput: root.needsInput } : null,
    deviceId
      ? {
          name: deviceRows.find((r) => r.id === deviceId)?.name ?? deviceId,
          online: shownDevice ? deviceIsOnline(shownDevice) : false,
        }
      : null
  )
  const subtitleClass = CHAT_HEADER_TONE_CLASS[header.tone]
  // #EXP-1729: Account › and Model › (composer-menu.json, an app editor's
  // composer, always): held picks ride the next send until the root is live.
  const heldRunner = useEditorRunner(root, shownDevice)
  const pickDevice = (next: string) => {
    saveEditorDevice(app.id, next)
    setPickedDevice({ appId: app.id, deviceId: next })
    if (root && root.status !== `ended` && deviceMovePick(next, root) === `move`) void requestDeviceMove(root.id, next)
  }

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

  // A picked version loads its package on demand (the head's and the
  // published one's ride on the synced row).
  useEffect(() => {
    if (viewing === null || viewing === head || viewing === app.publishedVersion) {
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
  }, [viewing, head, app.publishedVersion, app.id])

  // eApps V1: PUBLISH a version (the head by default); the preview returns
  // to its default.
  const [publishing, setPublishing] = useState<number | null>(null)
  const publish = async (number: number) => {
    if (publishing !== null) return
    setPublishing(number)
    try {
      const result = await trpc.apps.publish.mutate({ id: app.id, number }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      toast(APPS_COPY.publishedToast.replace(`{n}`, String(result.published)))
      setViewing(null)
    } catch (error) {
      failedAction(`publish`, `the version`, error)
    } finally {
      setPublishing(null)
    }
  }

  // #VAPP-256 (apps.json `rollbackCases`): a rollback past a migrated version
  // asks first; every other publish goes through at once.
  const [rollbackTarget, setRollbackTarget] = useState<number | null>(null)
  const requestPublish = (number: number) => {
    if (rollbackWarns(app.publishedVersion, number, versions)) setRollbackTarget(number)
    else void publish(number)
  }

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
      // VAPP-242: no Apps page on web ≥ md: the team's home (phones: the Apps tab).
      void (isMobile
        ? navigate({ to: `/t/$teamSlug/apps`, params: { teamSlug }, replace: true })
        : navigate({ to: `/t/$teamSlug`, params: { teamSlug }, replace: true }))
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
    <Pill size="sm" className={app.publishedVersion > 0 ? `tabular-nums` : `text-muted-foreground`}>
      {listPill(app)}
    </Pill>
  )
  const showPublish = canWrite && canPublish(app)
  // VAPP-234: a draft's "Start from a template" opens the import dialog.
  const [importing, setImporting] = useState(false)
  const exportDialog = (
    <>
      <AppExportDialog app={exportOpen ? app : null} onOpenChange={setExportOpen} />
      <AppImportDialog open={importing} onOpenChange={setImporting} teamId={teamId} teamSlug={teamSlug} />
    </>
  )

  // A version's Preview: the Preview tab showing it (phones: the sheet closes).
  const previewVersion = (number: number) => {
    setViewing(number === defaultShown ? null : number)
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
      const result = await sendSessionMessage({ teamId, appId: app.id, deviceId, text, ...heldRunner?.send })
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
          {/* VAPP-234: the template gallery (the import dialog's templates step). */}
          {isMember && (
            <Pill mode="action" size="md" onClick={() => setImporting(true)} data-testid={APP_TEMPLATES_TEST_IDS.fromTemplate}>
              {APP_TEMPLATES_COPY.fromTemplate}
            </Pill>
          )}
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
      deviceMenu={{ value: deviceId, devices: deviceRows, onChange: pickDevice }}
      moveDevice={device.move}
      heldRunner={heldRunner}
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

  // VAPP-179 (wide only): Desktop | Phone at the start; the template Select at
  // the end only when the SHOWN package has more than one template.
  const shownPackage = shown === 0 ? null : shown === head ? headPackage : shown === app.publishedVersion ? publishedPackage : viewingPackage
  // VAPP-218: with screens the Select is hidden (the screens ARE the picker).
  const hasScreens = shownPackage ? packageScreens(shownPackage).length > 0 : false
  const templates = hasScreens ? [] : previewTemplates(shownPackage as { templates?: Record<string, unknown> } | null)
  const shownTemplate = previewTemplate(shownPackage as { templates?: Record<string, unknown> } | null, pickedTemplate)
  const toolbar = isMobile ? null : (
    <div className="flex shrink-0 items-center gap-3" data-testid={APPS_TEST_IDS.previewToolbar}>
      <div className="shrink-0" data-testid={APPS_TEST_IDS.previewFrame}>
        <SegmentedControl
          value={frame}
          onValueChange={setFrame}
          options={(APPS_PREVIEW.frames as PreviewFrame[]).map((id) => ({ value: id, label: APPS_COPY.frame[id] }))}
        />
      </div>
      {runAsChoices && (
        <Select value={runAsId || `me`} onValueChange={(v) => setRunAsId(v === `me` ? `` : v)}>
          <SelectTrigger className="min-w-0" data-testid={APPS_TEST_IDS.previewRunAs}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {runAsChoices.map((o) => (
              <SelectItem key={o.value || `me`} value={o.value || `me`}>
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      <div className="flex-1" />
      {templates.length > 1 && shownTemplate !== null && (
        <Select value={shownTemplate} onValueChange={setPickedTemplate}>
          <SelectTrigger className="min-w-0" data-testid={APPS_TEST_IDS.previewTemplate}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {templates.map((id) => (
              <SelectItem key={id} value={id}>
                {id}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
    </div>
  )

  // VAPP-218 + VAPP-242: the editor's preview keeps its screen in memory
  // (deep links and the route's screen belong to THE RUN VIEW).
  const [screenRequest, setScreenRequest] = useState<ScreenRequest | null>(requestedScreen)
  const openScreen = (request: ScreenRequest) => setScreenRequest(request)

  const preview = (
    <div className="flex h-full min-h-0 flex-col gap-3 p-4">
      {toolbar}
      {viewing !== null && (
        <div
          className="flex items-center gap-3 rounded-md border border-border px-3 py-2 text-sm"
          data-testid={APPS_TEST_IDS.viewing}
        >
          <span className="flex-1 font-medium">{APPS_COPY.viewing.replace(`{n}`, String(viewing))}</span>
          {canWrite && viewing !== app.publishedVersion && (
            <Button size="sm" onClick={() => requestPublish(viewing)} pending={publishing === viewing} disabled={publishing !== null}>
              {APPS_COPY.publish}
            </Button>
          )}
          {canWrite && viewing !== head && (
            <Button size="sm" variant="outline" onClick={() => void restore(viewing)} pending={restoring !== null}>
              {APPS_COPY.restore}
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={() => setViewing(null)}>
            {APPS_COPY.head}
          </Button>
        </div>
      )}
      {/* A grant is about the version it was made on (`grantVersion`): any other previews asking for everything. */}
      <AppPreview pkg={shownPackage} glyph={getActionIcon({ icon: app.icon })} template={pickedTemplate} frame={isMobile ? `desktop` : frame} phone={isMobile} screen={screenRequest} onScreen={openScreen} appId={app.id} teamId={teamId} me={me} grantedTools={shown === grantVersion(app) ? (app.row?.grantedTools ?? []) : []} runAs={isMobile ? null : runAs} readOnly={((isMobile ? null : runAs)?.access ?? access) === `read`} migrateFrom={shown !== app.publishedVersion && app.publishedVersion > 0 ? ((app.row?.package as { version?: string } | null)?.version ?? null) : null} />
    </div>
  )

  // VAPP-177: the Versions and Settings bodies, THE SAME on the wide tab and
  // in the phone menu's sheet (the views' anchors live on them).
  const versionsBody = (
    <div data-testid={APPS_TEST_IDS.versions}>
      <VersionRows
        versions={versions}
        pointers={app}
        viewing={viewing}
        users={users as User[]}
        publishing={publishing}
        canWrite={canWrite}
        onPreview={previewVersion}
        onPublish={requestPublish}
        grouped={isMobile}
      />
    </div>
  )
  const settingsBody = (
    <div className="w-full" style={isMobile ? undefined : { maxWidth: APPS_EDITOR_TABS.settingsMaxWidth }} data-testid={APPS_TEST_IDS.settings}>
      <AppSettingsTab
        teamId={teamId}
        teamSlug={teamSlug}
        app={app}
        canManage={canManage}
        canGrant={canGrant}
        onExport={() => setExportOpen(true)}
        onDelete={() => setDeleteOpen(true)}
      />
    </div>
  )

  // VAPP-178: the Data tab's body (the wide tab and the phone segment).
  const dataBody = (
    <div className="w-full" data-testid={APPS_TEST_IDS.data}>
      <AppDataTab appId={app.id} pkg={headPackage} users={users as User[]} me={me} role={myRole} access={access} isMobile={isMobile} />
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

  const rollbackPrompt = rollbackTarget !== null && (
    <Prompt
      open
      onOpenChange={(next) => !next && setRollbackTarget(null)}
      title={rollbackTitle(rollbackTarget)}
      body={rollbackBody(rollbackTarget)}
      actions={[
        { label: `Cancel`, role: `cancel`, autoFocus: true },
        {
          label: APPS_COPY.publish,
          role: `primary`,
          testId: `app-rollback-confirm`,
          onSelect: () => {
            setRollbackTarget(null)
            void publish(rollbackTarget)
          },
        },
      ]}
    />
  )

  const tabOptions = (ids: readonly string[]) =>
    ids.map((id) => ({ value: id as EditorTab, label: APPS_COPY.tabs[id as keyof typeof APPS_COPY.tabs] }))

  if (isMobile) {
    // The header's `…` (apps.json `editorTabs.menu`).
    const menuEntries: MenuEntry[] = [
      // eApps V1: Publish first, while the head is ahead (the wide header's button).
      ...(showPublish
        ? [
            {
              kind: `item` as const,
              id: `publish`,
              label: APPS_COPY.publish,
              icon: UiSubmitIcon,
              onSelect: () => void publish(head),
              "data-testid": APPS_TEST_IDS.publish,
            },
          ]
        : []),
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
            // #EXP-1729: THE chat header (chats.json `shell.chatHeader`): the
            // lead disc carrying the app's glyph, the name over `status ·
            // device` in its tone; no version pill on phones.
            <PhoneHeaderTitle
              disc={
                <span data-slot="chat-lead" className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted">
                  {glyph}
                </span>
              }
              title={app.name}
              subtitle={<span className={subtitleClass} data-tone={header.tone}>{header.subtitle}</span>}
            />
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
          {tab === `data` && <div className="h-full overflow-y-auto px-4 pt-2 pb-4">{dataBody}</div>}
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
        {rollbackPrompt}
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid={APPS_TEST_IDS.editor}>
      <div className="flex shrink-0 items-center gap-3 border-b border-border px-6 py-4">
        {glyph}
        {/* #EXP-1729: the chat header's title block: the name over `status · device`. */}
        <div className="flex min-w-0 flex-col">
          <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight">{app.name}</h1>
          <span className={cn(`truncate text-xs`, subtitleClass)} data-tone={header.tone} data-testid="chat-header-subtitle">
            {header.subtitle}
          </span>
        </div>
        {versionPill}
        <div className="flex-1" />
        {showPublish && (
          <Button size="sm" onClick={() => void publish(head)} pending={publishing === head} disabled={publishing !== null} data-testid={APPS_TEST_IDS.publish}>
            {APPS_COPY.publish}
          </Button>
        )}
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
          {tab === `data` && <div className="h-full overflow-y-auto p-4">{dataBody}</div>}
          {tab === `versions` && <div className="h-full overflow-y-auto p-4">{versionsBody}</div>}
          {tab === `settings` && <div className="h-full overflow-y-auto p-4">{settingsBody}</div>}
        </div>
      </div>
      {exportDialog}
      {deletePrompt}
      {rollbackPrompt}
    </div>
  )
}

type PreviewFrame = `desktop` | `phone`

const attachmentSrc = (id: string) => `/api/attachments/${id}`

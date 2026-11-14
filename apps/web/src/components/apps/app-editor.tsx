import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  APPS_COPY,
  APPS_EDITOR_TABS,
  APPS_PREVIEW,
  APPS_TEST_IDS,
  canPublish,
  grantVersion,
  isDraftApp,
  listPill,
  previewTemplate,
  previewTemplates,
  runAsOptions,
  shownVersion,
  versionLabel,
  versionRow,
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
  ListRow,
  type MenuEntry,
  Pill,
  Prompt,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SETTINGS_LIST_CLASS,
  sessionScreenMenuRows,
  toast,
  useIsMobile,
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
import {
  SESSION_SCREEN_COPY,
  sessionScreen,
  sessionScreenMenu,
  sessionScreenStatus,
  sessionScreenSubtitle,
  sessionScreenTone,
  type SessionScreenOpen,
  type SessionScreenTab,
} from "@exp/domain-contract/session-screen"
import { SessionScreenFrame } from "@/components/chat/session-screen-frame"
import { SessionScreenMenuButton, sessionMenuLine } from "@/components/chat/chat-menu"
import { CHAT_HEADER_TONE_CLASS } from "@/components/chat/chat-header"
import { useEditorRunner } from "@/components/apps/editor-runner"
import { InPlaceSession } from "@/components/chat/session-body"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import { AppImportDialog } from "@/components/apps/app-import-dialog"
import { APP_TEMPLATES_COPY, APP_TEMPLATES_TEST_IDS } from "@exp/domain-contract/app-templates"
import { relativeTime } from "@/components/comment-rows/format"
import { useAppVersions, useEditorSession, type AppListEntry } from "@/hooks/use-apps"
import { useSessionTitles } from "@/hooks/use-session-tree"
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

// VAPP-110 (fixtures/apps.json): THE EDITOR. #FEED-239: leaving the editor
// no longer ends the member's editor session (it stays as it is); leaving
// an EMPTY draft still discards it.
//
// #EXP-1732 (session-screen.json): the editor IS THE session screen of
// kind `app` (an app IS its agent's session): the header (the BoardGlyph,
// the name + the version pill over ONE subtitle line, Publish, THE `…` =
// THE session screen menu: Compact · New conversation (sent into the
// editor session), App settings (the opened item), Export, Delete app;
// #FEED-239: no gear) and THE SegmentedControl Chat · Preview · Data ·
// Versions (wide drops Chat: the chat IS the left pane); wide = the chat
// (the member's editor session: THE chat conversation rows + THE session
// composer) beside the tab's pane (default Preview), phones = one pane. A
// version's Preview shows it read-only on the Preview tab with a Restore
// banner; Restore = a new head.

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiHistoryIcon = conceptIcon(`ui-history`)

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
  requestedOpen = null,
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
  /** #EXP-1732: the route's `?open=` (settings, a session of the tree; read once). */
  requestedOpen?: { open: SessionScreenOpen; sessionId?: string } | null
  /** VAPP-179: the route's `?frame=` (read once; screenshots open on Phone). */
  requestedFrame?: PreviewFrame | null
  /** VAPP-218: the preview's first screen (null = the package's first); a
   *  tap or `app.navigate` then moves it in memory (VAPP-242). */
  requestedScreen?: ScreenRequest | null
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  // #EXP-1732: the requested tab (`?tab=` read once, legacy ids map through
  // `legacyTabs`; `settings` opens the gear's settings) and the item open
  // over it (settings, a session of the editor tree).
  const [tab, setTab] = useState<string | null>(requestedTab)
  const [opened, setOpened] = useState<{ open: SessionScreenOpen; sessionId?: string } | null>(requestedOpen)
  const [viewing, setViewing] = useState<number | null>(null)
  // VAPP-179: the preview toolbar's frame + picked template (editor lifetime only).
  const [frame, setFrame] = useState<PreviewFrame>(requestedFrame ?? (APPS_PREVIEW.defaultFrame as PreviewFrame))
  const [pickedTemplate, setPickedTemplate] = useState<string | null>(null)
  const [viewingPackage, setViewingPackage] = useState<Record<string, unknown> | null>(null)
  const [restoring, setRestoring] = useState<number | null>(null)
  const [exportOpen, setExportOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  // VAPP-218: a screen link while the editor is open shows the Preview tab.
  const requestedScreenName = requestedScreen?.screen ?? null
  useEffect(() => {
    if (requestedScreenName === null) return
    setOpened(null)
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
  const headerDeviceInput = deviceId
    ? {
        name: deviceRows.find((r) => r.id === deviceId)?.name ?? deviceId,
        online: shownDevice ? deviceIsOnline(shownDevice) : false,
      }
    : null
  // #EXP-1732: the subtitle's device part alone (no root = no word).
  const headerDevice = editorHeader(null, headerDeviceInput)
  // #EXP-1729: Account › and Model › (composer-menu.json, an app editor's
  // composer, always): held picks ride the next send until the root is live.
  const heldRunner = useEditorRunner(root, shownDevice)
  const pickDevice = (next: string) => {
    saveEditorDevice(app.id, next)
    setPickedDevice({ appId: app.id, deviceId: next })
    if (root && root.status !== `ended` && deviceMovePick(next, root) === `move`) void requestDeviceMove(root.id, next)
  }

  // VAPP-176: leaving an EMPTY draft (nothing ever sent) discards it.
  // #FEED-239: leaving never ends the editor session any more.
  const rootRef = useRef<SessionRow | null>(null)
  rootRef.current = root
  const draftRef = useRef(isDraftApp(app))
  draftRef.current = isDraftApp(app)
  useEffect(() => {
    const onPageHide = () => {
      if (!rootRef.current && draftRef.current) discardDraft(app.id)
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

  // The delete prompt (THE `…`'s Delete app): leaves for the Apps list.
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

  // #EXP-1732: the editor session's tree (the status word, a session
  // opened in place).
  const tree = useMemo(
    () => (root ? teamSessions.filter((row) => (row.rootId ?? row.id) === root.id) : []),
    [teamSessions, root]
  )
  const { titles } = useSessionTitles(tree)
  /** A session of the editor tree opens IN PLACE (the opened item); any
   *  other on its own route. */
  const openSession = (session: SessionRow) => {
    if (root && session.id !== root.id && (session.rootId ?? session.id) === root.id) {
      setOpened({ open: `session`, sessionId: session.id })
      return
    }
    void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: session.id } })
  }

  const glyph = (
    <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted">
      <BoardGlyph board={{ icon: app.icon, color: app.color }} className="size-5" />
    </span>
  )
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
    setOpened(null)
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

  // #EXP-1732: the Chat pane IS the editor session's own session view (its
  // raw work, THE Dock, the app's composer with its device menu); before the
  // first send, the draft's starters (or the empty state) over the composer.
  const appComposer = (opaque: boolean) =>
    isMember ? (
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
        opaque={opaque}
        hideReplies={opaque}
        testId="app-composer"
        className={opaque ? undefined : `mx-auto max-w-3xl`}
      />
    ) : null
  const empty = draft ? (
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

  const chat = root ? (
    <div className="flex h-full min-h-0 flex-col" data-testid={APPS_TEST_IDS.chat}>
      <InPlaceSession
        key={root.id}
        row={root}
        floating={isMobile}
        onOpenXref={(sessionId) => {
          const target = teamSessions.find((row) => row.id === sessionId)
          if (target) openSession(target)
        }}
        composer={isMember ? appComposer : undefined}
      />
    </div>
  ) : (
    <div className="flex h-full min-h-0 flex-col" data-testid={APPS_TEST_IDS.chat}>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col justify-end px-4 pt-6 pb-6">{empty}</div>
      </div>
      {isMember && (
        <div className="shrink-0 px-4 pt-2 pb-4" data-testid={APPS_TEST_IDS.composer}>
          {appComposer(false)}
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

  // ── THE screen (session-screen.json, kind `app`) ─────────────────────────
  const openSessionRow =
    opened?.open === `session` ? (tree.find((row) => row.id === opened.sessionId) ?? null) : null
  const open: SessionScreenOpen | null =
    opened?.open === `settings` ? `settings` : openSessionRow ? `session` : null
  const screen = sessionScreen({
    kind: `app`,
    wide: !isMobile,
    requested: tab,
    open,
    counts: { issues: 0, sessions: 0, results: 0 },
  })
  const pickTab = (next: SessionScreenTab) => {
    setOpened(null)
    setTab(next)
  }
  /** Phones: the opened item's `‹`: back to the tab (a requested `settings` too). */
  const closeItem = () => {
    setOpened(null)
    setTab(screen.active)
  }

  let pane: ReactNode = null
  if (screen.active === `preview`) pane = preview
  else if (screen.active === `data`) pane = <div className="h-full overflow-y-auto p-4">{dataBody}</div>
  else if (screen.active === `versions`) pane = <div className="h-full overflow-y-auto p-4">{versionsBody}</div>

  const item: ReactNode =
    screen.open === `settings` ? (
      <div className="h-full overflow-y-auto p-4">{settingsBody}</div>
    ) : screen.open === `session` && openSessionRow ? (
      <InPlaceSession key={openSessionRow.id} row={openSessionRow} floating={isMobile} />
    ) : null

  // #FEED-239 (session-screen.json `menuCases`): the header's `…` = THE
  // session screen menu: Publish (phones; the wide header's button) ·
  // Compact, New conversation (sent into the editor session, the agent
  // menu's path) · App settings (the opened item) · Export, Delete app.
  const sendLine = async (text: string) => {
    try {
      const result = root
        ? await sendSessionMessage({ teamId, sessionId: root.id, text }, SKIP_ERROR_TOAST)
        : await sendSessionMessage({ teamId, appId: app.id, deviceId: deviceId ?? undefined, text, ...heldRunner?.send }, SKIP_ERROR_TOAST)
      await sessionCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch (error) {
      failedAction(`send`, `the message`, error)
    }
  }
  const menuEntries: MenuEntry[] = sessionScreenMenuRows(
    `app`,
    sessionScreenMenu({
      kind: `app`,
      phone: isMobile,
      member: isMember,
      linkIssue: false,
      settings: true,
      publish: showPublish,
      export: head > 0,
      delete: canManage,
    }),
    (entry) => {
      const line = sessionMenuLine(entry)
      if (line) void sendLine(line)
      else if (entry === `publish`) void publish(head)
      else if (entry === `settings`) setOpened({ open: `settings` })
      else if (entry === `export`) setExportOpen(true)
      else if (entry === `delete`) setDeleteOpen(true)
    }
  )
  const headerMenu = (className: string) => (
    <SessionScreenMenuButton entries={menuEntries} title={app.name} className={className} />
  )

  // #EXP-1732 (session-screen.json STATUS WORD): THE tree's word (never
  // dropped) · apps.json `editorHeader`'s DEVICE part (the name, else `no
  // device online`).
  const statusRows = root && !tree.some((row) => row.id === root.id) ? [root, ...tree] : tree
  const status = sessionScreenStatus(
    statusRows.map((row) => ({
      status: row.status,
      busy: row.busy,
      needsInput: row.needsInput,
      root: row.id === root?.id,
    }))
  )
  const subtitle = sessionScreenSubtitle({ status, device: headerDevice.subtitle, trigger: null })
  // session-screen.json TONE ×4: the word's tone, amber while no device is online.
  const tone = sessionScreenTone(status, headerDeviceInput?.online !== true)

  return (
    <>
      <SessionScreenFrame
        screen={screen}
        testId={APPS_TEST_IDS.editor}
        avatar={glyph}
        title={app.name}
        titleTrailing={versionPill}
        subtitle={subtitle}
        subtitleClass={CHAT_HEADER_TONE_CLASS[tone]}
        trailing={
          showPublish && !isMobile ? (
            <Button size="sm" onClick={() => void publish(head)} pending={publishing === head} disabled={publishing !== null} data-testid={APPS_TEST_IDS.publish}>
              {APPS_COPY.publish}
            </Button>
          ) : undefined
        }
        menu={headerMenu}
        onBack={() => void navigate({ to: `/t/$teamSlug/apps`, params: { teamSlug } })}
        onTab={pickTab}
        chat={chat}
        pane={pane}
        item={item}
        onCloseItem={closeItem}
        itemTitle={
          screen.open === `settings`
            ? SESSION_SCREEN_COPY.settings
            : openSessionRow
              ? (titles.get(openSessionRow.id) ?? ``)
              : ``
        }
      />
      {exportDialog}
      {deletePrompt}
      {rollbackPrompt}
    </>
  )
}

type PreviewFrame = `desktop` | `phone`

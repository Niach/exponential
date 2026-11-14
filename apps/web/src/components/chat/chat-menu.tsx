import { useMemo, useState, type ReactNode } from "react"
import { sendSessionMessage } from "@/lib/enet/send-sealed"
import { useNavigate, useRouter } from "@tanstack/react-router"
import {
  CHATS_COPY,
  chatMenuEntries,
} from "@exp/domain-contract/chats"
import { isAssistant } from "@exp/domain-contract/agent-settings"
import {
  SESSION_SCREEN_MENU_SENDS,
  SESSION_SCREEN_TEST_IDS,
  sessionScreenMenu,
  type SessionScreenMenuEntry,
} from "@exp/domain-contract/session-screen"
import {
  Button,
  chatMenuRows,
  sessionScreenMenuRows,
  conceptIcon,
  Menu,
  Prompt,
  toastActionError,
  type MenuEntry,
  type MenuSessionProps,
} from "@exp/ui"
import { useLongPressSelects, type IssueMenuSelection } from "@/components/issue-context-menu/selection"
import { useSession } from "@/hooks/use-session"
import { useAgentRow, useMyAgentConversation } from "@/hooks/use-agent-row"
import { useSessionRow, useSessionTitles } from "@/hooks/use-session-tree"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamUsers } from "@/hooks/use-team-data"
import type { Team } from "@/db/schema"
import { actionErrorDescription, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { messageCollection, sessionCollection } from "@/lib/collections"
import { canSteerSession } from "@/lib/session-account-switch"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// Chat 58cb32df (fixtures/chats.json `menuCases`) + #FEED-239
// (session-screen.json `menuCases`): the chat's and the agent's menus ×web.
// A chat row's right-click / long-press (kind `chat` through THE team
// layout's `MenuGestureHost`, `menuProps(CHAT_MENU_KIND, rootId)`) = Delete;
// every session screen's `…` and an agent row's right-click / long-press
// (kind `agent`) = THE session screen menu (`sessionScreenMenu`):
//
//   Compact / New    SEND `/compact` / `/new` (one path with the typed ones)
//   Agent settings   the agent's settings as the screen's opened item (a
//                    sidebar row: that agent's chat with `?open=settings`),
//                    never the Settings screen
//   Delete           `sessions.delete` (the server ends what is live first)
//
// S-3: no Stop / End anywhere (the composer's stop-turn is the only stop).
//
// A delete asks first (`copy.confirmDelete`); after it the open chat closes
// back to the list. Failures toast `copy.toasts.*`.

export { CHAT_ROW_MENU_KIND as CHAT_MENU_KIND, AGENT_ROW_MENU_KIND as AGENT_MENU_KIND } from "@exp/ui"

const COPY = CHATS_COPY
const MoreIcon = conceptIcon(`ui-more`)

const toastFailed = (title: string, error: unknown) =>
  toastActionError(title, { description: actionErrorDescription(error) })

/** Delete a chat (a ROOT session) behind its confirm. `dialog` = the
 *  confirm, mounted by the host (it outlives the menu). */
export function useChatActions(teamSlug: string | undefined): {
  requestDelete: (rootId: string) => void
  dialog: ReactNode
} {
  const navigate = useNavigate()
  const router = useRouter()
  const [deleting, setDeleting] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const remove = async (rootId: string) => {
    setBusy(true)
    try {
      const result = await trpc.sessions.delete.mutate({ sessionId: rootId }, SKIP_ERROR_TOAST)
      setDeleting(null)
      // The open chat closes back to the list.
      const path = router.state.location.pathname
      if (teamSlug && path.endsWith(`/sessions/${rootId}`)) {
        void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug } })
      }
      await sessionCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch (error) {
      toastFailed(COPY.toasts.deleteFailed, error)
    } finally {
      setBusy(false)
    }
  }

  const dialog = (
    <Prompt
      open={deleting !== null}
      onOpenChange={(open) => {
        if (!open && !busy) setDeleting(null)
      }}
      busy={busy}
      title={COPY.confirmDelete.title}
      body={COPY.confirmDelete.body}
      data-testid="chat-delete-confirm"
      actions={[
        { label: `Cancel`, role: `cancel` },
        {
          label: COPY.menu.delete,
          role: `destructive`,
          busy,
          onSelect: () => {
            if (deleting) void remove(deleting)
          },
        },
      ]}
    />
  )

  return { requestDelete: setDeleting, dialog }
}

/** Whether the viewer may delete the chat: its starter or a team owner
 *  (`sessions.delete`), while they may steer it. */
function useCanDeleteChat(root: Pick<SessionRow, `teamId` | `shared` | `userId`> | null): boolean {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const { members } = useTeamUsers(root?.teamId)
  if (!root || !me) return false
  const isOwner = members.some((m) => m.userId === me && m.role === `owner`)
  return (root.userId === me || isOwner) && canSteerSession(root, me)
}

/** A chat's menu entries for the viewer (`chatMenuEntries`): the delete
 *  entries only for the chat's starter or a team owner (`sessions.delete`). */
export function useChatMenuEntries(
  root: Pick<SessionRow, `id` | `teamId` | `status` | `shared` | `userId`> | null,
  actions: Pick<ReturnType<typeof useChatActions>, `requestDelete`>
): MenuEntry[] {
  const canDelete = useCanDeleteChat(root)
  if (!root) return []
  const entries = chatMenuEntries({ rootStatus: root.status, canSteer: canDelete, canDelete })
  return chatMenuRows(entries, () => actions.requestDelete(root.id))
}

/** #FEED-239: a chat's session screen menu (`sessionScreenMenu`, kind
 *  `session`): Delete for its starter or a team owner. */
export function useChatScreenMenuRows(
  root: Pick<SessionRow, `id` | `teamId` | `status` | `shared` | `userId`> | null,
  actions: Pick<ReturnType<typeof useChatActions>, `requestDelete`>,
  /** #EXP-1834: THE link menu's rows (a member), absent = no Link entry. */
  linkIssue?: readonly MenuEntry[]
): MenuEntry[] {
  const canDelete = useCanDeleteChat(root)
  if (!root) return []
  const entries = sessionScreenMenu({
    kind: `session`,
    phone: false,
    member: false,
    linkIssue: Boolean(linkIssue?.length),
    settings: false,
    publish: false,
    export: false,
    delete: canDelete,
  })
  return sessionScreenMenuRows(
    `session`,
    entries,
    (entry) => {
      if (entry === `delete`) actions.requestDelete(root.id)
    },
    linkIssue
  )
}

const SelectIcon = conceptIcon(`ui-selected`)

/** The `chat` kind's menu at the pointer (a sheet titled with the chat's
 *  title on a phone). #EXP-1388: on the Chats list (`selection` = the list's
 *  adapter) a pointer's menu starts with Select; a touch long-press enters
 *  the selection with the row instead of opening a menu. */
export function ChatMenuSession({
  target,
  open,
  onOpenChange,
  actions,
  selection,
}: MenuSessionProps & { actions: ReturnType<typeof useChatActions>; selection?: IssueMenuSelection }) {
  const { row } = useSessionRow(target.id)
  const root = row && row.parentId === null && !row.agentId ? row : null
  // #EXP-1380 (chats.json LIST DATA): the title reads the digest, never a
  // transcript.
  const rootRows = useMemo(() => (root ? [root] : []), [root])
  const { titles } = useSessionTitles(rootRows)
  const chatEntries = useChatMenuEntries(root, actions)
  const longPressSelects = useLongPressSelects(selection, target.id, open, onOpenChange)
  if (longPressSelects) return null
  const entries: MenuEntry[] = selection
    ? [
        {
          kind: `item`,
          id: `select`,
          label: COPY.selection.select,
          icon: SelectIcon,
          "data-testid": `chat-menu-select`,
          onSelect: () => selection.toggle(target.id),
        },
        ...(chatEntries.length > 0 ? [{ kind: `separator` as const, id: `select-separator` }] : []),
        ...chatEntries,
      ]
    : chatEntries
  if (entries.length === 0) return null
  const title = (root ? titles.get(root.id) : undefined) || COPY.title
  return (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={open}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label={title}
      title={title}
      contentClassName="w-60"
      data-testid="chat-menu"
      entries={entries}
    />
  )
}

/** #FEED-239: THE session screen's `…` (every kind): the entries the host
 *  builds (`sessionScreenMenuRows`, + a chat's Guide PR rows). */
export function SessionScreenMenuButton({
  entries,
  title,
  className,
  closeRef,
}: {
  entries: readonly MenuEntry[]
  title: string
  className?: string
  /** #EXP-1834: set to this menu's close (the link list's pick closes it). */
  closeRef?: { current: () => void }
}) {
  const [open, setOpen] = useState(false)
  if (closeRef) closeRef.current = () => setOpen(false)
  if (entries.length === 0) return null
  return (
    <Menu
      open={open}
      onOpenChange={setOpen}
      align="end"
      aria-label={title}
      title={title}
      contentClassName="w-60"
      data-testid={`${SESSION_SCREEN_TEST_IDS.menu}-content`}
      entries={entries}
      trigger={
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="More"
          data-testid={SESSION_SCREEN_TEST_IDS.menu}
          className={className}
        >
          <MoreIcon />
        </Button>
      }
    />
  )
}


/** A menu line (`SESSION_SCREEN_MENU_SENDS`: `/compact`, `/new`) into the
 *  viewer's conversation with the agent (or a fresh one), the failure as a
 *  toast. #EXP-1383: the Dock's "Instructions changed" banner sends its
 *  `/new` through here too, so both are one path. */
export async function sendAgentLine({
  teamId,
  agentId,
  conversationId,
  text,
}: {
  teamId: string
  agentId: string
  conversationId: string | null
  text: string
}): Promise<void> {
  try {
    const result = conversationId
      ? await sendSessionMessage({ teamId, sessionId: conversationId, text }, SKIP_ERROR_TOAST)
      : await sendSessionMessage({ agentId, text }, SKIP_ERROR_TOAST)
    await messageCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
  } catch (error) {
    toastFailed(`Couldn't send the message`, error)
  }
}

/** #FEED-239: what a menu entry sends (`menuSends`), null = not a line. */
export function sessionMenuLine(entry: Exclude<SessionScreenMenuEntry, `separator`>): string | null {
  return (SESSION_SCREEN_MENU_SENDS as Record<string, string>)[entry] ?? null
}

/** #FEED-239: THE session screen menu of an agent (or the Assistant):
 *  Compact and New conversation SEND their line (members), Agent settings
 *  (team owners; the Assistant: its owner) opens the settings as the
 *  screen's item: `onSettings` on its own screen, else (a sidebar row)
 *  that agent's chat with `?open=settings`. */
export function useAgentMenuRows({
  team,
  agentId,
  onSettings,
  linkIssue,
}: {
  team: Team | null | undefined
  agentId: string | null
  /** On the agent's own screen: open the settings item there. */
  onSettings?: () => void
  /** #EXP-1834: on the agent's own screen, THE link menu's rows. */
  linkIssue?: readonly MenuEntry[]
}): MenuEntry[] {
  const navigate = useNavigate()
  const permissions = useTeamPermissions(team ?? null)
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const conversation = useMyAgentConversation(agentId, me)
  const { agent } = useAgentRow(agentId)
  // #EXP-1714: the viewer's global Assistant lives outside the team.
  const assistant = agent ? isAssistant(agent) : false
  const inTeam = agent?.teamId === team?.id || (assistant && agent?.ownerId === me)
  if (!team || !agentId || !agent || !inTeam || !permissions.isMember) return []
  const kind = assistant ? `assistant` : `agent`
  const entries = sessionScreenMenu({
    kind,
    phone: false,
    member: permissions.isMember,
    linkIssue: Boolean(linkIssue?.length),
    settings: assistant ? agent.ownerId === me : permissions.isOwner,
    publish: false,
    export: false,
    delete: false,
  })
  return sessionScreenMenuRows(kind, entries, (entry) => {
    const line = sessionMenuLine(entry)
    if (line) {
      void sendAgentLine({ teamId: team.id, agentId, conversationId: conversation?.id ?? null, text: line })
      return
    }
    if (entry !== `settings`) return
    if (onSettings) onSettings()
    else
      void navigate({
        to: `/t/$teamSlug/agents/$agentId`,
        params: { teamSlug: team.slug, agentId },
        search: { open: `settings` },
      })
  }, linkIssue)
}

/** The `agent` kind's menu at the pointer (a bubble's long-press, the
 *  sidebar's agent row). */
export function AgentMenuSession({
  target,
  open,
  onOpenChange,
  team,
}: MenuSessionProps & { team: Team | null }) {
  const { agent } = useAgentRow(target.id)
  const entries = useAgentMenuRows({ team, agentId: target.id })
  if (entries.length === 0) return null
  const title = agent?.name ?? ``
  return (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={open}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label={title}
      title={title}
      contentClassName="w-60"
      data-testid="agent-menu"
      entries={entries}
    />
  )
}

/** The agent chat header's `…` (THE session screen menu). */
export function AgentHeaderMenu({
  team,
  agentId,
  title,
  onSettings,
  linkIssue,
  closeRef,
  className,
}: {
  team: Team | null | undefined
  agentId: string
  title: string
  onSettings: () => void
  linkIssue?: readonly MenuEntry[]
  closeRef?: { current: () => void }
  className?: string
}) {
  const entries = useAgentMenuRows({ team, agentId, onSettings, linkIssue })
  return <SessionScreenMenuButton entries={entries} title={title} className={className} closeRef={closeRef} />
}

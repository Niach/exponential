import { useMemo, useState, type ReactNode } from "react"
import { useNavigate, useRouter } from "@tanstack/react-router"
import {
  agentMenuEntries,
  AGENT_MENU_SENDS,
  CHATS_COPY,
  threadMenuEntries,
} from "@exp/domain-contract/chats"
import {
  agentMenuRows,
  Button,
  chatMenuRows,
  conceptIcon,
  Menu,
  Prompt,
  toastActionError,
  type MenuEntry,
  type MenuSessionProps,
} from "@exp/ui"
import { useLongPressSelects, type IssueMenuSelection } from "@/components/issue-context-menu/selection"
import { useSession } from "@/hooks/use-session"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSessionRow, useSessionTitles, useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamUsers } from "@/hooks/use-team-data"
import type { Team } from "@/db/schema"
import { actionErrorDescription, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { messageCollection, sessionCollection } from "@/lib/collections"
import { canSteerSession } from "@/lib/session-account-switch"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// Thread 58cb32df (fixtures/chats.json `menuCases` + `agentMenuCases`): the
// chat's and the agent's menus ×web. ONE set of entries behind every way in:
// a chat row's right-click / long-press (kind `thread` through THE team
// layout's `MenuGestureHost`, `menuProps(CHAT_MENU_KIND, rootId)`), the
// thread header's `…`; the agent row's right-click / long-press (kind
// `agent`) and the agent chat header's `…`.
//
//   Delete           `sessions.delete` (the server ends what is live first)
//   Compact / New    SEND `/compact` / `/new` (one path with the typed ones)
//   Agent settings   the agent's settings page (owners)
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
      if (teamSlug && path.endsWith(`/threads/${rootId}`)) {
        void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } })
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

/** A chat's menu entries for the viewer (`threadMenuEntries`): the delete
 *  entries only for the chat's starter or a team owner (`sessions.delete`). */
export function useChatMenuEntries(
  root: Pick<SessionRow, `id` | `teamId` | `status` | `shared` | `userId`> | null,
  actions: Pick<ReturnType<typeof useChatActions>, `requestDelete`>
): MenuEntry[] {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const { members } = useTeamUsers(root?.teamId)
  if (!root) return []
  const isOwner = Boolean(me) && members.some((m) => m.userId === me && m.role === `owner`)
  const canDelete = Boolean(me) && (root.userId === me || isOwner)
  const entries = threadMenuEntries({ rootStatus: root.status, canSteer: canSteerSession(root, me), canDelete })
  return chatMenuRows(entries, () => actions.requestDelete(root.id))
}

const SelectIcon = conceptIcon(`ui-selected`)

/** The `thread` kind's menu at the pointer (a sheet titled with the chat's
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

/** The thread header's `…`: the chat's entries (+ the host's extra rows,
 *  the Guide PR's Close). */
export function ChatHeaderMenu({
  entries,
  title,
  className,
}: {
  entries: readonly MenuEntry[]
  title: string
  className?: string
}) {
  const [open, setOpen] = useState(false)
  if (entries.length === 0) return null
  return (
    <Menu
      open={open}
      onOpenChange={setOpen}
      align="end"
      aria-label={title}
      title={title}
      contentClassName="w-60"
      data-testid="chat-header-menu"
      entries={entries}
      trigger={
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="More"
          data-testid="chat-header-more"
          className={className}
        >
          <MoreIcon />
        </Button>
      }
    />
  )
}

/** The viewer's conversation with an agent (a root with its `agentId`). */
function useAgentConversation(teamId: string | undefined, agentId: string | null): SessionRow | null {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const { rows } = useTeamSessions(teamId)
  return useMemo(
    () =>
      agentId
        ? (rows.find((row) => row.agentId === agentId && row.userId === me && row.parentId === null) ?? null)
        : null,
    [rows, agentId, me]
  )
}

/** An agent menu line (`AGENT_MENU_SENDS`: `/compact`, `/new`) into the
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
      ? await trpc.sessions.send.mutate({ teamId, sessionId: conversationId, text }, SKIP_ERROR_TOAST)
      : await trpc.sessions.send.mutate({ agentId, text }, SKIP_ERROR_TOAST)
    await messageCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
  } catch (error) {
    toastFailed(`Couldn't send the message`, error)
  }
}

/** The agent menu's rows (`agentMenuEntries`): Compact and New conversation
 *  SEND their builtin line; settings for owners. */
export function useAgentMenuEntries({
  team,
  agentId,
}: {
  team: Team | null | undefined
  agentId: string | null
}): MenuEntry[] {
  const navigate = useNavigate()
  const permissions = useTeamPermissions(team ?? null)
  const conversation = useAgentConversation(team?.id, agentId)
  const { agent } = useAgentRow(agentId)
  if (!team || !agentId || !agent || agent.teamId !== team.id || !permissions.isMember) return []
  const busy = conversation?.busy ?? false

  const send = (text: string) =>
    sendAgentLine({ teamId: team.id, agentId, conversationId: conversation?.id ?? null, text })
  return agentMenuRows(agentMenuEntries({ busy, isOwner: permissions.isOwner }), (entry) => {
    const line = AGENT_MENU_SENDS[entry]
    if (line) void send(line)
    else void navigate({ to: `/t/$teamSlug/settings/agents/$agentId`, params: { teamSlug: team.slug, agentId } })
  })
}

/** The `agent` kind's menu at the pointer (a bubble's long-press, the
 *  rail's agent row). */
export function AgentMenuSession({
  target,
  open,
  onOpenChange,
  team,
}: MenuSessionProps & { team: Team | null }) {
  const { agent } = useAgentRow(target.id)
  const entries = useAgentMenuEntries({ team, agentId: target.id })
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

/** The agent chat header's `…`. */
export function AgentHeaderMenu({
  team,
  agentId,
  title,
  className,
}: {
  team: Team | null | undefined
  agentId: string
  title: string
  className?: string
}) {
  const entries = useAgentMenuEntries({ team, agentId })
  return <ChatHeaderMenu entries={entries} title={title} className={className} />
}

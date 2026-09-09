import { useEffect, useMemo, useRef, useState } from "react"
import {
  createFileRoute,
  redirect,
  useCanGoBack,
  useNavigate,
  useRouter,
} from "@tanstack/react-router"
import { ChevronDown, LoaderCircle } from "lucide-react"
import { MAX_ACTION_INPUT_TEXT } from "@exp/db-schema/domain"
import type { User } from "@/db/schema"
import { AgentSessionView, useSteerConfig } from "@/components/agent-session"
import {
  EndedSessionRow,
  pastRunRowByline,
} from "@/components/agent-session-row"
import { Composer, ComposerSubmit } from "@/components/composer"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { AGENT_LABELS } from "@/components/launch-dialog/launch-options-pane"
import { useLaunchOptions } from "@/components/launch-dialog/use-launch-options"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { GlassSectionHeader } from "@/components/ui/glass-rows"
import { Pill } from "@/components/ui/pill"
import { Switch } from "@/components/ui/switch"
import { conceptIcon } from "@/lib/icons.generated"
import { agentSupportsPlanMode } from "@/lib/coding-launch-prefs"
import { BUILTIN_CHAT_ID, BUILTIN_CHAT_NAME } from "@/lib/builtin-actions"
import { isChatSession, sessionIdentity } from "@/lib/session-identity"
import { deviceHasRunnableAgent, deviceIsOnline } from "@/lib/steer-devices"
import {
  rowPrState,
  useAgentsData,
  usePastRuns,
  useSessionRow,
} from "@/hooks/use-agents-data"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// EXP-739: the team's Chat page — a conversation with an agent bound to no
// issue and (unlike the launch dialog's Chat tab) to no repository either: a
// repo-less chat runs in the agent's scratch directory with the Exponential
// MCP server wired up, so it can read and write the tracker without a
// worktree. Picking a repo is still possible from the start-coding dialog,
// which gives the run its own `exp/chat-<id8>` worktree.
//
// Like the session route, this page renders a session BY ID and survives that
// session ending: `?session=` names one explicitly (the dock strip's tabs use
// it, so a second running chat is reachable), otherwise the newest running
// chat, otherwise the one this page was last showing (`heldId`) — an agent
// closing itself out, a kill or a heartbeat gap must not yank the transcript
// out from under the reader. Only when there is nothing to show at all does
// the page fall back to the start card and the caller's past chats.

const UiBackIcon = conceptIcon(`ui-back`)

type ChatSearch = { session?: string }

export const Route = createFileRoute(`/t/$teamSlug/chat`)({
  validateSearch: (search: Record<string, unknown>): ChatSearch => ({
    session:
      typeof search.session === `string` && search.session !== ``
        ? search.session
        : undefined,
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ChatPage,
})

function ChatPage() {
  const { teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const router = useRouter()
  const canGoBack = useCanGoBack()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  // EXP-790: the prompt box is the mention field, so `@` needs the roster.
  const { users: teamUsers } = useTeamUsers(team?.id)

  const currentUserId = authSession?.user?.id
  const teamId = team?.id
  // Steer tickets require team membership and a configured relay; the server
  // enforces both at mint time, this only decides what renders.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  const { running } = useAgentsData(teamId, currentUserId)
  // `running` is already newest-first, so the head is the current chat.
  const newestRunningChat = useMemo(
    () => running.find((row) => isChatSession(row.session)),
    [running]
  )
  const newestRunningChatId = newestRunningChat?.session.id ?? null

  // What the page was last showing on its own: the id sticks so an ending run
  // leaves the transcript on screen (read-only) instead of unmounting it.
  const [heldId, setHeldId] = useState<string | null>(null)
  useEffect(() => {
    if (newestRunningChatId) setHeldId(newestRunningChatId)
  }, [newestRunningChatId])

  const viewId = search.session ?? newestRunningChatId ?? heldId
  const { row, session, isReady } = useSessionRow(
    teamId,
    currentUserId,
    viewId ?? undefined
  )

  // EXP-739: the newest 20 CHATS, not the chats among the newest 20 runs.
  const { past } = usePastRuns(teamId, currentUserId, { only: isChatOnly })

  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId,
  })

  const goBack = () => {
    if (canGoBack) {
      router.history.back()
      return
    }
    void navigate({ to: `/t/$teamSlug/devices`, params: { teamSlug } })
  }

  // Put the page back on the start card: forget what it was holding AND drop
  // any `?session=`, so nothing resolves a session id any more.
  const startNewChat = () => {
    setHeldId(null)
    void navigate({
      to: `/t/$teamSlug/chat`,
      params: { teamSlug },
      search: {},
    })
  }

  if (!team || !currentUserId) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  if (viewId && !isReady) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  if (viewId && session && row) {
    // EXP-312: a teammate's run — the synced row is all this client may ever
    // see. No AgentSessionView, so no ticket is minted.
    if (session.userId !== currentUserId) {
      const identity = sessionIdentity(row)
      return (
        <div className="flex h-full min-h-0 flex-col">
          <ChatHeader title={identity.subject} onBack={goBack} />
          <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
            <SessionStatusBadge
              session={session}
              prState={rowPrState(session, row.issue)}
            />
            <p className="text-sm text-muted-foreground">
              Only the owner can steer this session.
            </p>
          </div>
        </div>
      )
    }

    const live = session.status === `running` || session.status === `in_review`
    return (
      <div className="flex h-full min-h-0 flex-col">
        {/* The run finished while the page was open (or it was reached by id
            after the fact): the transcript stays, read-only, and the only
            forward move is a fresh chat. */}
        {!live && (
          <div className="flex shrink-0 items-center gap-3 border-b border-border px-3 py-1.5">
            <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
              This chat has ended
            </span>
            <Button variant="outline" size="sm" onClick={startNewChat}>
              New chat
            </Button>
          </div>
        )}
        <AgentSessionView
          key={session.id}
          session={session}
          currentUserId={currentUserId}
          identity={sessionIdentity(row)}
          mergeTarget={row.mergeTarget}
          onBack={goBack}
        />
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <ChatHeader title={BUILTIN_CHAT_NAME} onBack={goBack} />

      {/* EXP-772: an essentially empty page — one centred prompt box with a
          subtle picker row under it. The mobile tab bar hides on this route
          (its own back header takes over), so there is no floating pill to
          reserve clearance for. */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-10 px-4 py-6">
          {steerEnabled ? (
            <ChatPrompt
              devices={remote.devices}
              starting={remote.starting}
              sentTo={remote.sentTo}
              users={teamUsers}
              onStart={(device, options, inputs) => {
                remote
                  .runAction(
                    device,
                    {
                      id: BUILTIN_CHAT_ID,
                      name: BUILTIN_CHAT_NAME,
                      teamId: team.id,
                    },
                    options,
                    inputs
                  )
                  .catch(() => {})
              }}
            />
          ) : (
            <p className="my-auto text-center text-sm text-muted-foreground">
              Live steering is unavailable on this instance.
            </p>
          )}

          {past.length > 0 && (
            <div>
              <GlassSectionHeader label="Past chats" />
              <div className="flex flex-col gap-2">
                {past.map((row) => (
                  <EndedSessionRow
                    key={row.session.id}
                    row={{ session: row.session }}
                    title={row.title}
                    byline={pastRunRowByline(row)}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

/** Module-level so the `usePastRuns` memo dependency is stable. */
function isChatOnly(session: Parameters<typeof isChatSession>[0]): boolean {
  return isChatSession(session)
}

/** The page's own header — the AgentSessionView draws its own. */
function ChatHeader({
  title,
  onBack,
}: {
  title: string
  onBack: () => void
}) {
  return (
    <div className="flex items-center gap-1 border-b border-border px-1 py-1.5 md:px-3">
      <Button
        variant="ghost"
        size="icon"
        className="shrink-0 md:hidden"
        aria-label="Back"
        onClick={onBack}
      >
        <UiBackIcon />
      </Button>
      <span className="min-w-0 flex-1 truncate text-sm font-medium max-md:text-center">
        {title}
      </span>
      {/* Balances the back button so the title stays optically centred. */}
      <span className="size-9 shrink-0 md:hidden" />
    </div>
  )
}

/** EXP-790: the chips over the empty prompt box. Each inserts its text with
 *  the `#` last, so the issue-ref autocomplete opens on the caret at once —
 *  the chat's job is mostly "do this to that issue". Hand-mirrored ×4. */
export const CHAT_SUGGESTIONS: readonly string[] = [`Fix #`, `Explain #`, `Review #`]

/** EXP-772: the chat launcher — one wide prompt box, and under it a single
 * muted row of inline pickers (machine → agent → plan) seeded from the
 * selected machine's launch defaults. EXP-790: the box is the mention field
 * (`@` members, `#` issue refs, `:` emoji), model and effort stay the machine's
 * defaults (they left the row with the session composer's pickers), and three
 * suggestion chips sit over the empty field. Enter sends, Shift+Enter breaks
 * the line. Plan mode starts OFF here: a chat is a conversation, not a change
 * proposal. */
function ChatPrompt({
  devices,
  starting,
  sentTo,
  users,
  onStart,
}: {
  /** null while the first device lookup is in flight. */
  devices: ReturnType<typeof useRemoteStart>[`devices`]
  starting: boolean
  sentTo: string | null
  /** The team roster, for the field's `@` autocomplete. */
  users: User[]
  onStart: (
    device: NonNullable<ReturnType<typeof useRemoteStart>[`devices`]>[number],
    options: ReturnType<ReturnType<typeof useLaunchOptions>[`buildOptions`]>,
    inputs: Record<string, string>
  ) => void
}) {
  const [prompt, setPrompt] = useState(``)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  // The same candidate filter the launch dialog uses (EXP-403/EXP-409): the
  // registry lists offline machines and signed-out ones, neither is startable.
  const candidateDevices = useMemo(
    () => (devices ?? []).filter(deviceIsOnline).filter(deviceHasRunnableAgent),
    [devices]
  )
  // `open: true` — this page IS the launcher, there is no dialog to settle on.
  const launch = useLaunchOptions({
    open: true,
    devices: candidateDevices,
    planModeOff: true,
  })
  // EXP-773: an agent outside the machine's reported ACP set has no transport
  // left to start on — the note under the pickers says so.
  const blocked =
    starting ||
    !launch.device ||
    launch.agentNotReady ||
    prompt.trim().length === 0
  const send = () => {
    if (!launch.device || blocked) return
    onStart(launch.device, launch.buildOptions(), { prompt })
  }

  return (
    <div className="my-auto flex flex-col gap-2">
      {/* EXP-790: the chips only while there is nothing typed — once the
          field has text they would just be in the way. */}
      {prompt.length === 0 && (
        <div className="flex flex-wrap items-center gap-1.5 px-1">
          {CHAT_SUGGESTIONS.map((suggestion) => (
            <Pill
              key={suggestion}
              size="sm"
              mode="action"
              onClick={() => fieldRef.current?.insertText(suggestion)}
            >
              {suggestion}
            </Pill>
          ))}
        </div>
      )}
      <Composer
        submit={
          <ComposerSubmit
            aria-label="Start chat"
            title="Start chat"
            disabled={blocked}
            onClick={send}
          >
            {starting ? <LoaderCircle className="animate-spin" /> : undefined}
          </ComposerSubmit>
        }
      >
        <MentionTextarea
          ref={fieldRef}
          id="chat-page-prompt"
          value={prompt}
          onValueChange={setPrompt}
          users={users}
          onKeyDown={(event) => {
            // Shift+Enter breaks the line; an IME's own Enter is never a send.
            if (event.key !== `Enter` || event.shiftKey) return
            if (event.nativeEvent.isComposing) return
            event.preventDefault()
            send()
          }}
          placeholder="Ask the agent…"
          className="min-h-20 resize-none border-0 bg-transparent px-3 pt-3 shadow-none focus-visible:ring-0"
          // Client parity with the server's per-value cap, so a long paste is
          // refused at the field instead of at submit.
          maxLength={MAX_ACTION_INPUT_TEXT}
        />
      </Composer>
      {candidateDevices.length === 0 ? (
        <p className="px-1 text-xs text-muted-foreground">
          No desktop online. Open the Exponential desktop app to start a chat.
        </p>
      ) : (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-xs text-muted-foreground">
          <InlinePicker
            label="Machine"
            value={launch.device?.deviceId ?? ``}
            options={candidateDevices.map((device) => ({
              value: device.deviceId,
              label: `${device.deviceLabel || device.deviceId}${
                device.owner ? ` — ${device.owner.name}` : ``
              }`,
            }))}
            onChange={launch.setDeviceId}
          />
          <InlinePicker
            label="Agent"
            value={launch.agent}
            options={launch.availableAgents.map((agent) => ({
              value: agent,
              label: AGENT_LABELS[agent] ?? agent,
            }))}
            onChange={launch.switchAgent}
          />
          {agentSupportsPlanMode(launch.agent) && (
            <label className="flex items-center gap-1.5">
              <span>Plan</span>
              <Switch
                size="sm"
                checked={launch.planMode}
                onCheckedChange={launch.setPlanMode}
                aria-label="Plan mode"
              />
            </label>
          )}
          {launch.agentNotReady && (
            <span>
              {`Not ready on ${launch.device!.deviceLabel || launch.device!.deviceId}. Run the doctor there.`}
            </span>
          )}
          {/* The desktop inserts the row when the launcher spins up; the page
              flips to the live view the moment it syncs. */}
          {sentTo && <span>{`Waiting for ${sentTo}…`}</span>}
        </div>
      )}
    </div>
  )
}

/** A picker as one word of the muted line under the prompt box: the current
 * value plus a chevron, no chrome. */
function InlinePicker({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string
  options: { value: string; label: string }[]
  onChange: (value: string) => void
}) {
  if (options.length === 0) return null
  const current = options.find((option) => option.value === value)
  if (options.length === 1) {
    return <span title={label}>{current?.label ?? options[0].label}</span>
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="flex items-center gap-0.5 outline-none hover:text-foreground focus-visible:text-foreground"
        title={label}
        aria-label={label}
      >
        {current?.label ?? label}
        <ChevronDown className="size-3" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {options.map((option) => (
          <DropdownMenuItem
            key={option.value}
            onSelect={() => onChange(option.value)}
          >
            {option.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

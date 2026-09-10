import { useMemo, useRef, useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { ChevronDown, LoaderCircle } from "lucide-react"
import { MAX_ACTION_INPUT_TEXT } from "@exp/db-schema/domain"
import type { User } from "@/db/schema"
import { useSteerConfig } from "@/components/agent-session"
import { AgentShell, SessionsList } from "@/components/agent-shell"
import { Composer, ComposerSubmit } from "@/components/composer"
import { AGENT_LABELS } from "@/components/launch-dialog/launch-options-pane"
import { useLaunchOptions } from "@/components/launch-dialog/use-launch-options"
import { McpServerPicker } from "@/components/launch-dialog/mcp-server-picker"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useNow } from "@/hooks/use-now"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Pill } from "@/components/ui/pill"
import { Switch } from "@/components/ui/switch"
import { conceptIcon } from "@/lib/icons.generated"
import { agentSupportsPlanMode } from "@/lib/coding-launch-prefs"
import { BUILTIN_CHAT_ID, BUILTIN_CHAT_NAME } from "@/lib/builtin-actions"
import { deviceHasRunnableAgent, deviceIsOnline } from "@/lib/steer-devices"
import { useIsMobile } from "@/hooks/use-mobile"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// EXP-818: the team's AGENT page — the sessions list on the left (Running,
// then Past; the sections the Devices page carried) and, on the right, the
// chat prompt (EXP-739/772: a conversation with an agent bound to no issue
// and to no repository — it runs in the agent's scratch directory with the
// Exponential MCP server wired up). Clicking a session opens
// `/t/$teamSlug/sessions/$sessionId`, which renders inside the same shell,
// so the list never goes away. It replaced the `/chat` page, whose
// `?session=` trick for reaching a second running chat is unnecessary now
// that every run has a row here. On phones the page is the list over the
// prompt, and a session is its own screen.

const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/t/$teamSlug/agent`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: AgentPage,
})

function AgentPage() {
  const { teamSlug } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const isMobile = useIsMobile()
  // EXP-790: the prompt box is the mention field, so `@` needs the roster.
  const { users: teamUsers } = useTeamUsers(team?.id)

  const currentUserId = authSession?.user?.id
  // Steer tickets require team membership and a configured relay; the server
  // enforces both at mint time, this only decides what renders.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId: team?.id,
  })

  if (!team || !currentUserId) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  const prompt = (
    <div className="flex h-full min-h-0 flex-col">
      <ChatHeader title={BUILTIN_CHAT_NAME} />
      {/* EXP-772: an essentially empty pane — one centred prompt box with a
          subtle picker row under it. */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-10 px-4 py-6">
          {steerEnabled ? (
            <ChatPrompt
              teamId={team.id}
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
        </div>
      </div>
      {/* The phone: the list under the prompt — the page is both. */}
      {isMobile && (
        <SessionsList
          teamId={team.id}
          currentUserId={currentUserId}
          activeSessionId={null}
          className="shrink-0"
        />
      )}
    </div>
  )

  return (
    <AgentShell teamId={team.id} currentUserId={currentUserId} activeSessionId={null}>
      {prompt}
    </AgentShell>
  )
}

/** The prompt pane's own header — the AgentSessionView draws its own. */
function ChatHeader({ title }: { title: string }) {
  return (
    <div className="flex items-center gap-1 border-b border-border px-3 py-1.5">
      <UiBackIcon className="hidden" aria-hidden />
      <span className="min-w-0 flex-1 truncate text-sm font-medium">{title}</span>
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
  teamId,
  devices,
  starting,
  sentTo,
  users,
  onStart,
}: {
  teamId: string
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
  // EXP-792: the team's MCP servers ride the same picker row as the machine
  // and agent; the pick seeds from the team's defaults / last pick.
  const mcp = useMcpServers(teamId)
  const mcpNow = useNow(30_000)
  // `open: true` — this page IS the launcher, there is no dialog to settle on.
  const launch = useLaunchOptions({
    open: true,
    devices: candidateDevices,
    planModeOff: true,
    teamId,
    mcpServers: mcp.servers,
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
          {mcp.servers && mcp.servers.length > 0 && (
            <McpServerPicker
              servers={mcp.servers}
              selectedIds={launch.mcpServerIds}
              onToggle={launch.toggleMcpServer}
              device={launch.device}
              now={mcpNow}
              renderTrigger={(summary) => (
                <button
                  type="button"
                  className="flex items-center gap-0.5 outline-none hover:text-foreground focus-visible:text-foreground"
                  title="MCP servers"
                  aria-label="MCP servers"
                >
                  {`MCP: ${summary}`}
                  <ChevronDown className="size-3" />
                </button>
              )}
            />
          )}
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

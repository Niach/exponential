// EXP-484: the device-settings "Agents" section — who each agent CLI on this
// machine is signed in as, how much of its rate-limit windows is spent, and
// the Login / Switch account buttons that queue the `agent_login` device
// command.
//
// The machine collects all of this locally and ships it on register/heartbeat
// (it never holds, copies or refreshes a credential); this only renders the
// synced row. Hand-mirrored on iOS (`DeviceSettingsSheet.agentsSection`),
// Android (`DeviceSettingsSheet.kt`) and the desktop IDE
// (`ui/src/device_settings.rs`) — same captions, same gating.
import { useState } from "react"
import { LoaderCircle } from "lucide-react"
import type { Device, DeviceAgentAccount } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import {
  accountRow,
  parseAgentLoginResult,
  parseAgentUsage,
  usageIsFresh,
} from "@/lib/agent-usage"
import {
  readAgentUsageWindow,
  writeAgentUsageWindow,
} from "@/lib/agent-usage-prefs"
import { AgentUsageWindows } from "@/components/agent-usage-bar"
import { relativeTime } from "@/components/comment-rows/format"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"

const SignInIcon = conceptIcon(`ui-sign-in`)
const SwapIcon = conceptIcon(`ui-swap`)
const CopyIcon = conceptIcon(`ui-copy`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)

/** The command key the dialog tracks a per-agent login under. */
export function agentLoginKey(agent: string): string {
  return `login:${agent}`
}

export function DeviceAgentsSection({
  agents,
  row,
  online,
  canAgentLogin,
  now,
  errors,
  isPending,
  results,
  onLogin,
}: {
  /** The agents this machine actually reported (never the full contract). */
  agents: string[]
  row: Device | null
  online: boolean
  /** The machine's build runs the `agent_login` command (caps). */
  canAgentLogin: boolean
  now: Date
  errors: Record<string, string>
  isPending: (key: string) => boolean
  results: Record<string, string>
  /** Queue a login; the dialog owns the Codex switch confirmation. */
  onLogin: (agent: string, switchAccount: boolean) => void
}) {
  if (agents.length === 0) return null
  return (
    <div className="space-y-2">
      <Label>Agents</Label>
      {agents.map((agent) => (
        <AgentAccountRow
          key={agent}
          agent={agent}
          account={row?.agentAccounts?.[agent] ?? null}
          usage={parseAgentUsage(row?.agentUsage?.[agent])}
          agentUsageAt={row?.agentUsageAt ?? null}
          online={online}
          canAgentLogin={canAgentLogin}
          now={now}
          error={errors[agentLoginKey(agent)] ?? ``}
          pending={isPending(agentLoginKey(agent))}
          result={results[agentLoginKey(agent)] ?? null}
          onLogin={onLogin}
        />
      ))}
    </div>
  )
}

function AgentAccountRow({
  agent,
  account,
  usage,
  agentUsageAt,
  online,
  canAgentLogin,
  now,
  error,
  pending,
  result,
  onLogin,
}: {
  agent: string
  account: DeviceAgentAccount | null
  usage: ReturnType<typeof parseAgentUsage>
  agentUsageAt: Date | null
  online: boolean
  canAgentLogin: boolean
  now: Date
  error: string
  pending: boolean
  result: string | null
  onLogin: (agent: string, switchAccount: boolean) => void
}) {
  const [selectedKey, setSelectedKey] = useState(() =>
    readAgentUsageWindow(agent)
  )
  // pi's sign-in is an interactive prompt with no device-code flow to hand
  // back — local only, and the server refuses the command outright.
  const canLogin = online && canAgentLogin && agent !== `pi`
  const signedIn = account?.signedIn === true
  const fresh = usageIsFresh(usage, now)
  const asOf = account?.checkedAt ?? agentUsageAt

  return (
    <div className="space-y-1 border-b border-border/30 py-1.5 last:border-b-0">
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {accountRow(agent, account)}
        </span>
        {canLogin && (
          <Button
            variant="outline"
            size="sm"
            className="h-6 shrink-0 gap-1 px-2 text-xs"
            disabled={pending}
            onClick={() => onLogin(agent, signedIn)}
          >
            {pending ? (
              <LoaderCircle className="size-3 animate-spin" />
            ) : signedIn ? (
              <SwapIcon className="size-3" />
            ) : (
              <SignInIcon className="size-3" />
            )}
            {signedIn ? `Switch account` : `Login`}
          </Button>
        )}
      </div>
      {fresh && usage && (
        <AgentUsageWindows
          agent={agent}
          usage={usage}
          now={now}
          selectedKey={selectedKey}
          onSelect={(key) => {
            setSelectedKey(key)
            writeAgentUsageWindow(agent, key)
          }}
        />
      )}
      {(!online || !canAgentLogin) && asOf && (
        <p className="text-[11px] text-muted-foreground">
          as of {relativeTime(asOf)}
        </p>
      )}
      {pending && (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          {online
            ? `Waiting for the sign-in link…`
            : `This machine is offline — the sign-in runs when it comes online.`}
        </p>
      )}
      {result && <AgentLoginOutcome result={result} />}
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  )
}

/** What a finished login command hands back: the CLI's own sign-in URL (open
 * it anywhere) plus, for Codex's device-code flow, the code to type on the
 * machine. Anything unparsable renders as the raw text the device sent. */
function AgentLoginOutcome({ result }: { result: string }) {
  const progress = parseAgentLoginResult(result)
  if (progress?.phase === `failed`) {
    return (
      <p className="text-xs text-destructive">
        {progress.message ?? `The machine reported a failure.`}
      </p>
    )
  }
  if (!progress?.url) {
    return <p className="text-xs text-muted-foreground">{result}</p>
  }
  const code = progress.code
  return (
    <div className="space-y-1">
      <a
        href={progress.url}
        target="_blank"
        rel="noreferrer"
        className="flex items-center gap-1 text-xs text-primary underline underline-offset-2"
      >
        <ExternalLinkIcon className="size-3 shrink-0" />
        <span className="min-w-0 truncate">{progress.url}</span>
      </a>
      {code && (
        <div className="flex items-center gap-1.5">
          <span className="font-mono text-xs">{code}</span>
          <Button
            variant="ghost"
            className="h-5 w-5 p-0 text-muted-foreground"
            aria-label="Copy code"
            title="Copy code"
            onClick={() => {
              void navigator.clipboard?.writeText(code)
            }}
          >
            <CopyIcon className="size-3" />
          </Button>
        </div>
      )}
      <p className="text-[11px] text-muted-foreground">
        Open the link on any device
        {code ? ` and enter the code on the machine.` : `.`}
      </p>
    </div>
  )
}

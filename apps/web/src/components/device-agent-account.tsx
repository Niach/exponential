// EXP-484/688: the account block inside ONE agent's tab of device settings —
// who that CLI is signed in as on this machine, how much of its rate-limit
// windows is spent, and the Login / Switch account button that queues the
// `agent_login` device command.
//
// The machine collects all of this locally and ships it on register/heartbeat
// (it never holds, copies or refreshes a credential); this only renders the
// synced row. EXP-688 moved it out of a standalone "Agents" section and under
// each agent's own defaults, so the tab you are editing is the tab that tells
// you whose account it runs as. Hand-mirrored on iOS
// (`DeviceSettingsSheet`), Android (`DeviceSettingsSheet.kt`) and the desktop
// IDE (`ui/src/device_settings.rs`) — same captions, same gating.
import { LoaderCircle } from "lucide-react"
import type { Device, DeviceAgentAccount } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import {
  accountLine,
  parseAgentLoginResult,
  parseAgentUsage,
  usageIsFresh,
} from "@/lib/agent-usage"
import { AgentUsageCards } from "@/components/agent-usage-bar"
import { relativeTime } from "@/components/comment-rows/format"
import { Button } from "@/components/ui/button"

const SignInIcon = conceptIcon(`ui-sign-in`)
const SwapIcon = conceptIcon(`ui-swap`)
const CopyIcon = conceptIcon(`ui-copy`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)

/** The command key the dialog tracks a per-agent login under. */
export function agentLoginKey(agent: string): string {
  return `login:${agent}`
}

export function AgentAccountBlock({
  agent,
  row,
  online,
  canAgentLogin,
  now,
  error,
  pending,
  result,
  onLogin,
}: {
  agent: string
  row: Device | null
  online: boolean
  /** The machine's build runs the `agent_login` command (caps). */
  canAgentLogin: boolean
  now: Date
  error: string
  pending: boolean
  result: string | null
  /** Queue a login; the dialog owns the Codex switch confirmation. */
  onLogin: (agent: string, switchAccount: boolean) => void
}) {
  const account: DeviceAgentAccount | null = row?.agentAccounts?.[agent] ?? null
  const usage = parseAgentUsage(row?.agentUsage?.[agent])
  // pi's sign-in is an interactive prompt with no device-code flow to hand
  // back — local only, and the server refuses the command outright.
  const canLogin = online && canAgentLogin && agent !== `pi`
  const signedIn = account?.signedIn === true
  const fresh = usageIsFresh(usage, now)
  const asOf = account?.checkedAt ?? row?.agentUsageAt ?? null

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {accountLine(account)}
        </span>
        {canLogin && (
          <Button
            variant="glass"
            size="sm"
            className="shrink-0"
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
      {/* Fresh numbers become cards; anything older (or an account with no
          usage at all, like pi) says how old the report is — the same
          fall-through the desktop/iOS/Android sheets use. */}
      {fresh && usage && usage.windows.length > 0 ? (
        <AgentUsageCards usage={usage} now={now} compact />
      ) : (
        (usage || account) &&
        asOf && (
          <p className="text-[11px] text-muted-foreground">
            as of {relativeTime(asOf)}
          </p>
        )
      )}
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

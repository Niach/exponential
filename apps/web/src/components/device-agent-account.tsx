// EXP-484/688: the account block inside ONE agent's tab of device settings —
// who that CLI is signed in as on this machine, how much of its rate-limit
// windows is spent, and the Login / Switch account button that queues the
// `agent_login` device command.
//
// The machine collects all of this locally and ships it on register/heartbeat
// (it never holds, copies or refreshes a credential); this only renders the
// synced row. EXP-688 moved it out of a standalone "Agents" section and under
// each agent's own defaults, so the tab you are editing is the tab that tells
// you whose account it runs as. EXP-694 folds it INTO that tab's glass group
// as its closing rows (the account line is the bare email now, the usage
// windows are flat rows). Hand-mirrored on iOS (`DeviceSettingsSheet`),
// Android (`DeviceSettingsSheet.kt`) and the desktop IDE
// (`ui/src/device_settings.rs`) — same captions, same gating.
//
// EXP-765: claude's sign-in link carries `code=true` — the browser page ends
// by showing an authorization CODE the CLI on the machine is still waiting
// for. The code field under the link hands it back as an `agent_login_code`
// command (its own cap: a build that only runs the login would report the
// command unsupported, so the field stays hidden without it).
import { useState } from "react"
import { LoaderCircle } from "lucide-react"
import type { Device, DeviceAgentAccount } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import {
  accountLine,
  parseAgentLoginResult,
  parseAgentUsage,
} from "@/lib/agent-usage"
import { relativeTime } from "@/components/comment-rows/format"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

const SignInIcon = conceptIcon(`ui-sign-in`)
const SwapIcon = conceptIcon(`ui-swap`)
const CopyIcon = conceptIcon(`ui-copy`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)
const UsageIcon = conceptIcon(`ui-usage`)

/** The command key the dialog tracks a per-agent login under. */
export function agentLoginKey(agent: string): string {
  return `login:${agent}`
}

/** EXP-765: the key the dialog tracks a per-agent `agent_login_code` under. */
export function agentLoginCodeKey(agent: string): string {
  return `login-code:${agent}`
}

const LOGIN_CODE_PREFIX = `login-code:`

/** The agent behind a `login-code:` key, or null for any other key. */
export function agentOfLoginCodeKey(key: string): string | null {
  return key.startsWith(LOGIN_CODE_PREFIX)
    ? key.slice(LOGIN_CODE_PREFIX.length)
    : null
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
  onOpenUsage,
  codeError,
  codePending,
  codeResult,
  onEnterCode,
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
  /** EXP-827: open the Devices page's Accounts section (the usage lives
   *  there now, not inline under the account line). Absent = no button. */
  onOpenUsage?: () => void
  codeError: string
  codePending: boolean
  codeResult: string | null
  /** Hand the code the browser showed back to the waiting login. */
  onEnterCode: (agent: string, code: string) => void
}) {
  const account: DeviceAgentAccount | null = row?.agentAccounts?.[agent] ?? null
  const usage = parseAgentUsage(row?.agentUsage?.[agent])
  // EXP-849: every remaining agent has a device-code flow.
  const canLogin = online && canAgentLogin
  const signedIn = account?.signedIn === true
  const asOf = account?.checkedAt ?? row?.agentUsageAt ?? null
  // EXP-827: the usage windows moved to the Devices page's Accounts
  // section; the block only says how old the account report is and links
  // there. `now` still drives the caption's relative time.
  void now
  const hasUsage = (usage?.windows.length ?? 0) > 0

  return (
    // EXP-694: the FINAL ROWS of the agent's own glass group — the row rhythm
    // (16h/12v) is ours, the divider above comes from the group.
    <div className="flex flex-col gap-2 px-4 py-3">
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
        {onOpenUsage && (account || hasUsage) && (
          <Button
            variant="glass"
            size="icon-sm"
            className="shrink-0 rounded-full"
            aria-label="Usage"
            title="Usage"
            onClick={onOpenUsage}
          >
            <UsageIcon />
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
      {result && (
        <AgentLoginOutcome
          result={result}
          codePending={codePending}
          onEnterCode={(code) => onEnterCode(agent, code)}
        />
      )}
      {error && <p className="text-xs text-destructive">{error}</p>}
      {codePending && (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          Sending the code to the machine…
        </p>
      )}
      {codeResult && (
        <p className="text-xs text-muted-foreground">{codeResult}</p>
      )}
      {codeError && <p className="text-xs text-destructive">{codeError}</p>}
      {/* EXP-827: no inline usage cards any more (the Usage button above
          opens the Accounts section); the report age stays as a caption. */}
      {(usage || account) && asOf && (
        <p className="text-[11px] text-muted-foreground">
          as of {relativeTime(asOf)}
        </p>
      )}
    </div>
  )
}

/** What a finished login command hands back: the CLI's own sign-in URL (open
 * it anywhere) plus, for Codex's device-code flow, the code to type on the
 * machine. A link WITHOUT a code is claude's: the browser hands one back
 * instead, and (EXP-765) the field below the link returns it to the machine.
 * Anything unparsable renders as the raw text the device sent. */
export function AgentLoginOutcome({
  result,
  codePending,
  onEnterCode,
}: {
  result: string
  codePending: boolean
  onEnterCode: (code: string) => void
}) {
  const [draft, setDraft] = useState(``)
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
  const wantsCodeBack = !code
  const submit = () => {
    const trimmed = draft.trim()
    if (!trimmed || codePending) return
    onEnterCode(trimmed)
    setDraft(``)
  }
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
      {wantsCodeBack && (
        <div className="flex items-center gap-1.5">
          <Input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === `Enter`) {
                event.preventDefault()
                submit()
              }
            }}
            placeholder="Code from the browser"
            aria-label="Code from the browser"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="h-7 font-mono text-xs"
          />
          <Button
            variant="glass"
            size="sm"
            className="shrink-0"
            disabled={!draft.trim() || codePending}
            onClick={submit}
          >
            <SignInIcon className="size-3" />
            Enter code
          </Button>
        </div>
      )}
      <p className="text-[11px] text-muted-foreground">
        {code
          ? `Open the link on any device and enter the code on the machine.`
          : wantsCodeBack
            ? `Open the link on any device, then paste the code it shows here.`
            : `Open the link on any device.`}
      </p>
    </div>
  )
}

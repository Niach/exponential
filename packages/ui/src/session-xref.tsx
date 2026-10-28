import { THREAD_COPY, THREAD_TEST_IDS, type SessionXref as SessionXrefModel } from "@exp/domain-contract/session-tree"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { UserAvatar, type UserAvatarAgent } from "./user-avatar"

// S-3 inter-session links ×4 (`session-thread.json` `copy.xref`,
// `testIds.xref`; styleguide entry `session-xref`): a row that came from
// ANOTHER session says so in ONE compact line = the muted lead
// (`Message from` · `Report from` · `Started by` · `Reported to`), the
// session's chip (the agent's avatar at 16 when `agentId` is set, else the
// session glyph; its name; a trailing chevron). A tap opens that session
// (the host pushes it on its back stack).

const SessionIcon = conceptIcon(`coding-assistant`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

export function SessionXref({
  xref,
  agent = null,
  onOpen,
  className,
}: {
  xref: SessionXrefModel
  /** The mark of `xref.agentId` (its icon + colour); null = the session
   *  glyph (no agent, or its row has not synced yet). */
  agent?: UserAvatarAgent | null
  onOpen?: (sessionId: string) => void
  className?: string
}) {
  const lead = (THREAD_COPY.xref as Record<SessionXrefModel[`kind`], string>)[xref.kind]
  const content = (
    <>
      <span className="shrink-0">{lead}</span>
      {xref.agentId && agent ? (
        <UserAvatar user={null} agent={agent} size={16} />
      ) : (
        <SessionIcon className="size-4 shrink-0" />
      )}
      <span className="min-w-0 truncate font-medium text-foreground">{xref.name}</span>
      <ChevronRightIcon className="size-3.5 shrink-0" />
    </>
  )
  const base = cn(
    `flex min-w-0 max-w-full items-center gap-1 self-start text-xs text-muted-foreground`,
    className
  )
  if (!onOpen) {
    return (
      <div data-slot={THREAD_TEST_IDS.xref} data-testid={THREAD_TEST_IDS.xref} data-kind={xref.kind} className={base}>
        {content}
      </div>
    )
  }
  return (
    <button
      type="button"
      data-slot={THREAD_TEST_IDS.xref}
      data-testid={THREAD_TEST_IDS.xref}
      data-kind={xref.kind}
      aria-label={`${lead} ${xref.name}`}
      className={cn(base, `text-left outline-none hover:text-foreground focus-visible:text-foreground`)}
      onClick={() => onOpen(xref.sessionId)}
    >
      {content}
    </button>
  )
}

/** S-3 (DEV-ONLY view `session-xref`, the view catalog's capture): the four
 *  kinds, two through an agent's avatar, two through the session glyph,
 *  case for case with the styleguide entry and the natives' specimens. */
export function SessionXrefSpecimen() {
  const agent = { icon: `bot`, color: `#8b5cf6` }
  const row = (kind: SessionXrefModel[`kind`], name: string, withAgent: boolean) => ({
    xref: { kind, sessionId: `sg-xref-${kind}`, name, agentId: withAgent ? `sg-agent` : null },
    agent: withAgent ? agent : null,
  })
  return (
    <div data-testid="session-xref-specimen" className="flex w-[22rem] max-w-full flex-col items-start gap-3 rounded-3xl border border-glass-stroke-card bg-background p-4">
      {[
        row(`message`, `Triage`, true),
        row(`report`, `Fix the sign-in redirect`, false),
        row(`started`, `Triage`, true),
        row(`reported`, `Ship the login flow`, false),
      ].map(({ xref, agent }) => (
        <SessionXref key={xref.kind} xref={xref} agent={agent} onOpen={() => {}} />
      ))}
    </div>
  )
}

import type { Issue } from "@/db/schema"
import type { ComposerAgent, ComposerPrompt } from "@/hooks/use-launch-composer"
import { AgentIcon } from "@/components/team/sidebar-agents"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { IssueChip } from "@/components/issue-chip"
import { ChipRemoveButton, Pill, Spinner, conceptIcon, getActionIcon } from "@exp/ui"

// EXP-825: the composer's SUBJECT, as chips in the card's leading row — one
// per picked issue, ONE prompt chip (NEW-2, picked in the `/` menu) or ONE
// agent chip (NEW-3, Message an agent › in the "+"). They are how "exclusivity by swap"
// reads: picking the other kind replaces what is here, and the chips show it.
//
// EXP-827/EXP-885: an issue chip IS `<IssueChip>` — the one chip the session
// feed, the timeline and the markdown editor draw (small rounded rect, status
// glyph, mono identifier, muted title), here with its trailing ✕; the body
// opens the issue where an issue-ref provider is mounted and is inert
// otherwise. The prompt chip's body never dismisses the prompt — only its ✕
// does (the last issue chip going = a chat again).
// Test ids are byte-identical with the native suites
// (`agent-composer-chip-issue-<IDENT>` + `-remove`,
// `agent-composer-chip-prompt` + `-remove`).

const IssueRefIcon = conceptIcon(`editor-issue-ref`)

export function SubjectChips({
  issues,
  pendingIssueCount,
  prompt,
  promptPending,
  agent,
  agentPending,
  onRemoveIssue,
  onClearPrompt,
  onClearAgent,
  disabled,
}: {
  /** The checked issues that have synced rows. */
  issues: Issue[]
  /** Checked ids whose rows have not synced yet (a fresh deep link). */
  pendingIssueCount: number
  /** The subject prompt's row, when the subject is a prompt. */
  prompt: ComposerPrompt | null
  /** The subject is a prompt whose row is not synced (or gone). */
  promptPending: boolean
  /** NEW-3: the subject agent's row, when the subject is an agent (a DM). */
  agent?: ComposerAgent | null
  /** The subject is an agent whose row is not synced (or gone). */
  agentPending?: boolean
  onRemoveIssue: (issueId: string) => void
  onClearPrompt: () => void
  onClearAgent?: () => void
  disabled?: boolean
}) {
  const issueRefs = useIssueRefs()

  // NEW-3: ONE agent chip, the agent's icon in its colour + its name: the
  // message goes into the caller's DM with it (`sessions.send({agentId})`).
  if (agent || agentPending) {
    return (
      <Pill
        size="sm"
        mode="readonly"
        className="max-w-[20rem] pr-1"
        data-testid="agent-composer-chip-agent"
        title={agent?.name}
      >
        {agent ? <AgentIcon agent={agent} className="size-3" /> : <Spinner />}
        <span className="min-w-0 truncate">{agent?.name ?? `Agent…`}</span>
        <ChipRemoveButton
          label={`Remove ${agent?.name ?? `agent`}`}
          testId="agent-composer-chip-agent-remove"
          disabled={disabled}
          onRemove={() => onClearAgent?.()}
        />
      </Pill>
    )
  }

  if (prompt || promptPending) {
    const RowIcon = prompt ? getActionIcon(prompt) : IssueRefIcon
    const name = prompt?.name ?? `prompt`
    return (
      <Pill
        size="sm"
        mode="readonly"
        className="max-w-[20rem] pr-1"
        data-testid="agent-composer-chip-prompt"
        title={prompt?.name}
      >
        <RowIcon className="size-3 shrink-0" />
        <span className="min-w-0 truncate">{prompt?.name ?? `Prompt…`}</span>
        <ChipRemoveButton
          label={`Remove ${name}`}
          testId="agent-composer-chip-prompt-remove"
          disabled={disabled}
          onRemove={onClearPrompt}
        />
      </Pill>
    )
  }
  return (
    <>
      {issues.map((issue) => (
        <IssueChip
          key={issue.id}
          issue={issue}
          className="max-w-[20rem]"
          // The composer is a typing surface — a hover card popping over the
          // field while picking issues is noise.
          preview={false}
          onClick={
            issueRefs ? () => issueRefs.open(issue.identifier) : undefined
          }
          onRemove={() => onRemoveIssue(issue.id)}
          removeDisabled={disabled}
          testId={`agent-composer-chip-issue-${issue.identifier}`}
          removeTestId={`agent-composer-chip-issue-${issue.identifier}-remove`}
        />
      ))}
      {pendingIssueCount > 0 && (
        // Issues not synced yet: the spinner + a count, no "loading" words
        // (EXP-1259 D8).
        <Pill size="sm" aria-busy leading={<Spinner />}>
          {`${pendingIssueCount} more`}
        </Pill>
      )}
    </>
  )
}

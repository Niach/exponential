/* ─── The ONE unified Start-coding dialog — its own 760×540 native window
   (EXP-268/285/635): the Issues | Actions | Chat subject strip over two
   columns (subject picker left, Device + agent defaults right) and a pinned
   action bar carrying the blocker message.

   The Issues picker is always a searchable multi-issue checklist (EXP-106),
   scoped to the board's OPEN issues plus whatever is already checked (checked
   rows pin FIRST): 1 checked issue → a plain session on exp/<IDENTIFIER>,
   2+ → ONE batch session on ONE exp/batch-<id8> branch ending in ONE combined
   PR. The right column is the shared AgentDefaultsGroup (launch_options.rs):
   ONE hairline-divided glass group of the agent tabs, Model, the agent's
   effort concept and the capability-gated SWITCHES — no checkboxes, no loose
   controls. Defaults are per AGENT (EXP-206), not per mode. ─── */
import { useMemo, useState } from "react"
import { ISSUES, STATUS_LABEL } from "./data"
import { useIde, type CodingTarget } from "./state"
import {
  IcCheck,
  IcChevDown,
  IcFlask,
  IcGitBranch,
  IcPackage,
  IcSparkles,
  type IdeIcon,
} from "./icons"
import { ClaudeLogo, CodexLogo, PiLogo } from "../components/agent-icons"

type AgentId = `claude` | `codex` | `pi`

/* coding_selects.rs AGENT_CHOICES / model_choices_for / effort_choices_for,
   plus CodingAgent's capability gates and effort_label(). */
type AgentSpec = {
  id: AgentId
  label: string
  Logo: (props: { size?: number }) => React.ReactElement
  models: string[]
  effortLabel: string
  efforts: string[]
  ultracode: boolean
  planMode: boolean
}

const AGENTS: AgentSpec[] = [
  {
    id: `claude`,
    label: `Claude Code`,
    Logo: ClaudeLogo,
    /* Claude is explicit-always — no "CLI default" row. */
    models: [`Fable`, `Opus`, `Sonnet`],
    effortLabel: `Effort`,
    efforts: [`CLI default`, `Low`, `Medium`, `High`, `XHigh`, `Max`],
    ultracode: true,
    planMode: true,
  },
  {
    id: `codex`,
    label: `Codex`,
    Logo: CodexLogo,
    models: [`CLI default`, `GPT-5.6 Sol`, `GPT-5.6 Terra`, `GPT-5.6 Luna`],
    effortLabel: `Reasoning`,
    efforts: [`CLI default`, `Minimal`, `Low`, `Medium`, `High`, `XHigh`],
    ultracode: false,
    /* Codex has an interactive /plan but no flag to launch into it. */
    planMode: false,
  },
  {
    id: `pi`,
    label: `pi`,
    Logo: PiLogo,
    models: [
      `CLI default`,
      `Fable`,
      `Opus`,
      `Sonnet`,
      `GPT-5.6 Sol`,
      `GPT-5.6 Terra`,
      `GPT-5.6 Luna`,
      `Grok 4.5`,
    ],
    effortLabel: `Thinking`,
    efforts: [
      `CLI default`,
      `Off`,
      `Minimal`,
      `Low`,
      `Medium`,
      `High`,
      `XHigh`,
      `Max`,
    ],
    ultracode: false,
    planMode: true,
  },
]

const SUBJECTS = [`Issues`, `Actions`, `Chat`] as const
type Subject = (typeof SUBJECTS)[number]

/* The Actions tab lists the team's actions with the two virtual builtins
   pinned FIRST — name/description/icon are the shipped constants
   (apps/web/src/lib/builtin-actions.ts); the rest are team rows seeded from
   the curated suggestions (action-suggestions.ts). */
type DemoAction = {
  name: string
  description: string
  Icon: IdeIcon
  /* One required input, exactly like the builtin's `pr`. */
  input?: { label: string; placeholder: string }
}

/* "Create action" is deliberately absent — the Actions TAB filters it out
   (start_coding_dialog.rs); authoring lives in the create-action dialog. */
const ACTIONS: DemoAction[] = [
  {
    name: `Fix merge conflicts`,
    description: `Pick a conflicted pull request and let your agent rebase, resolve, and merge it`,
    Icon: IcGitBranch,
    input: { label: `Pull request`, placeholder: `Select pull request…` },
  },
  {
    name: `Update dependencies`,
    description: `Bump every package to the latest compatible release and open a PR.`,
    Icon: IcPackage,
  },
  {
    name: `Nightly test triage`,
    description: `Investigate failing or flaky tests and file issues for real bugs.`,
    Icon: IcFlask,
  },
  {
    name: `Draft release notes`,
    description: `Summarize merged PRs since the last release into user-facing notes.`,
    Icon: IcSparkles,
  },
]

const REPO = `acme/mobile-app`

/* One hairline-divided row of a glass group (surface::glass_group_rows). */
function GroupRow({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <div className="ide-grouprow">
      <span className="ide-grouprow-label">{label}</span>
      <div className="ide-flex1" />
      {children}
    </div>
  )
}

function PickerRow({
  label,
  value,
  onCycle,
  disabled,
}: {
  label: string
  value: string
  onCycle?: () => void
  disabled?: boolean
}) {
  const { interactive } = useIde()
  return (
    <GroupRow label={label}>
      <button
        className={`ide-grouprow-select${interactive && !disabled ? ` is-click` : ``}${disabled ? ` is-disabled` : ``}`}
        type="button"
        disabled={disabled}
        onClick={interactive && !disabled ? onCycle : undefined}
      >
        <span>{value}</span>
        <IcChevDown size={10} className="ide-c-muted" />
      </button>
    </GroupRow>
  )
}

/* EXP-694/698: a Switch on the group's row rhythm — never a checkbox. */
function ToggleRow({
  label,
  on,
  onToggle,
}: {
  label: string
  on: boolean
  onToggle: () => void
}) {
  const { interactive } = useIde()
  return (
    <GroupRow label={label}>
      <button
        className={`ide-switch${on ? ` is-on` : ``}${interactive ? ` is-click` : ``}`}
        type="button"
        role="switch"
        aria-checked={on}
        aria-label={label}
        onClick={interactive ? onToggle : undefined}
      >
        <span className="ide-switch-knob" />
      </button>
    </GroupRow>
  )
}

function LabeledField({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <div className="ide-dlg-labeled">
      <span className="ide-dlg-fieldlabel">{label}</span>
      {children}
    </div>
  )
}

/* Pre-seeded ids from the Start-coding button (or the bulk bar) — checked
   from the start and exempt from the open-only/search filters. */
const seededIds = (target: CodingTarget): string[] =>
  target.kind === `issue` ? [target.id] : target.issueIds

export function StartCodingDialog() {
  const { pendingCoding, cancelStartCoding, confirmStartCoding, interactive } = useIde()
  const seeded = useMemo(
    () => new Set(pendingCoding ? seededIds(pendingCoding) : []),
    [pendingCoding],
  )
  const [checked, setChecked] = useState<Set<string>>(() => new Set(seeded))
  const [query, setQuery] = useState(``)
  const [actionQuery, setActionQuery] = useState(``)
  const [subject, setSubject] = useState<Subject>(`Issues`)
  const [agentIx, setAgentIx] = useState(0)
  const [model, setModel] = useState(0)
  const [effort, setEffort] = useState(0)
  const [planMode, setPlanMode] = useState(true)
  const [ultracode, setUltracode] = useState(false)
  const [actionIx, setActionIx] = useState(0)
  const [prompt, setPrompt] = useState(``)

  const agent = AGENTS[agentIx]

  /* The picker's pool is the board's OPEN issues; a seeded/checked closed one
     stays listed with its status caption. Checked rows pin FIRST. */
  const { checkedRows, matchRows, noMatches } = useMemo(() => {
    const q = query.trim().toLowerCase()
    const pool = ISSUES.filter(
      (issue) => issue.status !== `done` || seeded.has(issue.id) || checked.has(issue.id),
    )
    const checkedRows = pool.filter((issue) => checked.has(issue.id))
    const matchRows = pool.filter(
      (issue) =>
        !checked.has(issue.id) &&
        (q.length === 0 || `${issue.id} ${issue.title}`.toLowerCase().includes(q)),
    )
    return {
      checkedRows,
      matchRows,
      noMatches: q.length > 0 && matchRows.length === 0 && pool.length > 0,
    }
  }, [query, seeded, checked])

  const actionRows = useMemo(() => {
    const q = actionQuery.trim().toLowerCase()
    return ACTIONS.filter((a) => q.length === 0 || a.name.toLowerCase().includes(q))
  }, [actionQuery])

  if (!pendingCoding) return null

  const action = ACTIONS[actionIx]

  /* One blocker message at a time, on the action bar's left (start_coding_dialog.rs). */
  const blocker =
    subject === `Issues`
      ? checked.size === 0
        ? `Select at least one issue.`
        : null
      : subject === `Actions`
        ? action.input
          ? `Fill in ${action.input.label}.`
          : null
        : prompt.trim().length === 0
          ? `Fill in Prompt.`
          : null

  const primaryLabel =
    subject === `Chat` ? `Start chat` : subject === `Actions` ? `Run action` : `Start coding`

  const toggle = (id: string) =>
    setChecked((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      return next
    })

  const start = () => {
    /* Stable board order, like the real launcher's prompt sections. */
    const ids = ISSUES.filter((i) => checked.has(i.id)).map((i) => i.id)
    const target: CodingTarget =
      ids.length === 1 ? { kind: `issue`, id: ids[0] } : { kind: `batch`, issueIds: ids }
    confirmStartCoding(target)
  }

  const issueRow = (issue: (typeof ISSUES)[number]) => {
    const on = checked.has(issue.id)
    return (
      <div
        key={issue.id}
        className={`ide-dlg-issue${interactive ? ` is-click` : ``}`}
        onClick={interactive ? () => toggle(issue.id) : undefined}
      >
        <span className={`ide-checkbox${on ? ` is-on` : ``}`}>
          {on && <IcCheck size={9} />}
        </span>
        <span className="ide-dlg-issue-id">{issue.id}</span>
        <span className="ide-dlg-issue-title">{issue.title}</span>
        {issue.status === `done` && (
          <span className="ide-dlg-issue-note">
            {STATUS_LABEL[issue.status].toLowerCase()}
          </span>
        )}
      </div>
    )
  }

  return (
    <div className="ide-dlg-backdrop">
      <div className="ide-dlg">
        <div className="ide-dlg-titlebar">
          <span className="ide-lights is-dialog">
            <i style={{ background: `#ff5f57` }} />
            <i style={{ background: `#4a4a4c` }} />
            <i style={{ background: `#28c840` }} />
          </span>
          <span className="ide-dlg-title">Start coding</span>
        </div>
        <div className="ide-dlg-body">
          <div className="ide-segmented">
            {SUBJECTS.map((label) => (
              <button
                key={label}
                type="button"
                className={`ide-segment${label === subject ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => setSubject(label) : undefined}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="ide-dlg-cols">
            <div className="ide-dlg-left">
              {subject === `Issues` && (
                <>
                  <input
                    className="ide-dlg-search"
                    placeholder="Search issues…"
                    value={query}
                    readOnly={!interactive}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                  <div className="ide-dlg-issues">
                    {checkedRows.map(issueRow)}
                    {matchRows.map(issueRow)}
                    {noMatches && (
                      <div className="ide-dlg-noresults">
                        No matches. Only open issues from this board are shown.
                      </div>
                    )}
                  </div>
                </>
              )}
              {subject === `Actions` && (
                <>
                  <input
                    className="ide-dlg-search"
                    placeholder="Search actions…"
                    value={actionQuery}
                    readOnly={!interactive}
                    onChange={(e) => setActionQuery(e.target.value)}
                  />
                  <div className="ide-dlg-actionlist">
                    {actionRows.length === 0 ? (
                      <div className="ide-dlg-noresults">No matching actions.</div>
                    ) : (
                      actionRows.map((a) => {
                        const ix = ACTIONS.indexOf(a)
                        const on = ix === actionIx
                        return (
                          <div
                            key={a.name}
                            /* EXP-721: the SELECTION marker leads the row. */
                            className={`ide-dlg-action${interactive ? ` is-click` : ``}`}
                            onClick={interactive ? () => setActionIx(ix) : undefined}
                          >
                            <span className={`ide-radio${on ? ` is-on` : ``}`} />
                            <a.Icon size={10} className="ide-c-muted" />
                            <div className="ide-dlg-action-main">
                              <span className="ide-dlg-action-name">{a.name}</span>
                              <span className="ide-dlg-action-desc">{a.description}</span>
                            </div>
                          </div>
                        )
                      })
                    )}
                  </div>
                  {action.input && (
                    <LabeledField label={action.input.label}>
                      <button className="ide-dlg-bigselect" type="button">
                        {action.input.placeholder}
                      </button>
                    </LabeledField>
                  )}
                </>
              )}
              {subject === `Chat` && (
                <>
                  {/* EXP-615: both fields come from the hidden Chat builtin's
                      own input definitions, so their labels and the prompt
                      placeholder cannot drift from the other three clients. */}
                  <LabeledField label="Prompt">
                    <textarea
                      className="ide-dlg-textarea"
                      placeholder="What should the agent do?"
                      value={prompt}
                      readOnly={!interactive}
                      onChange={(e) => setPrompt(e.target.value)}
                    />
                  </LabeledField>
                  <LabeledField label="Repository">
                    <button className="ide-dlg-bigselect" type="button">
                      {REPO}
                    </button>
                  </LabeledField>
                </>
              )}
            </div>
            <div className="ide-dlg-right">
              {/* The device the run lands on — its own glass group. */}
              <div className="ide-group-card">
                <PickerRow label="Device" value="Danny's MacBook Pro" />
              </div>
              {/* The shared AgentDefaultsGroup: tabs, Model, effort, toggles. */}
              <div className="ide-group-card">
                <div className="ide-agentstrip">
                  {AGENTS.map((a, i) => (
                    <button
                      key={a.id}
                      type="button"
                      className={`ide-agenttab${i === agentIx ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                      onClick={
                        interactive
                          ? () => {
                              /* Switching the agent RE-SEEDS its defaults. */
                              setAgentIx(i)
                              setModel(0)
                              setEffort(0)
                            }
                          : undefined
                      }
                    >
                      <a.Logo size={11} />
                      {a.label}
                    </button>
                  ))}
                </div>
                <PickerRow
                  label="Model"
                  value={agent.models[model % agent.models.length]}
                  onCycle={() => setModel((i) => (i + 1) % agent.models.length)}
                />
                <PickerRow
                  label={agent.effortLabel}
                  value={agent.efforts[effort % agent.efforts.length]}
                  onCycle={() => setEffort((i) => (i + 1) % agent.efforts.length)}
                  /* Ultracode IS the effort level while it is on (EXP-206). */
                  disabled={agent.ultracode && ultracode}
                />
                {agent.ultracode && (
                  <ToggleRow
                    label="Ultracode"
                    on={ultracode}
                    onToggle={() => setUltracode((v) => !v)}
                  />
                )}
                {agent.planMode && (
                  <ToggleRow
                    label="Plan mode"
                    on={planMode}
                    onToggle={() => setPlanMode((v) => !v)}
                  />
                )}
              </div>
              {subject === `Issues` && checked.size >= 4 && (
                <div className="ide-dlg-hint">Large batches can be token-expensive.</div>
              )}
            </div>
          </div>
        </div>
        <div className="ide-dlg-actions">
          {blocker && <span className="ide-dlg-blocker">{blocker}</span>}
          <div className="ide-flex1" />
          <button
            className={`ide-btn-outline${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? cancelStartCoding : undefined}
          >
            Cancel
          </button>
          <button
            className={`ide-btn-primary${interactive && !blocker ? ` is-click` : ``}`}
            type="button"
            disabled={Boolean(blocker)}
            onClick={interactive && !blocker && subject === `Issues` ? start : undefined}
          >
            {primaryLabel}
          </button>
        </div>
      </div>
    </div>
  )
}

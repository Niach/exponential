/* ─── The ONE unified Start-coding dialog — its own 760×540 native window
   (EXP-268/285/635): the Issues | Actions | Chat subject strip over two
   columns (picker left, agent + options right) and a pinned action bar.
   Always a searchable multi-issue picker (EXP-106): 1 checked issue → a
   plain session on exp/<IDENTIFIER>, 2+ → ONE batch session on ONE
   exp/batch-<id8> branch ending in ONE combined PR. Defaults are per AGENT
   (EXP-206), not per mode. ─── */
import { useMemo, useState } from "react"
import { ISSUES, STATUS_LABEL } from "./data"
import { useIde, type CodingTarget } from "./state"
import { IcCheck, IcChevDown, IcCircleDot, IcSparkles, IcBot } from "./icons"

const AGENTS = [
  { id: `claude`, label: `Claude Code` },
  { id: `codex`, label: `Codex` },
  { id: `pi`, label: `pi` },
] as const

const MODELS = [`Fable`, `Opus`, `Sonnet`]
const EFFORTS = [`CLI default`, `Low`, `Medium`, `High`, `XHigh`, `Max`]
const SUBJECTS = [`Issues`, `Actions`, `Chat`] as const

function Field({
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
    <div className="ide-dlg-field">
      <span className="ide-dlg-fieldlabel">{label}</span>
      <button
        className={`ide-dlg-select${interactive && !disabled ? ` is-click` : ``}${disabled ? ` is-disabled` : ``}`}
        type="button"
        disabled={disabled}
        onClick={interactive && !disabled ? onCycle : undefined}
      >
        <span>{value}</span>
        <IcChevDown size={10} className="ide-c-muted" />
      </button>
    </div>
  )
}

function CheckRow({
  label,
  on,
  onToggle,
  disabled,
}: {
  label: string
  on: boolean
  onToggle: () => void
  disabled?: boolean
}) {
  const { interactive } = useIde()
  return (
    <button
      className={`ide-dlg-checkrow${interactive && !disabled ? ` is-click` : ``}`}
      type="button"
      role="checkbox"
      aria-checked={on}
      disabled={disabled}
      onClick={interactive && !disabled ? onToggle : undefined}
    >
      <span className={`ide-checkbox${on ? ` is-on` : ``}`}>{on && <IcCheck size={9} />}</span>
      <span className={disabled ? `ide-c-dim` : undefined}>{label}</span>
    </button>
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
  const [subject, setSubject] = useState(0)
  const [agent, setAgent] = useState(0)
  const [model, setModel] = useState(0)
  const [effort, setEffort] = useState(0)
  const [planMode, setPlanMode] = useState(true)
  const [ultracode, setUltracode] = useState(false)
  const [skipPerms, setSkipPerms] = useState(false)

  /* Every issue stays listed — done ones carry their status caption, like
     the real picker; the search narrows everything but the seeded pick. */
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase()
    return ISSUES.filter((issue) => {
      if (seeded.has(issue.id) || checked.has(issue.id)) return true
      return q.length === 0 || `${issue.id} ${issue.title}`.toLowerCase().includes(q)
    })
  }, [query, seeded, checked])

  if (!pendingCoding) return null
  const canStart = checked.size > 0

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
            {SUBJECTS.map((label, i) => (
              <button
                key={label}
                type="button"
                className={`ide-segment${i === subject ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => setSubject(i) : undefined}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="ide-dlg-cols">
            <div className="ide-dlg-left">
              <input
                className="ide-dlg-search"
                placeholder="Search issues..."
                value={query}
                readOnly={!interactive}
                onChange={(e) => setQuery(e.target.value)}
              />
              <div className="ide-dlg-issues">
                {rows.length === 0 ? (
                  <div className="ide-dlg-noresults">No matching issues.</div>
                ) : (
                  rows.map((issue) => {
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
                  })
                )}
              </div>
            </div>
            <div className="ide-dlg-right">
              <div className="ide-agentstrip">
                {AGENTS.map((a, i) => (
                  <button
                    key={a.id}
                    type="button"
                    className={`ide-agenttab${i === agent ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                    onClick={interactive ? () => setAgent(i) : undefined}
                  >
                    {a.id === `claude` ? (
                      <IcSparkles size={10} />
                    ) : a.id === `codex` ? (
                      <IcCircleDot size={10} />
                    ) : (
                      <IcBot size={10} />
                    )}
                    {a.label}
                  </button>
                ))}
              </div>
              <div className="ide-dlg-fields">
                <Field
                  label="Model"
                  value={MODELS[model]}
                  onCycle={() => setModel((i) => (i + 1) % MODELS.length)}
                />
                <Field
                  label="Effort"
                  value={ultracode ? `ultracode` : EFFORTS[effort]}
                  onCycle={() => setEffort((i) => (i + 1) % EFFORTS.length)}
                  disabled={ultracode}
                />
              </div>
              <div className="ide-dlg-checks">
                <CheckRow
                  label="Dynamic workflows (ultracode)"
                  on={ultracode}
                  onToggle={() => setUltracode((v) => !v)}
                />
                <CheckRow
                  label="Plan mode"
                  on={planMode}
                  onToggle={() => setPlanMode((v) => !v)}
                />
                <CheckRow
                  label="Skip permissions"
                  on={skipPerms}
                  onToggle={() => setSkipPerms((v) => !v)}
                />
              </div>
              {checked.size >= 2 && (
                <div className="ide-dlg-hint">
                  {`${checked.size} issues · one session on one exp/batch-… branch · one combined PR.`}
                </div>
              )}
            </div>
          </div>
        </div>
        <div className="ide-dlg-actions">
          <button
            className={`ide-btn-outline${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? cancelStartCoding : undefined}
          >
            Cancel
          </button>
          <button
            className={`ide-btn-primary${interactive && canStart ? ` is-click` : ``}`}
            type="button"
            disabled={!canStart}
            onClick={interactive && canStart ? start : undefined}
          >
            Start coding
          </button>
        </div>
      </div>
    </div>
  )
}

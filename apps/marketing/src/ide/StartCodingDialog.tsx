/* ─── The ONE unified Start-coding dialog — always a searchable multi-issue
   picker, mirroring the real desktop dialog (EXP-106). The checked count
   decides the run mode: 1 issue → a plain session on exp/<IDENTIFIER>,
   2+ → ONE batch session on ONE exp/batch-<id8> branch ending in ONE
   combined PR. Per-mode defaults: issue runs plan ON / ultracode OFF,
   batch runs ultracode ON / plan OFF. ─── */
import { useEffect, useMemo, useState } from "react"
import { ISSUES } from "./data"
import { useIde, type CodingTarget } from "./state"
import { StatusIcon } from "./bits"
import { IcCheck, IcChevsUpDown, IcPlay, IcSearch } from "./icons"

const MODELS = [`Fable`, `Opus`, `Sonnet`]
const EFFORTS = [`Default`, `Low`, `Medium`, `High`, `XHigh`, `Max`]

function SelectRow({
  label,
  options,
  index,
  onCycle,
  disabled,
  disabledValue,
}: {
  label: string
  options: string[]
  index: number
  onCycle?: () => void
  disabled?: boolean
  disabledValue?: string
}) {
  const { interactive } = useIde()
  return (
    <div className="ide-dlg-row">
      <span className="ide-dlg-label">{label}</span>
      <button
        className={`ide-dlg-select${interactive && !disabled ? ` is-click` : ``}${disabled ? ` is-disabled` : ``}`}
        type="button"
        disabled={disabled}
        onClick={interactive && !disabled ? onCycle : undefined}
      >
        {disabled ? (disabledValue ?? options[index]) : options[index]}
        <IcChevsUpDown size={11} className="ide-c-muted" />
      </button>
    </div>
  )
}

function SwitchRow({
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
    <div className="ide-dlg-row">
      <span className="ide-dlg-label">{label}</span>
      <button
        className={`ide-switch${on ? ` is-on` : ``}${interactive ? ` is-click` : ``}`}
        type="button"
        role="switch"
        aria-checked={on}
        onClick={interactive ? onToggle : undefined}
      >
        <span className="ide-switch-knob" />
      </button>
    </div>
  )
}

function CheckboxRow({
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
    <div className="ide-dlg-row">
      <span className="ide-dlg-label">{label}</span>
      <button
        className={`ide-dlg-check${interactive ? ` is-click` : ``}`}
        type="button"
        role="checkbox"
        aria-checked={on}
        onClick={interactive ? onToggle : undefined}
      >
        <span className={`ide-checkbox${on ? ` is-on` : ``}`}>{on && <IcCheck size={10} />}</span>
      </button>
    </div>
  )
}

/* Pre-seeded ids from the Play button (or the bulk bar) — checked from the
   start and exempt from the open-only/search filters, like the real dialog. */
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
  const [model, setModel] = useState(0)
  const [effort, setEffort] = useState(0)
  const [planMode, setPlanMode] = useState(true)
  const [ultracode, setUltracode] = useState(false)

  /* Mode defaults re-apply when the checked count flips modes. */
  const isBatch = checked.size >= 2
  useEffect(() => {
    setPlanMode(!isBatch)
    setUltracode(isBatch)
  }, [isBatch])

  /* The project's OPEN issues (done/cancelled/duplicate hidden); pre-seeded
     ids stay visible regardless of status or search — the pick wins. */
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase()
    return ISSUES.filter((issue) => {
      if (seeded.has(issue.id) || checked.has(issue.id)) return true
      if (issue.status === `done`) return false
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
        <div className="ide-dlg-title">Start coding</div>
        <div className="ide-dlg-search">
          <IcSearch size={12} />
          <input
            className="ide-dlg-search-input"
            placeholder="Search issues…"
            value={query}
            readOnly={!interactive}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="ide-dlg-issues">
          <div className="ide-dlg-repo">Niach/exponential</div>
          {rows.length === 0 ? (
            <div className="ide-dlg-noresults">No matching open issues.</div>
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
                    {on && <IcCheck size={10} />}
                  </span>
                  <StatusIcon status={issue.status} size={13} />
                  <span className="ide-dlg-issue-id">{issue.id}</span>
                  <span className="ide-dlg-issue-title">{issue.title}</span>
                </div>
              )
            })
          )}
        </div>
        {isBatch && (
          <div className="ide-dlg-hint">
            One session · one exp/batch-… branch · one combined PR.
          </div>
        )}
        <SelectRow
          label="Model"
          options={MODELS}
          index={model}
          onCycle={() => setModel((i) => (i + 1) % MODELS.length)}
        />
        <SelectRow
          label="Effort"
          options={EFFORTS}
          index={effort}
          onCycle={() => setEffort((i) => (i + 1) % EFFORTS.length)}
          disabled={ultracode}
          disabledValue="ultracode sets effort"
        />
        <SwitchRow
          label="Dynamic workflows (ultracode)"
          on={ultracode}
          onToggle={() => setUltracode((v) => !v)}
        />
        <CheckboxRow label="Plan mode" on={planMode} onToggle={() => setPlanMode((v) => !v)} />
        <div className="ide-dlg-actions">
          {!canStart && <span className="ide-dlg-note">Select at least one issue.</span>}
          <button
            className={`ide-btn-sm ide-btn-plain${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? cancelStartCoding : undefined}
          >
            Cancel
          </button>
          <button
            className={`ide-btn-sm ide-btn-primary ide-dlg-start${interactive && canStart ? ` is-click` : ``}`}
            type="button"
            disabled={!canStart}
            onClick={interactive && canStart ? start : undefined}
          >
            <IcPlay size={12} />
            Start coding
          </button>
        </div>
      </div>
    </div>
  )
}

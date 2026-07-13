/* ─── "Start coding on <id>" dialog — the beat before every scripted session.
   Issue variant: Model + Effort selects and the Plan mode switch. The release
   variant adds the per-repo issue checklist, subagent model/effort and the
   ultracode switch, mirroring the real desktop dialog. ─── */
import { useState } from "react"
import { getIssue, getRelease } from "./data"
import { useIde, type CodingTarget } from "./state"
import { StatusIcon } from "./bits"
import { IcCheck, IcChevsUpDown, IcPlay } from "./icons"

const MODELS = [`Fable`, `Opus`, `Sonnet`]
const EFFORTS = [`Default`, `Low`, `Medium`, `High`, `XHigh`, `Max`]
const SUBAGENT_MODELS = [`Same as main`, `Fable`, `Opus`, `Sonnet`]

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

function IssueChecklist({
  target,
  checked,
  onToggle,
}: {
  target: CodingTarget
  checked: Set<string>
  onToggle: (id: string) => void
}) {
  const { interactive } = useIde()
  const release = getRelease(target.id)
  return (
    <div className="ide-dlg-issues">
      <div className="ide-dlg-repo">Niach/exponential</div>
      {release.issueIds.map((id) => {
        const issue = getIssue(id)
        const on = checked.has(id)
        return (
          <div
            key={id}
            className={`ide-dlg-issue${interactive ? ` is-click` : ``}`}
            onClick={interactive ? () => onToggle(id) : undefined}
          >
            <span className={`ide-checkbox${on ? ` is-on` : ``}`}>
              {on && <IcCheck size={10} />}
            </span>
            <StatusIcon status={issue.status} size={13} />
            <span className="ide-dlg-issue-id">{id}</span>
            <span className="ide-dlg-issue-title">{issue.title}</span>
          </div>
        )
      })}
    </div>
  )
}

export function StartCodingDialog() {
  const { pendingCoding, cancelStartCoding, confirmStartCoding, interactive } = useIde()
  const [model, setModel] = useState(0)
  const [effort, setEffort] = useState(0)
  const [planMode, setPlanMode] = useState(false)
  const [subModel, setSubModel] = useState(0)
  const [ultracode, setUltracode] = useState(true)
  const [checked, setChecked] = useState<Set<string>>(() => {
    if (pendingCoding?.kind !== `release`) return new Set()
    /* Done issues are already merged — the run scopes to the open ones. */
    return new Set(
      getRelease(pendingCoding.id).issueIds.filter((id) => getIssue(id).status !== `done`),
    )
  })

  if (!pendingCoding) return null
  const isRelease = pendingCoding.kind === `release`
  const canStart = !isRelease || checked.size > 0

  return (
    <div className="ide-dlg-backdrop">
      <div className="ide-dlg">
        <div className="ide-dlg-title">
          {isRelease ? `Start coding on release` : `Start coding on ${pendingCoding.id}`}
        </div>
        {isRelease && (
          <IssueChecklist
            target={pendingCoding}
            checked={checked}
            onToggle={(id) =>
              setChecked((prev) => {
                const next = new Set(prev)
                if (next.has(id)) {
                  next.delete(id)
                } else {
                  next.add(id)
                }
                return next
              })
            }
          />
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
        />
        {isRelease ? (
          <>
            <SelectRow
              label="Subagent model"
              options={SUBAGENT_MODELS}
              index={subModel}
              onCycle={() => setSubModel((i) => (i + 1) % SUBAGENT_MODELS.length)}
            />
            <SelectRow
              label="Subagent effort"
              options={EFFORTS}
              index={0}
              disabled={ultracode}
              disabledValue="ultracode sets effort"
            />
            <SwitchRow
              label="Dynamic workflows (ultracode)"
              on={ultracode}
              onToggle={() => setUltracode((v) => !v)}
            />
          </>
        ) : (
          <SwitchRow label="Plan mode" on={planMode} onToggle={() => setPlanMode((v) => !v)} />
        )}
        <div className="ide-dlg-actions">
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
            onClick={interactive && canStart ? confirmStartCoding : undefined}
          >
            <IcPlay size={12} />
            Start coding
          </button>
        </div>
      </div>
    </div>
  )
}

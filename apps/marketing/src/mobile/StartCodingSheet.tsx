/* ─── Mobile Start-coding sheet — faithful phone recreation ───
   Mirrors the iOS StartCodingSheet (apps/ios .../Session/StartCodingSheet
   .swift, post-EXP-211): Cancel + Start coding ride the top bar (no center
   title), grouped cards with ONE uniform gap — Issues (inline search +
   checkbox rows), Desktop picker, the agent capsule strip above Model/
   Effort, and the Claude toggles. Sized in fixed px for the agents-section
   stage; decorative only (rendered under aria-hidden + inert). */
import { AGENTS } from "../components/agent-icons"
import { IcChevSwap, IcCircleCheck, IcSearch } from "../components/icons"
import { PriorityIcon, StatusIcon } from "../ide/bits"
import { ISSUES } from "../ide/data"

/* The picker offers the same board fixtures the IDE demo codes on —
   EXP-8 pre-checked, open siblings below (done issues aren't eligible). */
const SHEET_ISSUES = [`EXP-8`, `EXP-11`, `EXP-12`, `EXP-13`].map(
  (id) => ISSUES.find((i) => i.id === id) ?? ISSUES[0],
)
const CHECKED_ID = `EXP-8`

function SheetToggle({ on }: { on?: boolean }) {
  return <span className={`mss-toggle${on ? ` is-on` : ``}`} />
}

function PickerValue({ value }: { value: string }) {
  return (
    <span className={`mss-value`}>
      {value}
      <IcChevSwap size={12} />
    </span>
  )
}

export function MobileStartCodingSheet() {
  return (
    <div className={`mss-phone`}>
      <div className={`mss-topbar`}>
        <span className={`mss-pill`}>Cancel</span>
        <span className={`mss-pill is-start`}>Start coding</span>
      </div>

      <span className={`mss-label`}>Issues</span>
      <div className={`mss-card`}>
        <div className={`mss-search`}>
          <IcSearch size={12} />
          Search issues
        </div>
        {SHEET_ISSUES.map((issue) => {
          const checked = issue.id === CHECKED_ID
          return (
            <div key={issue.id} className={`mss-row mss-issue`}>
              {checked ? (
                <IcCircleCheck size={14} className={`mss-checkon`} />
              ) : (
                <span className={`mss-checkoff`} />
              )}
              <PriorityIcon priority={issue.priority} size={12} />
              <span className={`mss-id`}>{issue.id}</span>
              <StatusIcon status={issue.status} size={12} />
              <span className={`mss-title`}>{issue.title}</span>
            </div>
          )
        })}
      </div>

      <div className={`mss-card`}>
        <div className={`mss-row`}>
          <span>Desktop</span>
          <PickerValue value={`dennis-mbp.local`} />
        </div>
      </div>

      <div className={`mss-chips`}>
        {AGENTS.map(({ id, name, Logo }) => (
          <span
            key={id}
            className={`mss-chip${id === `claude` ? ` is-active` : ``}`}
          >
            <Logo size={11} />
            {name}
          </span>
        ))}
      </div>

      <div className={`mss-card`}>
        <div className={`mss-row`}>
          <span>Model</span>
          <PickerValue value={`Fable`} />
        </div>
        <div className={`mss-row`}>
          <span>Effort</span>
          <PickerValue value={`CLI default`} />
        </div>
      </div>

      <div className={`mss-card`}>
        <div className={`mss-row`}>
          <span>Ultracode</span>
          <SheetToggle />
        </div>
        <div className={`mss-row`}>
          <span>Plan mode</span>
          <SheetToggle on />
        </div>
        <div className={`mss-row`}>
          <span>Skip permissions</span>
          <SheetToggle />
        </div>
      </div>
    </div>
  )
}

/* ─── Mobile Start-coding sheet — faithful phone recreation ───
   Traced 1:1 off the captured iOS view (shots/start-coding/ios.webp): the
   sheet in apps/ios/.../Session/StartCodingSheet.swift over the shared
   LaunchOptionsSection. Order is the real one — Issues|Actions|Chat glass
   segmented control, the "Issues" section header, the grouped picker card
   (inline search + checkbox rows, EXP-8 checked), the agent capsule strip
   (the Device row hides itself when there is one machine), Model/Effort and
   the launch toggles.

   EVERY number below is authored in iOS POINTS: `.mss-screen` is a 414pt-wide
   canvas that `mobile.css` scales down with one transform, so the recreation
   stays measurable against the store shot instead of drifting into
   hand-tuned marketing px. Decorative only (rendered under aria-hidden +
   inert). */
import { AGENTS } from "../components/agent-icons"
import { IcChev, IcSearch } from "../components/icons"
import { ISSUES, type Issue } from "../ide/data"
import { MssPriorityIcon, MssStatusIcon, MssCheckIcon } from "./sheet-icons"

/* The picker offers the same board fixtures the IDE demo codes on, filtered
   the way the sheet filters its pool: terminal issues aren't eligible.
   EXP-8 is pre-checked and pinned first (the sheet snapshots pin order at
   open). */
const SHEET_ISSUES: Issue[] = ISSUES.filter((issue) => issue.status !== `done`)
const CHECKED_ID = `EXP-8`

const TABS = [`Issues`, `Actions`, `Chat`] as const

function PickerRow({ label, value }: { label: string; value: string }) {
  return (
    <div className={`mss-row is-picker`}>
      <span className={`mss-rowlabel`}>{label}</span>
      <span className={`mss-value`}>{value}</span>
      <IcChev size={12} stroke={2} className={`mss-chev`} />
    </div>
  )
}

function ToggleRow({ label }: { label: string }) {
  return (
    <div className={`mss-row is-toggle`}>
      <span className={`mss-rowlabel`}>{label}</span>
      <span className={`mss-toggle`} />
    </div>
  )
}

export function MobileStartCodingSheet() {
  return (
    <div className={`mss-phone`}>
      <div className={`mss-screen`}>
        <div className={`mss-topbar`}>
          <span className={`mss-pill`}>Cancel</span>
          <span className={`mss-pill is-start`}>Start coding</span>
        </div>

        <div className={`mss-seg`}>
          {TABS.map((tab) => (
            <span
              key={tab}
              className={`mss-segbtn${tab === `Issues` ? ` is-active` : ``}`}
            >
              {tab}
            </span>
          ))}
        </div>

        <div className={`mss-header`}>Issues</div>

        <div className={`mss-card`}>
          <div className={`mss-search`}>
            <IcSearch size={13} stroke={2} />
            <span>Search issues</span>
          </div>
          <div className={`mss-sep`} />
          <div className={`mss-list`}>
            {SHEET_ISSUES.map((issue) => (
              <div
                key={issue.id}
                className={`mss-irow${issue.id === CHECKED_ID ? ` is-checked` : ``}`}
              >
                <MssCheckIcon checked={issue.id === CHECKED_ID} />
                <MssPriorityIcon priority={issue.priority} />
                <span className={`mss-id`}>{issue.id}</span>
                <MssStatusIcon status={issue.status} />
                <span className={`mss-title`}>{issue.title}</span>
              </div>
            ))}
          </div>
        </div>

        <div className={`mss-seg is-agents`}>
          {AGENTS.map(({ id, name, Logo }) => (
            <span
              key={id}
              className={`mss-segbtn${id === `claude` ? ` is-active` : ``}`}
            >
              <Logo size={13} />
              {name}
            </span>
          ))}
        </div>

        <div className={`mss-card`}>
          <PickerRow label={`Model`} value={`Fable`} />
          <div className={`mss-sep`} />
          <PickerRow label={`Effort`} value={`CLI default`} />
        </div>

        <div className={`mss-card`}>
          <ToggleRow label={`Ultracode`} />
          <div className={`mss-sep`} />
          <ToggleRow label={`Plan mode`} />
        </div>
      </div>
    </div>
  )
}

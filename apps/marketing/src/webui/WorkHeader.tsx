/* ─── The ONE work header (work-header.tsx, EXP-877) ───
   The issue face and the run face both render it, so the title never moves
   when the face flips. Fixed above the scrolling body in the shared 896px
   column:
     Row 1: the 24px title | the right cluster on the same line — the
            Issue | Run | +N -M face toggle (work-face-toggle.tsx; an
            unavailable face is HIDDEN), the pin, the "…" actions menu
     Row 2: the properties tray (issue-properties-tray.tsx) — the property
            pills, then [Merge PR while the PR is open] [the ONE coding
            action: Stop on my live run, else Start coding]. */
import { PRIORITY_LABEL, STATUS_LABEL, type Issue } from "../ide/data"
import { useWeb, type WorkFace } from "./state"
import { PriorityGlyph, StatusGlyph, WebAvatar } from "./bits"
import { sessionFor, WEB_BOARD } from "./data"
import {
  ICON_3,
  ICON_35,
  ICON_4,
  IcCalendar,
  IcCode,
  IcEllipsis,
  IcMerged,
  IcPin,
  IcPlay,
  IcStop,
  IcTag,
} from "./icons"

function FaceToggle({ issue }: { issue: Issue }) {
  const { interactive, face, setFace } = useWeb()
  const session = sessionFor(issue.id)
  if (!session) return null
  const items: { face: WorkFace; label: React.ReactNode }[] = [
    { face: `issue`, label: `Issue` },
    { face: `run`, label: `Run` },
  ]
  /* The diff face appears once the run's changes have replayed — the issue
     face never waits on them. */
  if (face !== `issue`) {
    items.push({
      face: `diff`,
      label: (
        <span className="web-difflabel">
          <span className="is-add">+{session.additions}</span>{` `}
          <span className="is-del">-{session.deletions}</span>
        </span>
      ),
    })
  }
  return (
    <div className="web-seg">
      {items.map((item) => (
        <button
          key={item.face}
          type="button"
          className={`web-seg-btn${face === item.face ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          onClick={interactive ? () => setFace(item.face) : undefined}
        >
          {item.label}
        </button>
      ))}
    </div>
  )
}

function PropertiesTray({ issue }: { issue: Issue }) {
  const { interactive, startCoding } = useWeb()
  const session = sessionFor(issue.id)
  const prOpen = session?.state === `review`
  return (
    <div className="web-traywrap">
      <div className="web-tray">
        <div className="web-tray-props">
          <button className="web-prop is-click" type="button">
            <StatusGlyph status={issue.status} size={ICON_3} />
            {STATUS_LABEL[issue.status]}
          </button>
          <button className="web-prop is-click" type="button">
            <PriorityGlyph priority={issue.priority} size={ICON_3} />
            {PRIORITY_LABEL[issue.priority]}
          </button>
          <button className="web-prop is-click" type="button">
            <WebAvatar person={issue.assignee} size={ICON_35} />
            {issue.assignee ? issue.assignee.name : `Unassigned`}
          </button>
          <button className="web-prop is-click" type="button">
            <IcTag size={ICON_3} />
            {issue.labels?.length ? (
              issue.labels.map((l) => (
                <span key={l.name} className="web-prop-labels">
                  <span className="web-label-dot" style={{ background: l.color }} />
                  {l.name}
                </span>
              ))
            ) : (
              <span>Label</span>
            )}
          </button>
          <button className="web-prop is-click" type="button">
            <IcCalendar size={ICON_3} />
            {issue.due ?? `Due date`}
          </button>
          <button className="web-prop is-click" type="button">
            <IcCode size={ICON_3} style={{ color: WEB_BOARD.color }} />
            {WEB_BOARD.name}
          </button>
        </div>
        <div className="web-tray-actions">
          {prOpen && (
            <button className="web-prop is-primary is-click" type="button">
              <IcMerged size={ICON_3} />
              Merge PR
            </button>
          )}
          {session ? (
            <button className="web-prop is-danger is-click" type="button" title="Stop the agent and end the session">
              <IcStop size={ICON_3} />
              Stop
            </button>
          ) : (
            <button
              className={`web-prop is-primary${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={interactive ? () => startCoding(issue.id) : undefined}
            >
              <IcPlay size={ICON_3} />
              Start coding
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

export function WebWorkHeader({ issue }: { issue: Issue }) {
  return (
    <div className="web-workhead">
      <div className="web-workcol web-workhead-row">
        <div className="web-worktitle">{issue.title}</div>
        <div className="web-workhead-trailing">
          <FaceToggle issue={issue} />
          <button className="web-icbtn is-lg is-click" type="button" title="Pin">
            <IcPin size={ICON_4} />
          </button>
          <button className="web-icbtn is-lg is-click" type="button" title="Actions">
            <IcEllipsis size={ICON_4} />
          </button>
        </div>
      </div>
      <PropertiesTray issue={issue} />
    </div>
  )
}

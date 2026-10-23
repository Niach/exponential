/* The watch view for one background task (EXP-1369), drawn as a draft:
   the flow with how each step was done, the per-window live capture with
   the agent's cursor and its target outlined, the step graph, the A2UI
   replay, and the spoken-caption card. One loop (--wv-t) drives the cursor,
   the click, the steps, the download toast and the graph in step. */
import { Check, Download, Eye, Hand, Paperclip, Pause, Play, SkipBack } from "lucide-react"
import { Cursor } from "./ConceptFigure"

type Step = { title: string; how: string; kind?: `confirm` }

/* Steps 1 to 3 are done when the loop starts; 4, 5 and 6 advance in it. */
const STEPS: Step[] = [
  { title: `Opened Firefox`, how: `monitor 2 · focus untouched` },
  { title: `Payouts page`, how: `accessibility tree · 41 nodes` },
  { title: `Filter September`, how: `AX set value` },
  { title: `Click Export CSV`, how: `routed click · #export` },
  { title: `Wait for download`, how: `watch · payouts-2026-09.csv` },
  { title: `Attach CSV to #S-1`, how: `tracker · attachments` },
  { title: `File the totals`, how: `asks first · money`, kind: `confirm` },
]

const GRAPH = [`Firefox`, `Payouts`, `Filter`, `Export`, `Download`, `Attach`, `Confirm`]

const ROWS = [
  [`€ 4,210.00`, `Sep 29`, `Paid`],
  [`€ 3,980.50`, `Sep 22`, `Paid`],
  [`€ 5,102.10`, `Sep 15`, `Paid`],
  [`€ 2,744.00`, `Sep 8`, `Paid`],
  [`€ 3,311.90`, `Sep 1`, `Paid`],
]

function StepIcon({ index }: { index: number }) {
  if (index === 6) return <i className="wv-ic wv-ic--ask">?</i>
  return (
    <i className={`wv-ic wv-ic--${index + 1}`}>
      <span className="wv-ic-num">{index + 1}</span>
      <span className="wv-ic-run" />
      <span className="wv-ic-done">
        <Check size={10} strokeWidth={3} />
      </span>
    </i>
  )
}

export function WatchView() {
  return (
    <div className="wv" role="img" aria-label="Draft of the watch view: a task header with Watching, Pause agent and Take over; the flow of seven steps with how each was done; the live Firefox window with the agent's cursor clicking Export CSV; the step graph and the A2UI replay scrubber">
      <header className="wv-head">
        <span className="wv-live" />
        <strong className="wv-title">Export last month's payouts into the Steuer board</strong>
        <span className="wv-chip wv-chip--bg">Background · monitor 2</span>
        <span className="wv-chip">Firefox · Files</span>
        <span className="wv-chip wv-chip--you">Your pointer stays yours</span>
        <div className="wv-actions">
          <span className="wv-btn is-on">
            <Eye size={12} /> Watching
          </span>
          <span className="wv-btn">
            <Pause size={12} /> Pause agent
          </span>
          <span className="wv-btn wv-btn--danger">
            <Hand size={12} /> Take over
          </span>
        </div>
      </header>

      <div className="wv-body">
        <aside className="wv-flow">
          <div className="wv-flow-head">
            The flow <span>7 steps · 2 min 14 s</span>
          </div>
          <ol>
            {STEPS.map((step, i) => (
              <li key={step.title} className={`wv-step wv-step--${i + 1}${step.kind ? ` wv-step--${step.kind}` : ``}`}>
                <StepIcon index={i} />
                <div>
                  <b>{step.title}</b>
                  <small>{step.how}</small>
                </div>
              </li>
            ))}
          </ol>
        </aside>

        <div className="wv-window">
          <div className="wv-titlebar">
            <i />
            <i />
            <i />
            <span>dashboard.stripe.com/payouts</span>
            <em>per-window capture · 4 fps</em>
          </div>
          <div className="wv-page">
            <div className="wv-page-head">Payouts</div>
            <div className="wv-filters">
              <span className="wv-field is-set">Sep 1 to Sep 30, 2026</span>
              <span className="wv-field">Paid</span>
              <span className="wv-export">
                Export CSV
                <i className="wv-target" />
                <i className="wv-ripple" />
              </span>
            </div>
            <table>
              <thead>
                <tr>
                  <th>Amount</th>
                  <th>Arrival</th>
                  <th>Status</th>
                  <th>Bank</th>
                </tr>
              </thead>
              <tbody>
                {ROWS.map((row) => (
                  <tr key={row[1]}>
                    <td>{row[0]}</td>
                    <td>{row[1]}</td>
                    <td>{row[2]}</td>
                    <td>•••• 4471</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="wv-toast">
              <Download size={11} /> payouts-2026-09.csv · 12 KB
            </div>
            <Cursor tone="agent" label="agent" className="wv-cursor" />
          </div>
          <div className="wv-pip">
            <div className="wv-pip-head">
              <b>Agent</b>
              <span className="wv-wave" aria-hidden>
                <i />
                <i />
                <i />
                <i />
              </span>
              <em>spoken · hold F13 to reply</em>
            </div>
            <p>Exporting September payouts now. I will ask before I touch the totals.</p>
          </div>
        </div>
      </div>

      <div className="wv-graph">
        {GRAPH.map((label, i) => (
          <span key={label} className={`wv-node wv-node--${i + 1}${i === 6 ? ` wv-node--confirm` : ``}`}>
            <i />
            <b>{label}</b>
          </span>
        ))}
      </div>

      <div className="wv-replay">
        <span className="wv-btn wv-btn--icon">
          <SkipBack size={11} />
        </span>
        <span className="wv-btn">
          <Play size={11} /> Replay
        </span>
        <span className="wv-scrub">
          <i className="wv-scrub-fill" />
          {GRAPH.map((label, i) => (
            <i key={label} className="wv-scrub-mark" style={{ left: `${(i / (GRAPH.length - 1)) * 100}%` }} />
          ))}
        </span>
        <span className="wv-time">
          01:23 / 02:14 · A2UI replay · 3 KB <Paperclip size={10} />
        </span>
      </div>
    </div>
  )
}

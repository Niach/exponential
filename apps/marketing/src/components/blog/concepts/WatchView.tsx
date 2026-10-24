/* The watch view for one background task (EXP-1369), drawn as a draft and
   kept to six things: the task, one status line, one control, the
   per-window capture with the agent's cursor and its outlined target, the
   step line (done, current, waiting, confirm) with the current step's
   method, and the spoken caption. One loop (--wv-t) drives the cursor, the
   click, the steps and the download toast in step. */
import { Download } from "lucide-react"
import { Cursor } from "./ConceptFigure"

const STEPS = [`Firefox`, `Payouts`, `Filter`, `Export`, `Download`, `Attach`, `Confirm`]

const ROWS = [
  [`€ 4,210.00`, `Sep 29`, `Paid`],
  [`€ 3,980.50`, `Sep 22`, `Paid`],
  [`€ 5,102.10`, `Sep 15`, `Paid`],
  [`€ 2,744.00`, `Sep 8`, `Paid`],
]

export function WatchView() {
  return (
    <div className="wv" role="img" aria-label="Draft of the watch view: the task, a status line, a Take over control, the live Firefox window with the agent's cursor clicking Export CSV, a line of seven steps with the current step's method, and the agent's spoken caption">
      <header className="wv-head">
        <div>
          <strong className="wv-title">Export last month's payouts into the Steuer board</strong>
          <span className="wv-status">
            <i className="wv-live" /> background · monitor 2 · Firefox · your pointer stays yours
          </span>
        </div>
        <span className="wv-btn">Take over</span>
      </header>

      <div className="wv-window">
        <div className="wv-titlebar">dashboard.stripe.com/payouts</div>
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
              </tr>
            </thead>
            <tbody>
              {ROWS.map((row) => (
                <tr key={row[1]}>
                  <td>{row[0]}</td>
                  <td>{row[1]}</td>
                  <td>{row[2]}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="wv-toast">
            <Download size={11} /> payouts-2026-09.csv
          </div>
          <Cursor tone="agent" label="agent" className="wv-cursor" />
        </div>
      </div>

      <div className="wv-steps">
        <span className="wv-line">
          <i className="wv-line-fill" />
          {STEPS.map((label, i) => (
            <b key={label} className={`wv-node wv-node--${i + 1}${i === 6 ? ` wv-node--confirm` : ``}`} style={{ left: `${(i / (STEPS.length - 1)) * 100}%` }} title={label} />
          ))}
        </span>
        <span className="wv-current">
          <em className="wv-current--4">4 · Click Export CSV · routed click</em>
          <em className="wv-current--5">5 · Wait for download</em>
          <em className="wv-current--6">6 · Attach CSV to #S-1</em>
        </span>
        <span className="wv-time">01:23 / 02:14 · step replay</span>
      </div>

      <p className="wv-caption">
        <span className="wv-wave" aria-hidden>
          <i />
          <i />
          <i />
        </span>
        Exporting September payouts now. I will ask before I file the totals.
      </p>
    </div>
  )
}

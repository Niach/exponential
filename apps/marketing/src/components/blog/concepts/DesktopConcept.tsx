/* A desktop whose panel, windows and tray are surfaces (NEW-10, VAPP-64),
   drawn as a draft: two agents work in their own windows with their own
   seats while the person's cursor stays theirs, a window streams in from a
   request component by component, and the tray keeps the mic one key away.
   One loop (--dk-t) drives the three cursors and the streaming window. */
import { Bot, FileText, Folder, Mic, Search, Wifi } from "lucide-react"
import { Cursor } from "./ConceptFigure"

const FILES = [`payouts-2026-08.csv`, `payouts-2026-09.csv`, `invoices-q3.pdf`, `notes.md`]

export function DesktopConcept() {
  return (
    <div className="dk" role="img" aria-label="Draft of a desktop: a top panel with workspaces, a clock and a tray with an agents chip and a microphone; an Expenses window being built component by component from a request; a Firefox window and a Files window where two agents' cursors work; the person's cursor in the Expenses window">
      <div className="dk-panel">
        <span className="dk-launch">
          <Search size={10} /> Search or ask
        </span>
        <span className="dk-spaces">
          <i>1</i>
          <i className="is-you">2</i>
          <i className="is-agents">3</i>
        </span>
        <span className="dk-clock">Thu 14:02</span>
        <span className="dk-tray">
          <span className="dk-tray-agents">
            <Bot size={10} /> 2 agents · DP-2
          </span>
          <span className="dk-tray-mic">
            <Mic size={10} />
            <span className="dk-level" aria-hidden>
              <i />
              <i />
              <i />
              <i />
              <i />
            </span>
            <em>F13</em>
          </span>
          <Wifi size={10} />
        </span>
      </div>

      <div className="dk-desk">
        <div className="dk-win dk-win--built">
          <div className="dk-bar">
            <span>Expenses</span>
            <em className="dk-tag dk-tag--stream">streaming · a2ui</em>
            <em className="dk-tag dk-tag--saved">9 components · ~/.config/desk/expenses.a2ui.json</em>
          </div>
          <div className="dk-ask">
            <i>you</i> a form to log an expense, with a place to drop the receipt
          </div>
          <div className="dk-form">
            <div className="dk-c dk-c--1 dk-h">Log an expense</div>
            <label className="dk-c dk-c--2 dk-field is-focus">
              <span>Amount</span>
              <b>42.00</b>
            </label>
            <label className="dk-c dk-c--3 dk-field">
              <span>Category</span>
              <b>Travel</b>
            </label>
            <label className="dk-c dk-c--4 dk-field">
              <span>Date</span>
              <b>Oct 24, 2026</b>
            </label>
            <div className="dk-c dk-c--5 dk-drop">
              <FileText size={12} /> Drop the receipt here
            </div>
            <div className="dk-c dk-c--6 dk-save">Save</div>
          </div>
          <Cursor tone="you" label="you" className="dk-cursor dk-cursor--you" />
        </div>

        <div className="dk-win dk-win--firefox">
          <div className="dk-bar">
            <span>Firefox · payouts</span>
            <em className="dk-tag dk-tag--seat">seat · agent 1</em>
          </div>
          <div className="dk-ff">
            <div className="dk-ff-row dk-ff-row--filters">
              <i>Sep 2026</i>
              <i>Paid</i>
              <i className="dk-ff-btn">Export CSV</i>
            </div>
            <div className="dk-ff-row" />
            <div className="dk-ff-row" />
            <div className="dk-ff-row" />
            <div className="dk-ff-row" />
          </div>
          <Cursor tone="agent" label="agent 1" className="dk-cursor dk-cursor--a1" />
        </div>

        <div className="dk-win dk-win--files">
          <div className="dk-bar">
            <span>Files · ~/Steuer/2026-09</span>
            <em className="dk-tag dk-tag--seat">seat · agent 2</em>
          </div>
          <ul className="dk-files">
            {FILES.map((name, i) => (
              <li key={name} className={`dk-file dk-file--${i + 1}`}>
                <Folder size={10} /> {name}
              </li>
            ))}
          </ul>
          <Cursor tone="agent-2" label="agent 2" className="dk-cursor dk-cursor--a2" />
        </div>
      </div>
    </div>
  )
}

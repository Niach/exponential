/* The drafts of "Agents get their own seat" (NEW-10): how input reaches
   windows when the desktop owns the compositor (sd-*), the libraries and
   their one glue (lm-*), the agent that manages agents (oa-*) and an agent
   editing the desktop's own UI (ue-*). Drawn in HTML and CSS with CSS-only
   motion (styles/blog-agent-desktop.css), one loop per figure. */
import { Bot, Calendar, Code, FileText, Folder, Gamepad2, Globe, Keyboard, MessageSquare, Mic, Monitor, MousePointer2, Search, TriangleAlert, Wifi } from "lucide-react"

/* ── 1. Seats: your input and an agent's never share a route ── */

function Wire({ tone }: { tone: `you` | `agent` }) {
  return (
    <span className={`sd-wire sd-wire--${tone}`} aria-hidden>
      <i />
    </span>
  )
}

export function SeatsDiagram() {
  return (
    <div className="sd" role="img" aria-label="Two input routes through one compositor: your keyboard and mouse reach your seat and the windows on your monitor, a game among them; an agent's tool calls go through ePilot to a virtual seat and reach only the windows in its offscreen agent space, one of them a Brave window lent from your screen. Below, the foreground mode for macOS, Windows and X11: one seat, your input held, Esc to take it back">
      <div className="sd-lane sd-lane--you">
        <span className="sd-node sd-node--src">
          <Keyboard size={12} />
          <MousePointer2 size={12} /> you
        </span>
        <Wire tone="you" />
        <span className="sd-node">your seat</span>
        <Wire tone="you" />
        <span className="sd-node sd-node--comp">ede-compositor</span>
        <Wire tone="you" />
        <span className="sd-out">
          <b>
            <Monitor size={11} /> DP-1 · your monitor
          </b>
          <span className="sd-win">
            <Gamepad2 size={11} /> Dota 2 <em>fullscreen</em>
          </span>
          <span className="sd-win">
            <MessageSquare size={11} /> Exponential
          </span>
          <span className="sd-tile">
            <Globe size={10} /> Brave · lent to agent <em>tile · take back</em>
          </span>
        </span>
      </div>

      <div className="sd-lane sd-lane--agent">
        <span className="sd-node sd-node--src sd-node--agent">
          <Bot size={12} /> agent
        </span>
        <Wire tone="agent" />
        <span className="sd-node sd-node--agent">ePilot · background</span>
        <Wire tone="agent" />
        <span className="sd-node sd-node--agent">virtual seat</span>
        <Wire tone="agent" />
        <span className="sd-out sd-out--agent">
          <b>agent space 1 · offscreen</b>
          <span className="sd-win sd-win--agent">
            <Globe size={11} /> Brave <em>lent · covered, still captured</em>
          </span>
          <span className="sd-win sd-win--agent">
            <Folder size={11} /> Files <em>launched here</em>
          </span>
        </span>
      </div>

      <div className="sd-foot">
        <span>
          <span className="sd-chip sd-chip--fg">foreground</span> macOS · Windows · X11 · eDE: one seat, your input held while the agent acts, <kbd>Esc</kbd> takes it back
        </span>
        <span>
          <span className="sd-chip sd-chip--bg">background</span> eDE only: the agent's own seat, yours untouched
        </span>
      </div>
    </div>
  )
}

/* ── 2. The modules: every capability is an MCP server, eMCP is the glue ── */

const LIBS = [
  { id: `agent`, name: `eAgent`, does: `runs agents · orchestrates many` },
  { id: `pilot`, name: `ePilot`, does: `computer use` },
  { id: `talk`, name: `eTalk`, does: `voice in and out` },
  { id: `tools`, name: `ede-tools`, does: `the desktop's settings, UI, apps` },
  { id: `ui`, name: `eUI`, does: `every screen, as data` },
  { id: `net`, name: `eNet`, does: `peer to peer` },
]

export function ModuleMap() {
  return (
    <div className="lm" role="img" aria-label="Three apps on top, Exponential, Spezl and eDE, each a composition of the same libraries: eAgent, ePilot, eTalk, ede-tools, eUI and eNet. Below them one bar, eMCP, through which an agent run reaches every tool: a call travels from eAgent through eMCP to ePilot">
      <div className="lm-apps">
        <span className="lm-app">
          <b>Exponential</b> teams, issues, coding sessions
        </span>
        <span className="lm-app">
          <b>Spezl</b> one agent for everyone
        </span>
        <span className="lm-app">
          <b>eDE</b> the desktop
        </span>
      </div>
      <div className="lm-libs">
        {LIBS.map((lib) => (
          <span key={lib.id} className={`lm-lib lm-lib--${lib.id}`}>
            <b>{lib.name}</b>
            <em>{lib.does}</em>
            <i className="lm-port" aria-hidden />
          </span>
        ))}
      </div>
      <div className="lm-bus">
        <b>eMCP</b> serve · combine · grant · audit · code mode
        <span className="lm-call" aria-hidden>
          left_click {`{ window: "brave", x: 812, y: 96 }`}
        </span>
        <i className="lm-pulse" aria-hidden />
      </div>
    </div>
  )
}

/* ── 3. One agent you talk to, that manages the others ── */

export function OrchestraDiagram() {
  return (
    <div className="oa" role="img" aria-label="One root conversation with memory in the middle. On the left what wakes it: a schedule at 07:00, an app crash, you speaking. On the right the sessions it started: a background flow in Brave, a coding session, a research session. A message wakes the root, it starts a child, the child's report wakes it again">
      <div className="oa-col oa-col--wake">
        <span className="oa-src oa-src--1">
          <Calendar size={11} /> 07:00 schedule
        </span>
        <span className="oa-src oa-src--2">
          <TriangleAlert size={11} /> an app crashed
        </span>
        <span className="oa-src oa-src--3">
          <Mic size={11} /> “sort my tax folder”
        </span>
      </div>
      <div className="oa-mid">
        <span className="oa-wire oa-wire--in" aria-hidden>
          <i />
        </span>
        <span className="oa-root">
          <b>
            <Bot size={13} /> your agent
          </b>
          <em>one conversation · memory · never ends</em>
          <span className="oa-state">
            <i className="oa-zz">sleeping</i>
            <i className="oa-awake">awake</i>
          </span>
        </span>
        <span className="oa-wire oa-wire--out" aria-hidden>
          <i />
        </span>
      </div>
      <div className="oa-col oa-col--kids">
        <span className="oa-kid oa-kid--1">
          <Globe size={11} /> flow · Brave · tax portal <em className="oa-run">running</em>
          <em className="oa-rep">report ↩</em>
        </span>
        <span className="oa-kid oa-kid--2">
          <Code size={11} /> coding session · a fix <em>in review</em>
        </span>
        <span className="oa-kid oa-kid--3">
          <FileText size={11} /> research · insurance offers <em>done</em>
        </span>
      </div>
      <div className="oa-foot">
        Same rules, two homes: Exponential's server (a team, synced) and <b>eagent-hub</b> (local, one person: Spezl, eDE)
      </div>
    </div>
  )
}

/* ── 4. The desktop's UI is a file an agent may edit ── */

export function UiEditDemo() {
  return (
    <div className="ue" role="img" aria-label="An agent edits the panel's file: a new line adds a weather item to the tray, the panel shows it a moment later, and a toast offers to undo the change">
      <div className="ue-panel">
        <span className="ue-launch">
          <Search size={10} /> Search or ask <kbd>Super Space</kbd>
        </span>
        <span className="ue-spaces">
          <i>1</i>
          <i className="is-on">2</i>
          <i className="is-agent">3</i>
        </span>
        <span className="ue-tray">
          <span className="ue-new">☀ 14° Wien</span>
          <span className="ue-agents">
            <Bot size={10} /> 1 agent
          </span>
          <Mic size={10} />
          <Wifi size={10} />
          <span>Fri 14:02</span>
        </span>
      </div>
      <div className="ue-body">
        <div className="ue-ask">
          <i>you</i> put the weather for Vienna in the tray
        </div>
        <pre className="ue-code">
          <span className="ue-file">~/.config/ede/ui/panel.json</span>
          {`{ "id": "tray", "component": "Stack", "children": [\n`}
          <span className="ue-add">{`+   { "id": "weather", "component": "Chip", "label": "☀ 14° Wien" },`}</span>
          {`    { "id": "agents", "component": "Chip", "label": "1 agent" },\n`}
          {`    { "id": "mic", "component": "VoiceWave", "variant": "bars" },\n`}
          {`    { "id": "clock", "component": "Text", "text": "$clock" }\n  ] }`}
        </pre>
        <div className="ue-checks">
          <span className="ue-check ue-check--1">valid against the eUI catalog</span>
          <span className="ue-check ue-check--2">hot-reloaded</span>
          <span className="ue-check ue-check--3">commit 4f1c2a · Undo</span>
        </div>
      </div>
    </div>
  )
}

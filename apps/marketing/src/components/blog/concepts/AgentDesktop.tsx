/* The drafts of "ExponentialOS" (NEW-10): the whole system as layers
   (os-*), how input reaches windows when the system owns the compositor
   (sd-*), the agent that manages agents (oa-*) and an agent editing the
   desktop's own UI (ue-*). Drawn in HTML and CSS with CSS-only
   motion (styles/blog-agent-desktop.css), one loop per figure. */
import { Bot, Calendar, Code, Cpu, FileText, Folder, Gamepad2, Globe, HardDrive, Keyboard, MessageSquare, Mic, Monitor, MousePointer2, Network, Package, Search, Settings, TriangleAlert, Usb, Wifi } from "lucide-react"

/* ── 0. The whole system: one request walks the layers ── */

type Part = { name: string; note?: string; step?: number }
const LAYERS: { label: string; tone?: string; parts: Part[] }[] = [
  { label: `you`, parts: [{ name: `“put my September payouts in the tax folder”`, note: `voice or text`, step: 1 }] },
  { label: `apps`, parts: [{ name: `desktop`, note: `eUI shell` }, { name: `eUI apps` }, { name: `Linux apps`, note: `Wayland · X11` }] },
  {
    label: `libraries`,
    parts: [
      { name: `eTalk`, note: `voice`, step: 2 },
      { name: `eAgent`, note: `agents`, step: 3 },
      { name: `ePilot`, note: `computer use`, step: 5 },
      { name: `eUI`, note: `screens` },
      { name: `eNet`, note: `devices` },
    ],
  },
  { label: `glue`, tone: `glue`, parts: [{ name: `eMCP`, note: `tools · grants · audit`, step: 4 }, { name: `eCore`, note: `shared types` }] },
  { label: `services`, parts: [{ name: `files` }, { name: `network` }, { name: `packages` }, { name: `settings` }, { name: `devices` }] },
  { label: `compositor`, tone: `comp`, parts: [{ name: `seats`, note: `yours + one per agent`, step: 6 }, { name: `spaces` }, { name: `scene tree` }] },
  { label: `Linux`, tone: `base`, parts: [{ name: `kernel` }, { name: `drivers` }, { name: `systemd` }] },
]

const SERVICE_ICON: Record<string, typeof Folder> = { files: Folder, network: Network, packages: Package, settings: Settings, devices: Usb, kernel: Cpu, drivers: HardDrive }

export function OsStack() {
  return (
    <div className="os" role="img" aria-label="ExponentialOS as layers: you; apps (the desktop, eUI apps, Linux apps); libraries (eTalk, eAgent, ePilot, eUI, eNet); the glue (eMCP and eCore); system services as MCP servers (files, network, packages, settings, devices); the compositor (seats, spaces, the scene tree); Linux. A spoken request walks from you through eTalk and eAgent, through eMCP to ePilot, and lands in an agent's own seat in the compositor">
      {LAYERS.map((layer) => (
        <div key={layer.label} className={`os-layer${layer.tone ? ` os-layer--${layer.tone}` : ``}`}>
          <span className="os-label">{layer.label}</span>
          <span className="os-parts">
            {layer.parts.map((part) => {
              const Icon = SERVICE_ICON[part.name]
              return (
                <span key={part.name} className={`os-part${part.step ? ` os-step os-step--${part.step}` : ``}`}>
                  {Icon ? <Icon size={11} /> : null}
                  <b>{part.name}</b>
                  {part.note ? <em>{part.note}</em> : null}
                  {part.step ? <i className="os-num">{part.step}</i> : null}
                </span>
              )
            })}
          </span>
        </div>
      ))}
    </div>
  )
}

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
    <div className="sd" role="img" aria-label="Two input routes through one compositor: your keyboard and mouse reach your seat and the windows on your monitor, a game among them; an agent's tool calls go through ePilot to a virtual seat and reach only the windows in its offscreen agent space, one of them a Brave window lent from your screen. ">
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
          <span className="sd-chip sd-chip--fg">your seat</span> the real keyboard, mouse and monitors
        </span>
        <span>
          <span className="sd-chip sd-chip--bg">agent seats</span> virtual, one per agent, offscreen; yours is never touched
        </span>
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
        <b>eagent-hub</b> · the same rules on your computer and on a team's server
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

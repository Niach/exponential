/* Article 3 (NEW-10): agents get their own seat. The plan for eDE, our own
   compositor and desktop, and the libraries around it (ePilot, eAgent, eMCP).
   Four drafts drawn in HTML and CSS (components/blog/concepts/AgentDesktop)
   and one live surface (surfaces.ts); every claim says what exists and what
   is planned. */
import { Callout, ConceptFigure, LiveSurface, ModuleMap, OrchestraDiagram, SeatsDiagram, Section, StatusChip, UiEditDemo } from "../components/blog"
import { DESKTOP_CHAT_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  seats: { src: `/blog/agent-seats.webp`, alt: `Draft: your keyboard and mouse reach your seat and your monitor; an agent's calls go through ePilot to a virtual seat and an offscreen agent space`, width: 1392, height: 712 },
  chat: { src: `/blog/desktop-chat.webp`, alt: `The desktop's agent chat: a request, four background steps and the reply`, width: 1392, height: 1184 },
}

export const meta: PostMeta = {
  slug: `agents-get-their-own-seat`,
  title: `Agents get their own seat`,
  description: `Background computer use that never touches your mouse needs the one program that routes input: the compositor. Our plan for eDE, a desktop drawn in Exponential UI, and the libraries around it: ePilot for computer use, eAgent for agents that manage agents, eMCP as the glue.`,
  date: `2026-10-31`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: STILLS.seats,
  readingMinutes: 6,
  ogImage: `/og/og-blog-agents-get-their-own-seat.png`,
  sources: [`src/blog/agents-get-their-own-seat.tsx`, `src/blog/surfaces.ts`, `src/components/blog/concepts/AgentDesktop.tsx`, `src/styles/blog-agent-desktop.css`, `public/blog`],
}

export function Post() {
  return (
    <>
      <Section id="modes" num="01" label="Modes">
        <h2>
          Foreground everywhere, background where we own the screen <StatusChip status="planned" />
        </h2>
        <p>
          Computer use today runs in the foreground <StatusChip status="shipped" />: on macOS, Windows and Linux under X11 the agent
          moves the real pointer, your input is held while it acts, and <kbd>Esc</kbd> hands it back. It is
          predictable because nothing is faked. What it cannot do is work while you keep using the machine.
        </p>
        <p>
          We tried the other paths. A second pointer on X11 fought your cursor and stole focus for a moment on every
          click. An accessibility-only driver was fast (about 40 ms per action, focus never moved) but blind to any
          app without labels; our own desktop app showed it 31 buttons, all unnamed. Both are outside clients
          guessing at a display they do not own. The program that decides where every key and click goes is the
          compositor, so background control belongs there and nowhere else.
        </p>
        <ConceptFigure id="seats" still="agent-seats" caption="Your input and an agent's never share a route: the compositor gives the agent a virtual seat and an offscreen space; a window it borrows stays on your panel as a tile.">
          <SeatsDiagram />
        </ConceptFigure>
      </Section>

      <Section id="ede" num="02" label="eDE">
        <h2>
          A desktop drawn in Exponential UI <StatusChip status="planned" />
        </h2>
        <p>
          eDE is a Wayland compositor on Smithay plus a desktop shell. Windows float and snap, with a taskbar and a
          start menu people know from Mint or Windows. Every part of the shell is an Exponential UI document under{` `}
          <code>~/.config/ede/ui</code>, painted by the same renderer as the desktop app: panel, menu, notifications,
          settings, the lock screen. A change is checked against the catalog before it applies, reloads live, and
          lands as a git commit, so undo is one click and a broken edit falls back to the last good version.
        </p>
        <ConceptFigure id="ui-edit" still="ui-edit" caption="An agent edits the panel's file: the new line is checked, the tray shows it, the change is a commit you can undo.">
          <UiEditDemo />
        </ConceptFigure>
        <p>
          Inside the compositor an agent gets an <strong>agent space</strong>: a virtual output that is never on
          your monitor, with its own seat. Apps it launches open there. It can also borrow one of your windows,
          say the Brave where you are logged in; the window moves into its space and stays on your panel as a live
          tile until it comes back or you take it. Every window renders into its own buffer, so a covered window
          still captures. Agent spaces run with CPU and memory caps, so a game on your monitor keeps its frames.
          The lock screen and password prompts are never captured or driven; the compositor enforces that, not
          the agent.
        </p>
      </Section>

      <Section id="chat" num="03" label="Chat">
        <h2>
          The agent is part of the desktop <StatusChip status="planned" />
        </h2>
        <p>
          <kbd>Super Space</kbd> opens a chat with your agent; a held key or a wake word does the same by voice.
          It runs your own Claude Code or Codex locally, uses background computer use in its own space, and has the
          desktop's tools: its UI, its settings, installing an app after you confirm. When an app crashes, the
          notification offers to ask the agent with the logs attached. Omarchy showed how much a desktop gains
          when agents are first-class; the difference here is that edits go through checked documents and tools
          rather than raw dotfiles.
        </p>
        <LiveSurface id="desktop-chat" tree={DESKTOP_CHAT_SURFACE} still={STILLS.chat} caption="The chat panel, painted live by the React renderer from the JSON in this page: one request done in the background." />
      </Section>

      <Section id="modules" num="04" label="Modules">
        <h2>
          Libraries that work alone, glued by MCP <StatusChip status="in-progress" />
        </h2>
        <p>
          The same pieces ship in three apps: Exponential, Spezl (one agent for everyone, on any OS) and eDE. Each
          library is useful on its own, so none of them links another to borrow a capability. Every capability is
          an MCP server instead, and eMCP is the one library that serves, combines, permits and audits them.
        </p>
        <ul>
          <li>
            <strong>ePilot:</strong> computer use. Foreground on every desktop, background only on eDE; elsewhere
            it tells the agent why.
          </li>
          <li>
            <strong>eAgent:</strong> runs your own agent CLI and orchestrates many (next section).
          </li>
          <li>
            <strong>eMCP:</strong> the tool layer: grants per run, an audit row per call, the local hub, code mode.
          </li>
          <li>
            <strong>eUI, eTalk, eNet:</strong> every screen as data, voice in and out, peer-to-peer transport.
          </li>
        </ul>
        <ConceptFigure id="modules" still="module-map" caption="Three apps, one set of libraries; an agent reaches every tool through eMCP: here a click on a borrowed window.">
          <ModuleMap />
        </ConceptFigure>
      </Section>

      <Section id="eagent" num="05" label="eAgent">
        <h2>
          One agent you talk to, that manages the others <StatusChip status="planned" />
        </h2>
        <p>
          Spezl's idea is one agent that is everything you talk to. Exponential's assistant manages other agents
          and wakes up on its own. They are the same shape: a conversation that never ends and keeps memory, child
          sessions it starts and messages, and wake-ups from a schedule, an event or a child's report. The rules
          for that tree become one shared spec. Exponential's server runs it for a team, synced; the local hub
          runs it for one person in Spezl and eDE.
        </p>
        <ConceptFigure id="orchestra" still="agent-orchestra" caption="A schedule wakes your agent, it starts a flow in Brave, the flow's report wakes it again; in between it sleeps.">
          <OrchestraDiagram />
        </ConceptFigure>
      </Section>

      <Section id="mint" num="06" label="Mint">
        <h2>Trying it on the Linux you have</h2>
        <p>
          eDE installs as a package that adds a session to the login screen; the desktop you have stays one click
          away. Moving from X11 to Wayland changes a few things, and these are the ones we plan for:
        </p>
        <ul>
          <li>
            <strong>X11 games on NVIDIA</strong> run through Xwayland, which needs version 24.1 or newer for smooth
            frames. We ship it.
          </li>
          <li>
            <strong>Screen sharing and screenshots</strong> go through portals; eDE brings its own.
          </li>
          <li>
            <strong>Tools that fake global input</strong> (xdotool, auto-type) stop working. Wayland forbids them
            by design, and ePilot reaches eDE through the compositor instead.
          </li>
          <li>
            <strong>Your old panel and applets</strong> are replaced by eDE's; ordinary GTK and Qt apps run as
            before.
          </li>
        </ul>
      </Section>

      <Section id="next" num="07" label="Next">
        <h2>
          An operating system wired with MCP <StatusChip status="planned" />
        </h2>
        <p>
          Follow the glue far enough and the system services become tools too: network, packages, files, devices,
          each an MCP server behind the same grants and audit. Over eNet, the same tools can reach your other
          devices and, with a grant, other people's agents: a network of capabilities rather than of pages. We
          would build it in that order, your own devices first, because every hop adds a place for a prompt
          injection to travel.
        </p>
      </Section>

      <Section id="status" num="08" label="Status">
        <h2>What exists</h2>
        <ul>
          <li>
            <strong>Foreground computer use:</strong> shipped on macOS, Windows and X11.
          </li>
          <li>
            <strong>The agent runtime:</strong> shipped inside Exponential and extracted as a library; the local
            orchestration is planned.
          </li>
          <li>
            <strong>eMCP:</strong> the contract and the server half exist; the local hub is planned.
          </li>
          <li>
            <strong>eDE and background computer use:</strong> planned; the figures above are drafts.
          </li>
        </ul>
        <Callout kind="note" title="Earlier">
          The idea of UI as data reaching down to the compositor is the{` `}
          <a href="/blog/ui-as-data-down-to-the-compositor/">previous post</a>; this one is the plan to build it.
        </Callout>
      </Section>
    </>
  )
}

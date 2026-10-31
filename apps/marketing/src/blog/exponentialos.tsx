/* Article 3 (NEW-10): ExponentialOS, a computer built for agents. One
   architecture, explained part by part, mostly drawn: five drafts in HTML
   and CSS (components/blog/concepts/AgentDesktop) and one live surface
   (surfaces.ts). */
import { Callout, ConceptFigure, LiveSurface, OrchestraDiagram, OsStack, SeatsDiagram, Section, StatusChip, UiEditDemo } from "../components/blog"
import { DESKTOP_CHAT_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  stack: { src: `/blog/os-stack.webp`, alt: `ExponentialOS as layers, from Linux and the compositor up through services, the glue and the libraries to apps and you`, width: 1392, height: 828 },
  chat: { src: `/blog/desktop-chat.webp`, alt: `The desktop's agent chat: a request, four background steps and the reply`, width: 1392, height: 1184 },
}

export const meta: PostMeta = {
  slug: `exponentialos`,
  title: `ExponentialOS: a computer built for agents`,
  description: `A Linux system where agents are part of the computer: their own seats in the compositor, a desktop drawn as data, every service a tool, and a handful of libraries that fit together through MCP. One architecture, part by part.`,
  date: `2026-10-31`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: STILLS.stack,
  readingMinutes: 3,
  ogImage: `/og/og-blog-exponentialos.png`,
  sources: [`src/blog/exponentialos.tsx`, `src/blog/surfaces.ts`, `src/components/blog/concepts/AgentDesktop.tsx`, `src/styles/blog-agent-desktop.css`, `public/blog`],
}

const PARTS: [string, string][] = [
  [`Linux`, `The kernel, drivers and systemd. Everything that runs on Linux runs here.`],
  [`Compositor`, `Owns the screen and every input. Gives you a seat and each agent its own.`],
  [`Services`, `Files, network, packages, settings, devices: each one an MCP server.`],
  [`eMCP`, `The glue between parts: serves tools, grants them per agent, writes an audit row per call.`],
  [`eCore`, `The types every part shares: ids, grants, results, events. Nothing else.`],
  [`eAgent`, `Runs your agent and the agents it starts, wakes them on schedules and events.`],
  [`ePilot`, `Computer use: sees and drives windows in an agent's own seat.`],
  [`eTalk`, `Voice in and out: wake word, speech to text, text to speech.`],
  [`eUI`, `Every screen as data, painted natively. The desktop is made of it.`],
  [`eNet`, `Your devices, connected directly to each other.`],
]

export function Post() {
  return (
    <>
      <Section id="picture" num="01" label="Picture">
        <h2>
          The whole picture <StatusChip status="planned" />
        </h2>
        <p>
          ExponentialOS is Linux with agents built in. Each layer does one job, and the parts talk to each other
          through tools. Follow one spoken request down the stack: it reaches an agent, the agent calls computer
          use through the glue, and the click lands in the agent's own seat.
        </p>
        <ConceptFigure id="stack" still="os-stack" caption="One request, six steps: your voice, eTalk, eAgent, eMCP, ePilot, an agent's seat in the compositor.">
          <OsStack />
        </ConceptFigure>
      </Section>

      <Section id="seats" num="02" label="Seats">
        <h2>Every agent gets its own seat</h2>
        <p>
          The compositor decides where every key and click goes. You get a seat with the real keyboard, mouse and
          monitors; each agent gets a virtual one and an offscreen space. Agents work while you play, and your
          pointer never moves. A window an agent borrows stays on your panel as a live tile.
        </p>
        <ConceptFigure id="seats" still="agent-seats" caption="Two routes through one compositor: yours to your monitor, the agent's to its own space.">
          <SeatsDiagram />
        </ConceptFigure>
      </Section>

      <Section id="data" num="03" label="Data">
        <h2>The desktop is data</h2>
        <p>
          Panel, menu, notifications and settings are eUI documents on disk. An agent changes them like any file:
          each edit is checked, appears live and can be undone.
        </p>
        <ConceptFigure id="ui-edit" still="ui-edit" caption="“Put the weather in the tray”: one line in the panel's file, checked, live, a commit.">
          <UiEditDemo />
        </ConceptFigure>
      </Section>

      <Section id="agent" num="04" label="Agent">
        <h2>Your agent, one key away</h2>
        <p>
          <kbd>Super Space</kbd> or your voice opens a chat with your agent. It is one conversation that never
          ends: it starts other agents for longer work, sleeps, and wakes when they report or a schedule fires.
        </p>
        <LiveSurface id="desktop-chat" tree={DESKTOP_CHAT_SURFACE} still={STILLS.chat} caption="The chat panel, painted live by the eUI renderer from the JSON in this page." />
        <ConceptFigure id="orchestra" still="agent-orchestra" caption="A schedule wakes your agent, it starts a flow, the flow's report wakes it again.">
          <OrchestraDiagram />
        </ConceptFigure>
      </Section>

      <Section id="parts" num="05" label="Parts">
        <h2>The parts</h2>
        <dl className="os-parts-list">
          {PARTS.map(([name, text]) => (
            <div key={name}>
              <dt>{name}</dt>
              <dd>{text}</dd>
            </div>
          ))}
        </dl>
        <p>Each library works on its own in any project; together they are the system.</p>
      </Section>

      <Section id="today" num="06" label="Today">
        <h2>What exists today</h2>
        <ul>
          <li>
            <strong>eUI</strong> <StatusChip status="shipped" /> six renderers, one conformance suite.
          </li>
          <li>
            <strong>eAgent, ePilot, eTalk, eNet</strong> <StatusChip status="shipped" /> inside Exponential, released as libraries.
          </li>
          <li>
            <strong>eMCP</strong> <StatusChip status="in-progress" /> the contract and the server half.
          </li>
          <li>
            <strong>The compositor, the desktop, eCore</strong> <StatusChip status="planned" /> the figures above are drafts.
          </li>
        </ul>
        <Callout kind="note" title="Earlier">
          How a screen becomes data, from one card to the compositor:{` `}
          <a href="/blog/ui-as-data-down-to-the-compositor/">UI as data, down to the compositor</a>.
        </Callout>
      </Section>
    </>
  )
}

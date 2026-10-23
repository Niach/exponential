/* Article 2 (EXP-1368): what a personal assistant should be able to do, in
   four abilities, and how much of each exists. Written like a Guide; the
   three drafts are A2UI surfaces (surfaces.ts) painted live by the
   Exponential UI React renderer, their stills captured from that render. */
import { Callout, IssueLink, LiveSurface, Section, Shot, ShotRow, StatusChip } from "../components/blog"
import { STACK_SURFACE, VOICE_SURFACE, WATCH_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  stack: { src: `/blog/assistant-stack.webp`, alt: `The four abilities as cards: it shows, it shapes its environment, it has hands you can watch, you can talk to it`, width: 1392, height: 1852 },
  watch: { src: `/blog/assistant-watch.webp`, alt: `Draft of a watch view: the flow as trace lines, the live window with a payouts table, the step graph and the replay`, width: 1392, height: 1582 },
  voice: { src: `/blog/assistant-voice.webp`, alt: `Draft of a voice pipeline: mic and desktop audio, local speech-to-text, a small router model, the assistant, the spoken reply`, width: 1392, height: 1118 },
}

export const meta: PostMeta = {
  slug: `what-an-assistant-should-do`,
  title: `What a personal assistant should be able to do`,
  description: `Four abilities a personal assistant needs from its software: a screen it can draw, a desktop it can edit, screen control the person can watch, and voice. What exists of each today and what is a draft.`,
  date: `2026-10-23`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: STILLS.watch,
  readingMinutes: 3,
  ogImage: `/og/og-blog-what-an-assistant-should-do.png`,
  sources: [`src/blog/what-an-assistant-should-do.tsx`, `src/blog/surfaces.ts`, `src/components/blog`, `public/blog`, `../../shots/exponential-ui-demo`],
}

export function Post() {
  return (
    <>
      <Section id="four-abilities" num="01" label="Four abilities">
        <h2>The four abilities</h2>
        <p>
          A personal assistant needs four abilities from the software around it: a way to draw a screen instead of
          describing it, a desktop it can edit, screen control the person can watch, and voice. Each section states
          what exists and what is a draft. The drafts are marked and are themselves painted by the renderer from the
          first section.
        </p>
        <LiveSurface id="assistant-stack" tree={STACK_SURFACE} still={STILLS.stack} caption="The four abilities, each with its state and the issues behind it." />
      </Section>

      <Section id="show" num="02" label="Showing">
        <h2>It shows, instead of telling <StatusChip status="shipped" /></h2>
        <p>
          Asked how a release went, the assistant answers with a card: a status, two numbers, a chart, two buttons.
          A2UI lets a model describe that screen as JSON from a fixed catalog, and a renderer paints it natively in
          the host's theme. Exponential UI (<IssueLink id="VAPP-84" />) does this for React, Solid, a web component,
          SwiftUI, Compose and gpui from one Rust core; every painter passes the same conformance suite, and a
          benchmark suite with ten scenarios, results per machine and a CI ratchet merged on October 23
          (<IssueLink id="VAPP-105" />). First paint of a 100,000-row list on an M4 Pro: 62 ms in the core, 112 ms in
          Compose, 120 ms in React, 160 ms in Solid, 341 ms in SwiftUI, which is a listed finding with its own issue.
          A URL policy denies private hosts by default and a consent card shows a host call's arguments before it runs
          (<IssueLink id="VAPP-107" />). Apps without code, an editor with a chat beside a live preview, are in
          progress (<IssueLink id="VAPP-110" /> <StatusChip status="in-progress" />).
        </p>
        <Shot view="exponential-ui-demo" platform="web" caption="One surface, the answer to a release question, on the web." />
        <ShotRow>
          <Shot view="exponential-ui-demo" platform="ios" caption="The same surface in SwiftUI" />
          <Shot view="exponential-ui-demo" platform="android" caption="Compose" />
        </ShotRow>
      </Section>

      <Section id="environment" num="03" label="Environment">
        <h2>It shapes its environment <StatusChip status="planned" /></h2>
        <p>
          If a screen can be painted from a description, the panel, launcher, settings and file view can be
          descriptions on disk too, and the assistant edits them while they are shown. The plan is a desktop
          environment painted by the same renderer, first on Linux Mint, later on a Nix-based system where the
          assistant installs software by writing configuration (<IssueLink id="NEW-10" />, <IssueLink id="VAPP-64" />).
          None of it exists yet beyond the renderer.
        </p>
      </Section>

      <Section id="hands" num="04" label="Hands">
        <h2>It has hands you can watch <StatusChip status="in-progress" /></h2>
        <p>
          Some tasks have no API, so the assistant drives the screen, and the person needs to see what it does. Today
          a session sees and drives the screen on Linux, macOS and Windows in the foreground, with your input held
          and Esc to take it back (<IssueLink id="EXP-1196" />, <IssueLink id="EXP-1273" />). The draft below adds
          work on a second monitor, accessibility actions before pixel clicks, and every run recorded as an A2UI
          replay that can be scrubbed, not a video (<IssueLink id="EXP-1369" />, <IssueLink id="VAPP-81" />).
        </p>
        <LiveSurface id="assistant-watch" tree={WATCH_SURFACE} still={STILLS.watch} caption="Draft of the watch view: the flow, the live window, the step graph, the replay." />
      </Section>

      <Section id="voice" num="05" label="Voice">
        <h2>You can talk to it <StatusChip status="planned" /></h2>
        <p>
          Voice is usable only if the first stages run locally and fast. Two small models make that feasible on a
          CPU: a 16.9 MB speech-to-text model with word timestamps, and an 8 to 29 MB router that returns tool calls
          with a confidence, asks when unsure and returns nothing for off-topic speech (<IssueLink id="NEW-9" />).
          Push to talk or a wake word opens the mic, desktop audio is captured per app, the router acts, confirms or
          hands the text to the assistant in about 50 ms, and the reply is spoken back.
        </p>
        <LiveSurface id="assistant-voice" tree={VOICE_SURFACE} still={STILLS.voice} caption="Draft of the voice pipeline. The first three stages run on the device." />
      </Section>

      <Section id="away" num="06" label="Away">
        <h2>It works while you are away <StatusChip status="planned" /></h2>
        <p>
          With the four abilities in place, the assistant can run a loop: take the open work, build it, release, test
          it by driving real devices, and file what it finds as the next round. The first slice is a caretaker that
          triages and proposes, and asks before it builds (<IssueLink id="NEW-11" />).
        </p>
      </Section>

      <Section id="feasible" num="07" label="Today">
        <h2>Status</h2>
        <ul>
          <li><strong>Showing:</strong> shipped. Open renderers on six platforms, one conformance suite, a benchmark suite with a ratchet.</li>
          <li><strong>Environment:</strong> not started; it uses the same renderer.</li>
          <li><strong>Hands:</strong> foreground shipped on three operating systems; background and the watch view are drafts.</li>
          <li><strong>Voice:</strong> the models run on a CPU; the pipeline is a plan.</li>
          <li><strong>The loop:</strong> a plan; the caretaker comes first.</li>
        </ul>
        <Callout kind="note" title="Follow along">
          Every item is an issue on a public board. The renderer is on npm and crates.io under{` `}
          <code>exponential_app</code>; <a href="https://ui.exponential.at/">ui.exponential.at</a> shows how to paint
          a surface on each platform.
        </Callout>
      </Section>
    </>
  )
}

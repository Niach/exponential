/* Article 2 (EXP-1368): where we are going, in four layers on one renderer.
   The three drafts are A2UI surfaces (surfaces.ts) painted live by the
   Exponential UI React renderer; their stills come from that render. The
   real captures are store shots. */
import { Callout, IssueLink, LiveSurface, Shot, ShotRow, StatusChip } from "../components/blog"
import { STACK_SURFACE, VOICE_SURFACE, WATCH_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  stack: { src: `/blog/four-layers-stack.webp`, alt: `The four layers as cards: A2UI and Exponential UI, a desktop that is a document, computer use you can watch, a speech assistant`, width: 1392, height: 1796 },
  watch: { src: `/blog/four-layers-watch.webp`, alt: `Draft of the watch view: the flow as trace lines, the live window with a payouts table, the step graph and the replay slider`, width: 1392, height: 1582 },
  voice: { src: `/blog/four-layers-voice.webp`, alt: `Draft of the voice pipeline: mic and desktop audio, Whistle speech-to-text, the Needle router, the Assistant agent and the spoken reply`, width: 1392, height: 1118 },
}

export const meta: PostMeta = {
  slug: `one-renderer-four-layers`,
  title: `One renderer, four layers`,
  description: `Agents describe UI as JSON and one Rust core paints it natively on every platform. On that renderer: a desktop that is a document, computer use you can watch, and a voice in front of all of it. What is shipped, what is drafted.`,
  date: `2026-10-23`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: STILLS.stack,
  readingMinutes: 5,
  ogImage: `/og/og-blog-one-renderer-four-layers.png`,
  sources: [`src/blog/one-renderer-four-layers.tsx`, `src/blog/surfaces.ts`, `src/components/blog`, `public/blog`, `../../shots/exponential-ui-demo`],
}

export function Post() {
  return (
    <>
      <section id="four-layers">
        <h2>Four layers</h2>
        <p>
          The <a href="/blog/where-we-are/">first post</a> described the thread: one place where people and agents
          work on an issue. This one is about what the agents will build their own screens with, and the three things
          we are putting on top of it. One layer is shipped, one is in progress, two are drafts. The pictures in this
          post say which: a capture is a capture, a draft carries a concept stamp. The three drafts are not mockups
          drawn in a design tool. They are A2UI trees painted by the same renderer the app ships, which is the point of
          the first layer.
        </p>
        <LiveSurface id="four-layers-stack" tree={STACK_SURFACE} still={STILLS.stack} caption="The four layers. Shipped, in progress, planned, with the issues behind each." />
      </section>

      <section id="a2ui">
        <h2>A2UI + Exponential UI <StatusChip status="shipped" /></h2>
        <p>
          A2UI is a JSON protocol for agent-generated UI: an agent streams a surface as components with props and a
          data model, and a renderer on the client paints it. Exponential UI (<IssueLink id="VAPP-84" />) is our
          renderer set for it, open under Apache-2.0 with its own site at{` `}
          <a href="https://ui.exponential.at/">ui.exponential.at</a>. One core catalog of about seventy components
          (rows, cards, tables, charts, traces, forms, overlays), one Rust core for layout, and native painters: React,
          Solid and a web component on the web, SwiftUI on iOS and macOS, Compose on Android, gpui in the desktop app.
          Every painter passes the same conformance suite, so a surface drawn on one platform looks the same on the
          other five.
        </p>
        <p>
          Themes are JSON, so a host paints surfaces in its own look; extensions add components a host needs. The
          Exponential app itself goes through that public API, with no private hooks. The numbers we track are plain:
          on a shared 100,000-row list, first paint is 70 ms in the Rust core, 87 ms in gpui, 147 ms in React on
          Chromium, 167 ms in SwiftUI and 139 ms in Compose on the JVM, and <IssueLink id="VAPP-105" />{` `}
          <StatusChip status="in-progress" /> turns those into a benchmark suite with real-device runs and a CI
          ratchet. Bundle and binary size got their own gate this week (<IssueLink id="VAPP-106" />).
        </p>
        <p>
          The next step on this layer is apps without code (<IssueLink id="VAPP-110" /> <StatusChip status="in-progress" />
          ): a package is a manifest, A2UI templates, bindings onto the team's synced data and an allowlist of
          functions. Data, never code. An agent writes one through MCP, every client renders it through the same host.
        </p>
        <Shot view="exponential-ui-demo" platform="web" caption="One surface, the answer an agent streams for a release question, painted on the web." />
        <ShotRow>
          <Shot view="exponential-ui-demo" platform="ios" caption="The same surface in SwiftUI" />
          <Shot view="exponential-ui-demo" platform="android" caption="Compose" />
        </ShotRow>
        <Shot view="exponential-ui-demo" platform="desktop" caption="gpui in the desktop app." />
      </section>

      <section id="desktop">
        <h2>A desktop that is a document <StatusChip status="planned" /></h2>
        <p>
          If a renderer paints any screen from JSON on disk, a desktop environment can be one too. That is the
          Softcoded DE (<IssueLink id="NEW-10" />): the panel, the launcher, settings and the file view are A2UI
          templates in files, painted by the gpui renderer, and the agent that already edits your repository edits
          your desktop the same way, live. Existing Linux apps keep running beside it. The architecture it builds on is
          the shell with slots and a distribution manifest in <IssueLink id="VAPP-64" />: one codebase, a shell that
          exposes slots, apps that fill them, and a distribution as data.
        </p>
        <p>
          The plan starts on Linux Mint, because that is where our computer use already runs and where most of the
          hardware questions are answered. A Nix-based system, where the agent installs software and changes the
          system by writing configuration, is the north star, not the first step. Nothing of this exists yet beyond the
          renderer it would be made of.
        </p>
      </section>

      <section id="watch">
        <h2>Computer use you can watch <StatusChip status="planned" /></h2>
        <p>
          What exists: a session on Linux, macOS or Windows can see the screen and drive it (<IssueLink id="EXP-1196" />
          ), in the foreground, with your keyboard and mouse held while it works and Esc to take them back
          (<IssueLink id="EXP-1273" />, October 20). We tried a background mode on a third-party driver first, with a
          second pointer so you kept yours, and removed it: too many platform-specific holes for what it gave.
        </p>
        <p>
          What we want is the version where you play on monitor one while the agent works on monitor two, and you can
          watch. The order of operations is the same on every platform: the accessibility tree first (set a value,
          press a button, no pixels), window-routed input next, the real pointer only when you ask for it. Every run is
          recorded as a flow, and that flow is an A2UI replay (<IssueLink id="VAPP-81" />): a timestamped list of
          surface updates you can scrub, a few kilobytes, drawn in your client's theme, never a video.
        </p>
        <p>
          The draft below is the product issue for that view (<IssueLink id="EXP-1369" />): the flow as trace lines on
          the left with how each step was done, the live window on the right with the target element, a step graph and
          the replay scrubber below, and a small spoken caption for the case where you are not looking.
        </p>
        <LiveSurface id="four-layers-watch" tree={WATCH_SURFACE} still={STILLS.watch} caption="Draft of the watch view: the flow, the live window, the step graph, the replay. Not product UI yet." />
      </section>

      <section id="voice">
        <h2>A speech assistant, local first <StatusChip status="planned" /></h2>
        <p>
          The last layer puts a voice in front of the thread system (<IssueLink id="NEW-9" />). The pieces are two
          small models we measured on October 10. Whistle is speech-to-text in a 16.9 MB file: 16 kHz mono, one pass
          of up to thirty seconds, word timestamps, seven languages, on the CPU. Needle 3 is a text model between 8 and
          29 MB that calls tools with a confidence head: it answers a request with the tool calls to make, asks when it
          is unsure, and returns an empty list for something off-topic instead of guessing.
        </p>
        <p>
          The pipeline: a push-to-talk key or a wake word opens the mic; desktop audio is captured per app, so a call in
          Zoom is its own channel. Whistle transcribes each channel and the transcripts merge by timestamp. Needle
          routes the result in about 50 ms: act on a quick command, confirm, or hand the text over. Handing over is one
          call, <code>sessions.send</code> into your conversation with the Assistant agent, the same team agent from the
          first post, with the full model and its tools. The reply is spoken back and shown as a caption.
        </p>
        <p>
          Three modes come from the same pipeline: call mode, where you keep talking while the agent works on the other
          monitor; game-safe, where the host only watches and never sends input into a game; and per-device settings,
          because the keybind and the mic belong to a machine.
        </p>
        <LiveSurface id="four-layers-voice" tree={VOICE_SURFACE} still={STILLS.voice} caption="Draft of the voice pipeline. Whistle and Needle run on your device; the Assistant agent runs wherever your team agent runs." />
      </section>

      <section id="closing">
        <h2>Where this ends up</h2>
        <p>
          Put together: a renderer that paints what an agent describes, a desktop made of the same files, hands that
          work while you watch, and a voice that turns what you say into a message. The closing piece is the loop that
          runs without you: an agent that works a board's issues through threads, releases, tests the result with
          computer use on real devices and files what it finds, with drafts, as the next round of issues
          (<IssueLink id="NEW-11" /> <StatusChip status="planned" />). The first slice of that is a caretaker: one
          cheap session per device that triages and proposes. The rest is the four layers above, in order.
        </p>
        <Callout kind="note" title="Follow along">
          Every item here is an issue on a public board, and the renderer is on npm, crates.io and the package
          registries under <code>exponential-at</code>. The guides at{` `}
          <a href="https://ui.exponential.at/">ui.exponential.at</a> show how to paint a surface in React, Solid,
          SwiftUI, Compose and gpui.
        </Callout>
      </section>
    </>
  )
}

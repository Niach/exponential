/* Article 2 (EXP-1368): what a personal assistant should be able to do, in
   four abilities, and how much of each exists today. An outlook written as
   learning material; the three drafts are A2UI surfaces (surfaces.ts)
   painted live by the Exponential UI React renderer, their stills captured
   from that render. */
import { Callout, IssueLink, LiveSurface, Shot, ShotRow, StatusChip } from "../components/blog"
import { STACK_SURFACE, VOICE_SURFACE, WATCH_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  stack: { src: `/blog/assistant-stack.webp`, alt: `The four abilities as cards: it shows, it shapes its environment, it has hands you can watch, you can talk to it`, width: 1392, height: 1796 },
  watch: { src: `/blog/assistant-watch.webp`, alt: `Draft of a watch view: the flow as trace lines, the live window with a payouts table, the step graph and the replay`, width: 1392, height: 1582 },
  voice: { src: `/blog/assistant-voice.webp`, alt: `Draft of a voice pipeline: mic and desktop audio, local speech-to-text, a small router model, the assistant, the spoken reply`, width: 1392, height: 1118 },
}

export const meta: PostMeta = {
  slug: `what-an-assistant-should-do`,
  title: `What a personal assistant should be able to do`,
  description: `Show you things instead of describing them. Shape the computer it runs on. Work with its hands while you watch. Listen and talk back. Four abilities, what each one needs, and how much of it exists today.`,
  date: `2026-10-23`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: STILLS.stack,
  readingMinutes: 6,
  ogImage: `/og/og-blog-what-an-assistant-should-do.png`,
  sources: [`src/blog/what-an-assistant-should-do.tsx`, `src/blog/surfaces.ts`, `src/components/blog`, `public/blog`, `../../shots/exponential-ui-demo`],
}

export function Post() {
  return (
    <>
      <section id="four-abilities">
        <h2>Four abilities</h2>
        <p>
          Most assistants today are a text box. You type, they answer in prose, and when the task touches the real
          world they hand you a list of steps. The gap between that and an assistant you would hand things to is not
          intelligence. It is four abilities the surrounding software has to provide: a way to show rather than tell,
          a say over the environment it runs in, hands you can watch, and a voice. This post goes through them in
          order. For each one: what it needs, what exists today, and what is still a draft. The drafts in this post
          are marked as such; they are drawn with the renderer from the first section, which is part of the point.
        </p>
        <LiveSurface id="assistant-stack" tree={STACK_SURFACE} still={STILLS.stack} caption="The four abilities, each with its state and the issues behind it." />
      </section>

      <section id="show">
        <h2>It shows, instead of telling <StatusChip status="shipped" /></h2>
        <p>
          Ask an assistant how last night's release went and the right answer is a card: a status, two numbers, a
          chart, two buttons. Text can carry that, but a screen carries it better, and an assistant that can only
          write paragraphs will describe a table instead of drawing one.
        </p>
        <p>
          What this needs is a way for a model to describe a screen that a client can paint natively. A2UI is one
          such protocol: the agent streams a surface as components with props and a small data model, and a renderer
          paints it. The pieces to get right are a fixed catalog (so the model cannot invent widgets), themes (so the
          surface wears the host's look), and native painters (so a phone gets a phone screen, not a web view). The
          catalog matters more than it sounds: with about seventy components that cover rows, cards, tables, charts,
          forms, traces and overlays, a model can draw almost any product screen and nothing outside the design
          system.
        </p>
        <p>
          This exists. Exponential UI (<IssueLink id="VAPP-84" />) is an open renderer set for A2UI with one Rust
          core for layout and painters for React, Solid, a web component, SwiftUI, Compose and gpui, each passing the
          same conformance suite. First paint of a 100,000-row list is 70 ms in the core and under 170 ms in every
          painter; a benchmark suite with real-device numbers is in progress (<IssueLink id="VAPP-105" />). The step
          after rendering is apps without code: a package of templates and bindings onto live data that any client
          renders (<IssueLink id="VAPP-110" /> <StatusChip status="in-progress" />).
        </p>
        <Shot view="exponential-ui-demo" platform="web" caption="One surface, the answer to a release question, painted on the web." />
        <ShotRow>
          <Shot view="exponential-ui-demo" platform="ios" caption="The same surface in SwiftUI" />
          <Shot view="exponential-ui-demo" platform="android" caption="Compose" />
        </ShotRow>
      </section>

      <section id="environment">
        <h2>It shapes its environment <StatusChip status="planned" /></h2>
        <p>
          If a renderer can paint any screen from a description, the screens around the assistant can be
          descriptions too. A desktop environment is a panel, a launcher, a settings screen and a file view; written as
          templates in files, they are something the assistant can read and rewrite. "Put the clock on the left and
          hide the launcher while I present" becomes an edit to a document, applied live, and reverted the same way.
        </p>
        <p>
          The feasible path is a desktop environment painted by the same renderer, with the templates on disk and
          existing applications running beside it unchanged. The plan starts on Linux Mint, where the computer use
          below already works, with a Nix-based system as the long-term goal: there the assistant installs software
          and changes the system by writing configuration (<IssueLink id="NEW-10" />, the shell architecture in{` `}
          <IssueLink id="VAPP-64" />). None of this exists yet beyond the renderer it would be made of.
        </p>
      </section>

      <section id="hands">
        <h2>It has hands you can watch <StatusChip status="in-progress" /></h2>
        <p>
          Some tasks have no API. Exporting last month's payouts from a dashboard, filling a form in a desktop app,
          clicking through an installer. An assistant needs to drive the screen for those, and the person needs to
          see what it does, because a wrong click costs money.
        </p>
        <p>
          The first half exists. A session can see the screen and drive it on Linux, macOS and Windows
          (<IssueLink id="EXP-1196" />), in the foreground, with the person's keyboard and mouse held while it works
          and Esc to take them back at once (<IssueLink id="EXP-1273" />). A background mode on a third-party driver,
          with a second pointer, was tried and removed: too many platform-specific holes for what it gave.
        </p>
        <p>
          The second half is a draft. The order of operations is the same on every platform: the accessibility tree
          first (set a value, press a button, no pixels), window-routed input next, the real pointer only on request.
          Every run is recorded as a flow and the flow is an A2UI replay (<IssueLink id="VAPP-81" />): a timestamped
          list of surface updates, a few kilobytes, drawn in the viewer's theme, scrubbable, never a video. The view
          for it (<IssueLink id="EXP-1369" />) puts the flow on the left with how each step was done, the live window
          on the right, a step graph and the replay below, and a spoken caption for when you are not looking.
        </p>
        <LiveSurface id="assistant-watch" tree={WATCH_SURFACE} still={STILLS.watch} caption="Draft of the watch view. The agent works on a second monitor; the person sees the flow, the window and the replay." />
      </section>

      <section id="voice">
        <h2>You can talk to it <StatusChip status="planned" /></h2>
        <p>
          Talking is the cheapest way to hand something over, and the one that works while your hands are busy. It
          only works if the first steps are local and fast: no round trip to decide whether "pause" was a command.
        </p>
        <p>
          Two small models make this feasible on a CPU today, measured on October 10 (<IssueLink id="NEW-9" />).
          Whistle is speech-to-text in 16.9 MB: 16 kHz mono, one pass of up to thirty seconds, word timestamps, seven
          languages. Needle 3 is a text model of 8 to 29 MB that calls tools with a confidence head: it answers a
          request with the tool calls to make, asks when unsure, and returns an empty list for something off-topic
          instead of guessing. The pipeline: a push-to-talk key or wake word opens the mic, desktop audio is captured
          per application so a call is its own channel, transcripts merge by timestamp, and the router decides in
          about 50 milliseconds whether to act, confirm, or hand the text to the full assistant as a message. The
          reply is spoken and shown as a caption. Call mode keeps the mic open while the assistant works elsewhere;
          game-safe mode watches and never sends input into a game; keybinds and microphones belong to a machine.
        </p>
        <LiveSurface id="assistant-voice" tree={VOICE_SURFACE} still={STILLS.voice} caption="Draft of the voice pipeline. The first three stages run on the device; the assistant runs wherever it runs." />
      </section>

      <section id="while-you-sleep">
        <h2>It works while you are away <StatusChip status="planned" /></h2>
        <p>
          With the four abilities in place the last step is time. An assistant that can show, act and ask can also
          run a loop on its own: take the open work, build it in threads, release, test the result on real devices
          with the hands above, and file what it finds as the next round, with drafts attached. The first slice of
          that is a caretaker: one cheap session per device that triages and proposes, and asks before it builds
          (<IssueLink id="NEW-11" />).
        </p>
      </section>

      <section id="how-feasible">
        <h2>How feasible, today</h2>
        <ul>
          <li>
            <strong>Showing:</strong> exists. Open renderers for A2UI on six platforms, one conformance suite, numbers
            on the way.
          </li>
          <li>
            <strong>Shaping the environment:</strong> feasible with the same renderer; not started.
          </li>
          <li>
            <strong>Hands:</strong> foreground computer use exists on three operating systems; background work and
            the watch view are drafts.
          </li>
          <li>
            <strong>Voice:</strong> the models exist and run on a CPU; the pipeline is a plan.
          </li>
          <li>
            <strong>The loop:</strong> a plan, with the caretaker as its first slice.
          </li>
        </ul>
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

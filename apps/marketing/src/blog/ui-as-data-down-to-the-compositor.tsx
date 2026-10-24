/* Article 2 (EXP-1369, replacing EXP-1368's draft): UI as data, in four
   layers. The screen drafts are drawn in HTML and CSS with CSS-only motion
   (components/blog/concepts); the one live surface is an A2UI tree
   (surfaces.ts) painted by the Exponential UI React renderer; the captures
   are real shots from the store. Written to describe, not to sell: every
   claim names what exists and what is a drawing. */
import { Callout, CompositorTree, ConceptFigure, DesktopConcept, LiveSurface, Section, Shot, ShotRow, StatusChip, WatchView } from "../components/blog"
import { EXPENSE_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  form: { src: `/blog/ui-as-data-form.webp`, alt: `An expense form painted from nine components: amount, category, date, a receipt upload, cancel and save`, width: 1392, height: 1168 },
  watch: { src: `/blog/watch-view.webp`, alt: `Draft of the watch view: the live Firefox window with the agent's cursor on Export CSV, a line of seven steps and the agent's spoken caption`, width: 1392, height: 782 },
}

export const meta: PostMeta = {
  slug: `ui-as-data-down-to-the-compositor`,
  title: `UI as data, down to the compositor`,
  description: `A screen described as data and painted natively is one idea with four layers: the card an agent answers with, a desktop whose panels are that data on disk, a compositor whose tree an agent can address, and a view for watching it work. What exists of each, and what is a drawing.`,
  date: `2026-10-24`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: STILLS.watch,
  readingMinutes: 5,
  ogImage: `/og/og-blog-ui-as-data-down-to-the-compositor.png`,
  sources: [`src/blog/ui-as-data-down-to-the-compositor.tsx`, `src/blog/surfaces.ts`, `src/components/blog`, `src/styles/blog-concepts.css`, `public/blog`, `../../shots/exponential-ui-demo`],
}

export function Post() {
  return (
    <>
      <Section id="data" num="01" label="Data">
        <h2>A screen as data <StatusChip status="shipped" /></h2>
        <p>
          A2UI is an open wire format for user interfaces, started at Google: a flat list of components from a
          fixed catalog, a data model they bind to, and the actions they send back. A language model can write one,
          because the catalog fits in a prompt, and a renderer paints it with each platform's own toolkit: SwiftUI,
          Compose, gpui, the DOM. Exponential UI is one implementation. A Rust core
          lays out the tree, three native painters draw it, and the React, Solid and web-component renderers pass
          the same conformance replays. A theme is a JSON file loaded at runtime.
        </p>
        <p>
          The form below is painted in this page by the React renderer from nine components. It returns in the
          next two sections, as a window and as a branch of a tree.
        </p>
        <LiveSurface id="ui-as-data-form" tree={EXPENSE_SURFACE} still={STILLS.form} caption="Nine components, painted live by the React renderer from the JSON in this page." />
        <Shot view="exponential-ui-demo" platform="desktop" caption="A release card from the renderer's demo, painted by gpui in the desktop app." />
        <ShotRow>
          <Shot view="exponential-ui-demo" platform="ios" caption="The same surface in SwiftUI" />
          <Shot view="exponential-ui-demo" platform="android" caption="Compose" />
        </ShotRow>
      </Section>

      <Section id="desktop" num="02" label="Desktop">
        <h2>A desktop made of the same data <StatusChip status="planned" /></h2>
        <p>
          If a card can be data, so can a panel. The desktop environment we are planning keeps its panel, launcher, settings and file view as surfaces on disk, painted
          by the same renderer, with existing applications running beside them as ordinary Wayland and X clients.
        </p>
        <ul>
          <li>
            <strong>Every painted part can be changed while it is shown.</strong> Moving the clock or adding a tray
            item is an edit to a file, by a person or by an agent.
          </li>
          <li>
            <strong>Agents work in the background, several at once,</strong> each on its own workspace; how their
            input is kept apart from yours is the next section.
          </li>
          <li>
            <strong>UI is built when asked.</strong> A request becomes a window whose content the agent streams as a
            surface, usable while it is still being written.
          </li>
        </ul>
        <p>
          Voice sits in the tray, one key away: push-to-talk or a wake word, local speech-to-text, short commands
          acted on at once, everything else sent to the assistant session and answered aloud. Today, computer use
          runs in the foreground on Linux (X11), macOS and Windows, with your input held and Esc to take it back; a
          second pointer on X11 was tried and removed after it fought the person's cursor.
        </p>
        <ConceptFigure id="desktop" still="desktop-concept" caption="Draft of the desktop: two agents in their own windows with their own seats, the expense form streaming in from a request, the mic in the tray.">
          <DesktopConcept />
        </ConceptFigure>
      </Section>

      <Section id="tree" num="03" label="Tree">
        <h2>The compositor's tree is the API <StatusChip status="planned" /></h2>
        <p>
          Every system draws from a tree; what differs is how deep an agent may reach into it. macOS gives the
          accessibility tree where an application exposes one and pixels where it does not, and Windows gives UI
          Automation. X11 gives pixels, synthetic input and AT-SPI where the toolkit supports it; Wayland keeps
          AT-SPI but routes capture and input through portals, one permission prompt at a time.
        </p>
        <p>
          A compositor written for this owns one tree from the outputs down: outputs, workspaces, toplevels,
          surfaces, and inside a painted window the components themselves, each with an id, a role, a value and its
          actions. An agent addresses a node by path and acts on it. A native window is pixels below its surface,
          but the desktop is the parent of every process, so it switches on whatever tree the toolkit offers at
          launch: AT-SPI for GTK, Qt, Firefox and Java, the accessibility flag or the DevTools protocol for Electron,
          an in-process probe for Qt. macOS needs private APIs and entitlements for the same reach; here it is an
          environment variable. The compositor joins those subtrees into one tree, with per-seat input and
          per-window capture: an agent's seat never moves your pointer, and a covered window still captures.
        </p>
        <p>
          The risks are plain: a tree is only as good as the app's labels, canvas apps and games stay pixels,
          multi-seat is poorly exercised by toolkits, X clients through Xwayland see one seat, and a compositor is
          years of edge cases, which argues for a library such as Smithay rather than a blank page.
        </p>
        <ConceptFigure id="tree" still="compositor-tree" caption="The tree an agent addresses: a native window gives pixels plus the tree its toolkit offers, a painted window continues to its components.">
          <CompositorTree />
        </ConceptFigure>
      </Section>

      <Section id="watch" num="04" label="Watch">
        <h2>Watching an agent work <StatusChip status="planned" /></h2>
        <p>
          Background work needs a window into it, or it is a trust exercise. The draft below is the watch view for
          one task: the live window as a per-window capture, the agent's cursor drawn by the viewer rather than by
          the operating system, the target outlined before the click lands, and under it the steps with how each
          was done. Which windows an agent may capture or drive is a per-window grant; the game in the tree above
          is watch-only.
        </p>
        <p>
          A step the agent marks as irreversible, or that matches a rule you set (payments, deletes), waits for
          confirmation. The step line and the agent's messages are recorded as timestamped A2UI messages, a few
          kilobytes you can scrub; the window capture itself is video, or a keyframe per
          step. The spoken caption is the same line the tray reads aloud.
        </p>
        <ConceptFigure id="watch" still="watch-view" caption="Draft of the watch view: the live window with the agent's cursor and its target, the step line, the spoken caption.">
          <WatchView />
        </ConceptFigure>
      </Section>

      <Section id="status" num="05" label="Status">
        <h2>What exists</h2>
        <ul>
          <li><strong>A screen as data:</strong> shipped. Six renderers, one conformance suite, themes as files.</li>
          <li><strong>The desktop:</strong> planned; nothing exists beyond the renderer it would use.</li>
          <li><strong>The compositor:</strong> planned; the tree above is a draft.</li>
          <li><strong>The watch view:</strong> a draft; the background path waits for the compositor.</li>
          <li><strong>Voice:</strong> planned. Speech-to-text and an intent router small enough for a CPU exist as open models; nothing is wired up.</li>
        </ul>
        <Callout kind="note" title="Sources">
          Each item is an issue on a public board, linked where it is named. The renderer's source and docs are at{` `}
          <a href="https://ui.exponential.at/">ui.exponential.at</a>; the first layer needs none of the other three.
        </Callout>
      </Section>
    </>
  )
}

/* Article 2 (EXP-1369, replacing EXP-1368's draft): UI as data, in four
   layers. The screen drafts are drawn in HTML and CSS with CSS-only motion
   (components/blog/concepts); the one live surface is an A2UI tree
   (surfaces.ts) painted by the Exponential UI React renderer; the captures
   are real shots from the store. Written to describe, not to sell: every
   claim names what exists and what is a drawing. */
import { Callout, CompositorTree, ConceptFigure, DesktopConcept, IssueLink, LiveSurface, Section, Shot, ShotRow, StatusChip, WatchView } from "../components/blog"
import { EXPENSE_SURFACE } from "./surfaces"
import type { PostMeta } from "./types"

/* public/blog/<id>.webp, photographed by scripts/blog-stills.ts at 2x. */
export const STILLS = {
  form: { src: `/blog/ui-as-data-form.webp`, alt: `An expense form painted from nine components: amount, category, date, a receipt upload, cancel and save`, width: 1392, height: 1168 },
  watch: { src: `/blog/watch-view.webp`, alt: `Draft of the watch view: the flow of seven steps, the live Firefox window with the agent's cursor on Export CSV, the step graph and the A2UI replay`, width: 1392, height: 944 },
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
          A2UI is a wire format for user interfaces. A surface is a flat list of components from a fixed catalog, a
          data model the components bind to, and the actions the surface sends back. A language model can write one,
          because the catalog is small enough to teach in a prompt, and a renderer paints it with the platform's own
          views rather than a webview. The implementation this post refers to is Exponential UI
          (<IssueLink id="VAPP-84" />): one Rust core lays out the tree, painters for SwiftUI, Compose and gpui draw
          it, and the React, Solid and web-component renderers match the same frames. A conformance suite of 24
          suites and about 2,000 cases replays on every renderer, and a theme is one JSON file of tokens and recipes
          loaded at runtime.
        </p>
        <p>
          The form below is painted in this page by the React renderer from the nine components it carries. The
          same form returns in the next two sections, as a window and as a branch of a tree.
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
          If a card can be data, so can a panel. The desktop environment in planning (<IssueLink id="NEW-10" />,{` `}
          <IssueLink id="VAPP-64" />) keeps its panel, launcher, settings, notifications and file view as surfaces on
          disk, painted by the same renderer, with existing applications running beside them as ordinary Wayland and
          X clients. Three things follow from that choice.
        </p>
        <ul>
          <li>
            <strong>Anything can be changed while it is shown.</strong> Moving the clock, adding a tray item or
            restyling every window is an edit to a file, by a person or by an agent, and a theme is one file more.
          </li>
          <li>
            <strong>Agents work in the background, several at once.</strong> Each gets a workspace on an output of
            your choice and its own seat, so your pointer and keyboard are never borrowed and a second agent is a
            second seat, not a conflict.
          </li>
          <li>
            <strong>UI is built when asked.</strong> A request produces a window whose content the agent streams as a
            surface, usable while it is still being written and saved as a file like everything else.
          </li>
        </ul>
        <p>
          Voice lives in the tray, because an assistant that has to be opened is not always there. Push-to-talk or
          a wake word opens the mic, speech-to-text and a small intent router run on the CPU
          (<IssueLink id="NEW-9" />), short commands act at once, and whatever the router cannot settle becomes a
          message to the assistant session, whose reply is spoken back.
        </p>
        <ConceptFigure id="desktop" still="desktop-concept" caption="Draft of the desktop: two agents in their own windows with their own seats, the expense form streaming in from a request, the mic in the tray.">
          <DesktopConcept />
        </ConceptFigure>
      </Section>

      <Section id="tree" num="03" label="Tree">
        <h2>The compositor's tree is the API <StatusChip status="planned" /></h2>
        <p>
          Every system draws from a tree; what differs is how deep an agent may reach into it. On macOS an agent
          gets the accessibility tree where an application exposes one and pixels where it does not. Windows offers
          UI Automation. X11 offers pixels and synthetic input, and a stock Wayland compositor offers the
          accessibility bus only, with no injected input outside a portal. A second pointer on X11 was tried here and
          removed after it fought the person's cursor (<IssueLink id="EXP-1273" />), so today's computer use runs in
          the foreground, with your input held and Esc to take it back (<IssueLink id="EXP-1196" />).
        </p>
        <p>
          A compositor written for this owns the tree from the outputs down: outputs, workspaces, toplevels,
          surfaces, and inside a painted window the components themselves, each with an id, a role, a value and the
          actions it accepts. An agent addresses a node by path and acts on it: set this field, press this button,
          read this table. Input reaches a window through a seat that belongs to the agent, so it never moves your
          pointer, and capture is per window, which works when the window is covered or on a screen you are not
          looking at. The limits are plain: a native application stays opaque below its surface unless it speaks
          the accessibility bus, and a compositor is years of edge cases that existing ones have already paid for,
          which argues for building on a library such as Smithay rather than from nothing.
        </p>
        <ConceptFigure id="tree" still="compositor-tree" caption="The tree an agent addresses: a native window ends at its surface, a painted window continues to its components.">
          <CompositorTree />
        </ConceptFigure>
      </Section>

      <Section id="watch" num="04" label="Watch">
        <h2>Watching an agent work <StatusChip status="planned" /></h2>
        <p>
          Background work needs a window into it, or it is a trust exercise. The draft below is the watch view for
          one task (<IssueLink id="EXP-1369" />). The flow lists each step with how it was done: a value set through
          the accessibility tree, a click routed to the window, or the foreground taken on purpose; a step that
          moves money or deletes something waits for a confirmation. The live window is a per-window capture; the
          violet cursor is not an operating-system cursor but the viewer's drawing of the agent's seat, and the
          target element is outlined before the click lands.
        </p>
        <p>
          Under it sit the step graph and a replay. The run is recorded as timestamped A2UI messages, a few
          kilobytes that scrub like a video and are painted by the renderer in the viewer's own theme
          (<IssueLink id="VAPP-81" />). Three controls are the whole contract: Watching, Pause agent, Take over. The
          small spoken-caption card can stay over a fullscreen game while the agent works on another screen.
        </p>
        <ConceptFigure id="watch" still="watch-view" caption="Draft of the watch view: the flow, the live window with the agent's cursor and its target, the step graph, the replay, the spoken caption.">
          <WatchView />
        </ConceptFigure>
      </Section>

      <Section id="status" num="05" label="Status">
        <h2>What exists</h2>
        <ul>
          <li><strong>A screen as data:</strong> shipped. Six renderers, one conformance suite, themes as files.</li>
          <li><strong>The desktop:</strong> a plan. Nothing exists beyond the renderer it would use.</li>
          <li><strong>The compositor:</strong> a plan. The tree above is the proposal, not a measurement.</li>
          <li><strong>The watch view:</strong> a drawing in this post. Foreground computer use is shipped on Linux (X11), macOS and Windows; the background path waits for the compositor.</li>
          <li><strong>Voice:</strong> the two models run on a CPU; the pipeline is a plan.</li>
        </ul>
        <Callout kind="note" title="Sources">
          Each item above is an issue on a public board, linked where it is named. The renderer's source and its
          docs are at <a href="https://ui.exponential.at/">ui.exponential.at</a>; the catalog and the A2UI format are
          open, so the first layer needs none of the other three.
        </Callout>
      </Section>
    </>
  )
}

/* Article 1 (EXP-1367): the session as the interface. Written like a Guide:
   numbered sections, two or three sentences, a real capture each. */
import { Callout, Section, Shot, ShotRow } from "../components/blog"
import { shotSrc } from "../components/DocShot"
import type { PostMeta } from "./types"

const COVER = shotSrc(`script-card`, `web`)

export const meta: PostMeta = {
  slug: `session-as-interface`,
  title: `The session as the interface`,
  description: `The tracker's MCP server runs inside the coding agent's session, and the session view knows the tools: a call is drawn by its kind, an issue reference is a chip, a surface is painted natively, and sessions message each other.`,
  date: `2026-10-23`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: { src: COVER.src, alt: `A session feed: a code-mode script card, a comment written through MCP, the queue`, width: COVER.frame.w, height: COVER.frame.h },
  readingMinutes: 3,
  ogImage: `/og/og-blog-session-as-interface.png`,
  sources: [`src/blog/session-as-interface.tsx`, `src/components/blog`, `../../shots/script-card`, `../../shots/composer-menu`, `../../shots/trace`, `../../shots/thread`, `../../shots/plan-card`, `../../shots/story-viewer`, `../../shots/ask-dock`, `../../shots/thread-replies`, `../../shots/agent-channel`],
}

export function Post() {
  return (
    <>
      <Section id="setup" num="01" label="Setup">
        <h2>The tracker's MCP inside the session</h2>
        <p>
          Claude Code or Codex starts with the tracker's MCP server in its config and a header that names the
          session (a thread is the top session of a tree). Every call the agent makes arrives as that session: it reads its issue, comments, links a pull
          request, asks a question or messages another session. The session view knows these tools and draws each
          result by its kind.
        </p>
        <Shot view="composer-menu" platform="web" caption="The composer's + menu: the issue to implement, the team's MCP servers, computer use." />
      </Section>

      <Section id="cards" num="02" label="Cards">
        <h2>Tool calls drawn by kind</h2>
        <p>
          File reads, searches and commands fold into one-line trace entries; a failing test run turns red and opens
          onto its output. A comment written through MCP is drawn as the comment itself, and a code-mode script as
          one card with its calls grouped by tool. The engine tags
          every call with a kind before it reaches a client, so all four platforms draw the same card.
        </p>
        <Shot view="trace" platform="web" caption="Trace lines: a command with its output, a sub-agent, a failed test run, the task list." />
      </Section>

      <Section id="chips" num="03" label="Chips">
        <h2>Issue references with previews</h2>
        <p>
          Any issue identifier in a message, a comment or a tool result is a chip: status glyph, identifier, title.
          Hover shows the assignee, status, priority and labels from data the client already has; a click opens the
          issue in the nearest view that shows it. Tool results return references by kind and id, not URLs, and
          each client resolves them from its own rows.
        </p>
        <Shot view="thread" platform="web" caption="Issue chips in the tab strip, in the agent's text and in a child's report." />
      </Section>

      <Section id="images" num="04" label="Images">
        <h2>Pictures inline in both directions</h2>
        <p>
          The agent uploads a screenshot through MCP and references it in markdown; the message, comment or plan
          renders it inline on every platform. When the agent reads an attachment back, the inline copy is capped at
          about 300 KB and 1280 px, because one full phone screenshot costs tens of thousands of tokens.
        </p>
      </Section>

      <Section id="surfaces" num="05" label="Surfaces">
        <h2>The agent draws its own UI</h2>
        <p>
          A plan or a question can carry an A2UI surface: the agent describes a screen as JSON and the card paints it
          with the Exponential UI renderer, native on web, iPhone, Android and the desktop app. On the native platforms there is no webview in between, on the web the
          renderer emits DOM, and what the person sets in the surface travels back with the answer.
        </p>
        <Shot view="plan-card" platform="web" caption="A plan card: a picture, the agent's surface as a live preview, a choice question." />
        <ShotRow>
          <Shot view="plan-card" platform="ios" caption="The same card and surface on iPhone" />
          <Shot view="plan-card" platform="android" caption="Android" />
        </ShotRow>
      </Section>

      <Section id="stories" num="06" label="Stories">
        <h2>Stories carry the results</h2>
        <p>
          What a session wants a person to see lands as a story: a result with its screenshots, a question or a
          plan. Each agent and each chat wears a ring on top of Chats; a tap opens its stories, what waits on the
          person first, and a question is answered right there. The pull request body is a short summary plus the
          story pictures of each platform; opening a PR with a UI change and no picture is refused.
        </p>
        <Shot view="story-viewer" platform="web" caption="A story: the agent's result with its screenshot, one segment per story on top." />
      </Section>

      <Section id="notifications" num="07" label="Notifications">
        <h2>A question waits at the composer</h2>
        <p>
          When the agent asks, the question does not scroll away in the transcript: it waits as a card at the
          composer, with its options and a text field, and several can wait at once. The thread moves under Needs
          you, the inbox gets the entry, the phones get the push, and an @email mention notifies that person. The agent can
          also offer quick replies, which send on one tap.
        </p>
        <Shot view="ask-dock" platform="web" caption="Waiting plans and a question at the composer." />
        <Shot view="thread-replies" platform="web" caption="Quick replies above the composer." />
      </Section>

      <Section id="bus" num="08" label="Bus">
        <h2>Sessions message each other</h2>
        <p>
          One tool, <code>sessions.send</code>, messages a session or a team agent, or starts a child. The server
          posts every child transition into the parent as a message, and a message is what wakes a session, so
          nothing polls. A team agent is a session with a longer life: a channel that
          triages, reports and starts threads.
        </p>
        <Shot view="agent-channel" platform="web" caption="A team agent's channel: its reports, a request, the thread it started as one card." />
      </Section>

      <Section id="pattern" num="09" label="Pattern">
        <h2>Four decisions</h2>
        <ul>
          <li><strong>One API for people and agents.</strong> The MCP tools are the product's own operations; the session identity rides on every call.</li>
          <li><strong>Results carry ids, not markup.</strong> Clients resolve references from local state and draw them in their own idiom.</li>
          <li><strong>Render by kind.</strong> A comment is a comment, a surface is painted natively, everything else is a trace line.</li>
          <li><strong>Messages are the only wake-up.</strong> A session runs when a message arrives and sleeps otherwise.</li>
        </ul>
        <Callout kind="note" title="Try it">
          The MCP server, the session view and the renderer are open source under Apache-2.0: the{` `}
          <a href="/docs/mcp/">MCP docs</a>, the <a href="/docs/coding/">coding docs</a> and{` `}
          <a href="/ui/">exponential.at/ui</a>.
        </Callout>
      </Section>
    </>
  )
}

/* Article 1 (EXP-1367): the session as the interface. What a coding agent
   can do once its tracker's own MCP server runs inside its session and the
   session view understands the tool schema. Learning material first; the
   pictures are real captures from the committed store. */
import { Callout, IssueLink, Shot } from "../components/blog"
import { shotSrc } from "../components/DocShot"
import type { PostMeta } from "./types"

const COVER = shotSrc(`script-card`, `web`)

export const meta: PostMeta = {
  slug: `session-as-interface`,
  title: `The session as the interface`,
  description: `Give a coding agent its tracker's own MCP server and a session view that understands the tool schema. Tool calls become cards, issue references become chips with previews, images sit inline, the Guide becomes the pull request, and sessions message each other.`,
  date: `2026-10-23`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: { src: COVER.src, alt: `A session feed: a workflow card, a code-mode script, a comment written through MCP, the queue`, width: COVER.frame.w, height: COVER.frame.h },
  readingMinutes: 6,
  ogImage: `/og/og-blog-session-as-interface.png`,
  sources: [`src/blog/session-as-interface.tsx`, `src/components/blog`, `../../shots/script-card`, `../../shots/trace`, `../../shots/thread`, `../../shots/plan-card`, `../../shots/guide`, `../../shots/ask-dock`, `../../shots/thread-replies`, `../../shots/agent-channel`, `../../shots/threads`],
}

export function Post() {
  return (
    <>
      <section id="two-things">
        <h2>Two things, wired together</h2>
        <p>
          A coding agent such as Claude Code or Codex is a terminal program. It reads files, runs commands and calls
          tools over MCP. On its own it knows nothing about the tracker the work came from, and the person watching
          it sees a scrolling transcript.
        </p>
        <p>
          Two changes turn that into something else. First, the tracker's own MCP server runs inside the session:
          the launcher writes the MCP config with a header that names the session, so every call the agent makes
          arrives as that session. The agent can read the issue it works on, comment on it, link a pull request, ask
          the person a question and message another session, with the same fifteen tools a human-facing client would
          use. Second, the session view understands those tools. A tool result is not a blob of JSON to it; it is a
          comment, an issue, a script, a question. It draws each one as the thing it is.
        </p>
        <p>
          This post walks through what that combination makes possible, using Exponential as the worked example.
          None of it depends on a particular agent, and most of it transfers to any product that has an API worth
          giving an agent.
        </p>
      </section>

      <section id="calls-as-cards">
        <h2>Tool calls drawn as what they are</h2>
        <p>
          The feed of a session is a list of tool calls, and most of them are boring: a file read, a search, a test
          run. Those collapse into one-line trace entries with an icon, a title and a muted detail; a failing test run
          turns red and opens onto its output. The calls that matter get a card. A comment the agent writes through
          MCP is drawn as the real comment, with the agent's mark and a "via MCP" tag, and clicking it opens the issue
          scrolled to that comment (<IssueLink id="EXP-1241" />). A code-mode script, where the agent batches many tool
          calls into one program, becomes one card listing the calls grouped by tool with the failures marked
          (<IssueLink id="EXP-1279" />). A workflow of sub-agents shows its phases and lanes. The queue of what is left
          sits at the bottom of the transcript.
        </p>
        <p>
          The rule behind it: the engine on the device tags every tool call with a kind from the contract before it
          reaches any client, so the web app, the desktop app, iPhone and Android all draw the same card from the same
          data.
        </p>
        <Shot view="trace" platform="web" caption="Trace lines: a command with its output, a sub-agent, a failed test run, the queue." />
      </section>

      <section id="chips">
        <h2>Issue references are chips with previews</h2>
        <p>
          Anywhere an issue identifier appears, in a message, a comment, a tool result or a plan, it is a chip: the
          status glyph, the mono identifier, the title. Hovering a chip shows a preview card with the assignee, the
          status, the priority and the labels, read from the data the client already has synced, so a preview never
          waits on the server. Clicking a chip opens the issue in the nearest place that holds it: a thread that
          already has the issue as a tab switches to that tab instead of leaving.
        </p>
        <p>
          The same works for every entity a tool can return. A tool result carries references by kind and id (issue,
          board, comment, session, device, member, repository) plus a display fallback, never a URL; each client
          resolves the target from its own rows (<IssueLink id="EXP-988" />). A list result yields one chip for the
          list and a few for its members, so hovering "12 issues" shows which twelve.
        </p>
        <Shot view="thread" platform="web" caption="Issue chips in a thread: in the tab strip, in the agent's text, in a child's report. Each one previews on hover." />
      </section>

      <section id="images">
        <h2>Images inline</h2>
        <p>
          An agent that takes screenshots needs somewhere to put them and a way to show them. Through MCP it uploads a
          file as an attachment and references it in markdown; the message, the comment or the plan renders the
          picture inline on every platform. The other direction matters as much: when an agent reads an attachment,
          the inline copy it receives is bounded to about 300 KB and 1280 px on the long edge (<IssueLink id="EXP-854" />),
          because one full-resolution phone screenshot is tens of thousands of tokens and has emptied a context before
          the agent read the issue.
        </p>
        <p>
          A plan card is where this pays off most. The agent researches, drafts the screen on its branch, photographs
          it and posts one card with the pictures inline, choice questions with picture tiles, and, if it wants, a live
          Exponential UI surface as a preview (<IssueLink id="EXP-1280" />). The person answers in the card.
        </p>
        <Shot view="plan-card" platform="web" caption="A plan card: a picture inline, a preview surface, the first of two questions." />
      </section>

      <section id="guide">
        <h2>The Guide is the pull request</h2>
        <p>
          A session closes out by writing a Guide through MCP: a Summary, then one topic per change with two or three
          sentences, the files it touched and a picture per platform. The Guide is a tab on the thread and the body of
          every pull request the thread opens. Opening a pull request with a UI change and no picture is refused by
          the tool, so the review starts from the pictures. Reviewing becomes reading.
        </p>
        <Shot view="guide" platform="web" caption="A thread's Guide: the stack of pull requests, the summary, one topic with its web and iPhone captures." />
      </section>

      <section id="notifications">
        <h2>Notifications that are the work</h2>
        <p>
          When an agent needs the person, it asks through a tool, and the question is not a line in a transcript. It
          takes the composer's place as a card with its options and a free-text field, in the session and in its
          thread alike; several questions stack and open one at a time. The thread list shows the thread under Needs
          you, the inbox gets the entry, and the phones get the push. An agent mentions a person by email to ping them
          and the mention lands in that person's inbox like any other.
        </p>
        <p>
          The reverse direction is just as cheap: an agent can suggest its likely next messages as quick replies above
          the composer. One tap sends the reply, even while the agent is still working.
        </p>
        <Shot view="ask-dock" platform="web" caption="The ask dock: one waiting plan as the big card, then two plans and a question stacked." />
        <Shot view="thread-replies" platform="web" caption="Quick replies above the composer, offered by the agent." />
      </section>

      <section id="sessions-talk">
        <h2>Sessions message each other</h2>
        <p>
          The last piece is that sessions are addressable. One tool, <code>sessions.send</code>, messages a session by
          id, a team agent by its id, or starts a child under a parent. The server posts every transition of a child
          into its parent's conversation as a message: the child's reply, its question, the pull request it opened,
          its end. A message is what wakes a session, so nothing polls and nothing waits; the rule for every agent is
          one sentence, end your turn and you are woken by messages (<IssueLink id="NEW-1" />). A session that depends
          on another names it and sleeps until it ends.
        </p>
        <p>
          Team agents are the same mechanism with a longer life: an agent definition runs as one conversation per
          member that never ends, like a channel. It triages, reports, and starts threads for work that needs code.
          The threads report back into the channel.
        </p>
        <Shot view="agent-channel" platform="web" caption="A team agent's channel: its reports, a request, the thread it started as one card." />
        <Shot view="threads" platform="web" caption="The thread list: what needs a person, what runs, what is recent, children under their parents." />
      </section>

      <section id="pattern">
        <h2>The pattern</h2>
        <p>If you want to build this for your own product, four decisions carry most of the weight:</p>
        <ul>
          <li>
            <strong>One API for people and agents.</strong> The MCP tools are the product's own operations, not a
            side door. The session identity rides on every call.
          </li>
          <li>
            <strong>Results carry ids, not markup.</strong> A tool returns references by kind and id with a display
            fallback. Each client resolves them from local state and draws them in its own idiom.
          </li>
          <li>
            <strong>Render by kind.</strong> A comment is a comment, a script is a script, a question is a card where
            the composer was. Everything else folds into a trace line.
          </li>
          <li>
            <strong>Messages are the only wake-up.</strong> Sessions address each other and sleep otherwise. The
            server, not the agent, turns events into messages.
          </li>
        </ul>
        <Callout kind="note" title="Try it">
          The MCP server, the session view and the contract behind the cards are open source under Apache-2.0. The{` `}
          <a href="/docs/mcp/">MCP docs</a> list the tools; the <a href="/docs/coding/">coding docs</a> show the
          session view in use.
        </Callout>
      </section>
    </>
  )
}

/* Article 1 (EXP-1367): where the product stands, story first. Every
   picture is a real capture from the committed store (shots/), every claim
   carries the issue or PR behind it. */
import { BeforeAfter, Callout, IssueLink, Shot, ShotRow, StatusChip } from "../components/blog"
import { shotSrc } from "../components/DocShot"
import { LINKS } from "../lib/links"
import type { PostMeta } from "./types"

const THREAD_WEB = shotSrc(`thread`, `web`)
const THREAD_DESKTOP = shotSrc(`thread`, `desktop`)

export const meta: PostMeta = {
  slug: `where-we-are`,
  title: `Where we are: humans and agents in one thread`,
  description: `Our agents kept stalling, so we rebuilt the session system in two steps. What came out: one thread per piece of work, team agents, plans you answer in a card, and a Guide that is the pull request.`,
  date: `2026-10-23`,
  author: { name: `Danny`, url: `https://github.com/Niach` },
  cover: { src: THREAD_WEB.src, alt: THREAD_WEB.title, width: THREAD_WEB.frame.w, height: THREAD_WEB.frame.h },
  readingMinutes: 6,
  ogImage: `/og/og-blog-where-we-are.png`,
  sources: [`src/blog/where-we-are.tsx`, `src/components/blog`, `../../shots/thread`, `../../shots/threads`, `../../shots/agent-channel`, `../../shots/plan-card`, `../../shots/guide`, `../../shots/steering`, `../../shots/script-card`],
}

export function Post() {
  return (
    <>
      <section id="the-honest-start">
        <h2>The honest start</h2>
        <p>
          In the middle of October our agents kept stalling. A thread would start, hand part of the work to a
          nested session, and then wait for an answer that never came. The dispatcher waited on the child, the
          child waited on the dispatcher, and a person had to open the right session and nudge it. The owner's own
          words in <IssueLink id="N-10" /> on October 21: not one thread made it to the end on its own, and we had
          not started anything new in a week.
        </p>
        <p>
          So we stopped and rebuilt the session system in two steps, with no compatibility layer. <IssueLink id="NEW-1" />{` `}
          (merged October 21) is the server half: one <code>sessions</code> table, the root session does the work
          itself, and the server posts every child transition into the parent's conversation as a message. A child's
          reply, its open question, its pull request, its end: each lands in the parent as the thing that wakes it.
          The rule in the runbook is one line: end your turn, you are woken by messages. A watchdog runs every minute
          and flags a session that sits queued for five minutes, idles on unread messages for two, or works for
          thirty without a sign of life. <IssueLink id="NEW-6" /> is the web and desktop half, and the phones followed
          in the October 22 release.
        </p>
        <Shot view="threads" platform="web" caption="The Threads list after the rebuild: what needs you, what runs, what is recent. One row per thread, its children nested under it." />
      </section>

      <section id="one-thread">
        <h2>One thread is the unit of work</h2>
        <p>
          The second thing we settled is what a thread is (<IssueLink id="EXP-1290" />, shipped with the October 14
          train as <IssueLink id="EXP-1291" />). A thread holds one or more sessions, on whatever devices they run,
          about the issues it links, with its pull requests and one Guide. The header is a title and one tab strip:
          Conversation, a chip per linked issue, a plus to add one, and the Guide. There is no session type to pick
          and no second entry point: typing in the composer, starting coding from an issue and running a saved prompt
          are the same start.
        </p>
        <p>
          It looks the same on the web, in the desktop app, on iPhone and on Android. The behaviour is locked in
          fixtures that all four clients replay, so a change edits the fixture first and the screens follow.
        </p>
        <BeforeAfter
          before={{ src: THREAD_WEB.src, alt: `${THREAD_WEB.title} on the web`, label: `web` }}
          after={{ src: THREAD_DESKTOP.src, alt: `${THREAD_DESKTOP.title} in the desktop app`, label: `desktop` }}
          width={THREAD_WEB.frame.w}
          height={THREAD_DESKTOP.frame.h}
          caption="The same thread on the web and in the desktop app. Drag to compare."
        />
        <ShotRow>
          <Shot view="thread" platform="ios" caption="iPhone" />
          <Shot view="thread" platform="android" caption="Android" />
        </ShotRow>
      </section>

      <section id="team-agents">
        <h2>Your assistant is a team agent</h2>
        <p>
          A team has agents the way it has boards. An agent is a definition: a name, an icon, instructions, the device
          it runs on and the model it uses (<IssueLink id="NEW-2" />). It runs as one conversation per member that
          never ends, so it behaves like a channel: you write to it, it reports back, and when something needs code it
          starts a thread instead of piling the work into the chat (<IssueLink id="NEW-3" />). Triggers fire from the
          server on a schedule or on team events, and the Agents page lists them as an events stream.
        </p>
        <p>
          The Home composer is still the one entry point. With no agent picked it starts a thread; with an agent
          picked the message goes into your conversation with it.
        </p>
        <Shot view="agent-channel" platform="web" caption="The Triage agent's channel: its own reports, a request from a person, the thread it started as one card, and the composer." />
        <ShotRow>
          <Shot view="agent-channel" platform="ios" caption="The same channel on iPhone" />
          <Shot view="agent-channel" platform="android" caption="Android" />
        </ShotRow>
      </section>

      <section id="agents-that-manage-agents">
        <h2>Agents that manage agents</h2>
        <p>
          A root session fans out only when the work calls for it. Each child reports at the end of every turn, and
          that report is a message in the parent, so the parent reads it and continues. When a child asks a question,
          the card is mirrored into the root under Needs you and delivered to the parent as a message; the first
          answer wins. Suggested follow-ups arrive as one card with Start, Options and Do it here, and a started one
          folds into a row with a live caption of what it is doing (<IssueLink id="EXP-1323" />, merged October 16).
        </p>
        <p>
          What is still missing is waiting you can see. A session that waits on CI or on another session reads as idle
          today. <IssueLink id="NEW-49" /> <StatusChip status="in-progress" /> moves the waiting to the server:
          a session names what it waits on, the row says Waiting on, and the bus wakes it when that thing happens.
        </p>
        <Shot view="thread" platform="web" caption="A thread with a child that needs you, three suggested tasks with one started, and a child's report folded into one line." />
      </section>

      <section id="plans-you-can-click">
        <h2>Plans you can click</h2>
        <p>
          We stopped using the agents' built-in plan modes. A plan is now one card (<IssueLink id="EXP-1280" />,
          merged October 18): the markdown with pictures inline, one block per question with text options or picture
          tiles, a free-text answer where it fits, and an Exponential UI surface the agent can draw into the card as a
          preview. Yes waits until every question is answered; No sends your note back and keeps planning. The card
          looks and works the same on the web, iPhone, Android and the desktop app.
        </p>
        <p>
          This article was planned with one. The agent read the open issues, proposed both articles, attached three
          drafts for the second one and asked four questions: which domain the blog launches on, how this article
          opens, which languages, and whether the watch-view draft should become a product issue of its own. The
          answers (exponential.at, story first, English only, yes) became the issues this post is built from.
        </p>
        <Shot view="plan-card" platform="web" caption="A plan waiting on you: a picture, a preview surface, and the first of two questions." />
        <ShotRow>
          <Shot view="plan-card" platform="ios" caption="The same card on iPhone" />
          <Shot view="plan-card" platform="android" caption="Android" />
        </ShotRow>
      </section>

      <section id="the-guide-is-the-pr">
        <h2>The Guide is the pull request</h2>
        <p>
          A thread writes one Guide: a Summary first, then one topic per change with two or three sentences, the files
          it touched and a picture per platform. That Guide is the body of every pull request the thread opens. The
          tool that opens a pull request refuses a UI change without a screenshot for each platform it touches, so by
          the time a PR exists, reviewing it means reading the Guide and looking at the pictures. The captures come
          from the same committed store this post uses.
        </p>
        <Shot view="guide" platform="web" caption="A thread's Guide tab: the stack of pull requests on top, the summary, one numbered topic with its web and iPhone captures." />
      </section>

      <section id="your-machines">
        <h2>Your machines, your agents</h2>
        <p>
          Agents run on your hardware, on your subscription. Claude Code or Codex starts in a git worktree on the
          desktop app or on a server running the <a href="/docs/cli/">CLI daemon</a>; a session can move to another of
          your online devices with its worktree and its conversation. From the phone you start a session, read its
          feed and answer its questions while it runs on the laptop at home.
        </p>
        <p>
          Bulk work goes through code mode (<IssueLink id="EXP-1236" />, merged October 7): instead of one tool call
          per item, the agent writes a short script that calls the tracker's MCP tools in parallel and gets one result
          back. The session shows it as one card: the calls grouped by tool, the failures in red, the script behind a
          disclosure.
        </p>
        <ShotRow>
          <Shot view="steering" platform="ios" caption="Steering a session from the iPhone" />
          <Shot view="steering" platform="android" caption="Android" />
        </ShotRow>
        <Shot view="script-card" platform="web" caption="A session feed: a workflow card, a code-mode script with 13 calls, a comment the agent wrote through MCP, and the queue of what is left." />
      </section>

      <section id="whats-next">
        <h2>What comes next</h2>
        <p>
          The agent can already use your computer. <IssueLink id="EXP-1196" /> gave sessions on Linux, macOS and
          Windows a screen to look at and a pointer to drive, served by the desktop app and the daemon. We then tried a
          background mode that worked in a second pointer while you kept yours (<IssueLink id="EXP-1217" />), and took
          it out again on October 20: the computer-use layer is now our own thin backend, foreground only, with your
          input held while the agent drives and Esc to take it back (<IssueLink id="EXP-1273" />).
        </p>
        <p>
          Getting the background mode back, with a view where you can watch what the agent does, is one of four layers
          we describe in the next post: a renderer that paints agent-written UI natively on every platform, a desktop
          built from that same JSON, computer use you can watch, and a voice in front of all of it.{` `}
          <a href="/blog/one-renderer-four-layers/">One renderer, four layers</a>.
        </p>
        <Callout kind="note" title="Try it">
          Exponential is open source under Apache-2.0 and free for teams of three on the cloud, or{` `}
          <a href="/docs/self-host/">self-hosted</a> without limits. Sign in at{` `}
          <a href={LINKS.app.login}>app.exponential.at</a>, or get the{` `}
          <a href="/download/">desktop app</a>.
        </Callout>
      </section>
    </>
  )
}

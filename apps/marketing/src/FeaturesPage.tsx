/* ─── /features/: "Everything in the Exponential issue tracker" (MKT-9) ───
   The old frontpage's feature tour, moved here when the home page was
   rewritten around the MCP story. A text hero, a section index, then one
   idea per block: the NEW blocks are data-driven FeatureBlocks (eyebrow,
   big h2, a short paragraph, a small point grid, docs links); the three
   animated sections (Agents, Actions, Collab) are reused unchanged, each
   followed by a thin docs row. Everything renders in SSR (prerendered by
   scripts/prerender.tsx): no browser APIs during render. Copy rules: every
   claim matches the docs pages, no em dashes, no prices. */
import { motion } from "motion/react"
import type { ReactNode } from "react"
import { ActionsSection } from "./components/ActionsSection"
import { AgentsSection } from "./components/AgentsSection"
import { CollabSection } from "./components/CollabSection"
import { IcArrow } from "./components/icons"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import {
  eyebrowDraw,
  heroChild,
  heroStagger,
  sectionReveal,
} from "./lib/animations"
import { LINKS } from "./lib/links"
import { MCP_TOOL_COUNT } from "./lib/product-facts"

type DocLink = { label: string; href: string }
type Point = { title: string; text: string }

/* ── The section index. Ids of the moved sections (`agents`, `actions`,
   `collaboration`) are the ones those components already render. ── */
const INDEX: { id: string; label: string; line: string }[] = [
  { id: `mcp`, label: `MCP`, line: `Agents work in the tracker` },
  { id: `issues`, label: `Issues`, line: `Realtime on every client` },
  { id: `agents`, label: `Coding agents`, line: `Claude Code and Codex` },
  { id: `actions`, label: `Actions`, line: `Reusable AI tasks` },
  { id: `reviews`, label: `Reviews`, line: `Every open PR, one queue` },
  { id: `workflows`, label: `Workflows`, line: `A backlog run as a graph` },
  { id: `devices`, label: `Devices`, line: `Your machines, your accounts` },
  { id: `teams`, label: `Teams`, line: `Boards, invites, roles` },
  { id: `collaboration`, label: `Feedback`, line: `Widget and helpdesk` },
  { id: `apps`, label: `Apps`, line: `Desktop, iOS, Android` },
]

/* Tool areas of the MCP server (apps/web/src/lib/mcp/tools.ts). */
const MCP_AREAS = [
  `Issues`,
  `Comments`,
  `Relations`,
  `Labels`,
  `Statuses`,
  `Boards`,
  `Attachments`,
  `Pull requests`,
  `Sessions`,
  `Devices`,
  `Actions`,
  `Automations`,
  `Workflows`,
  `Notifications`,
  `Helpdesk`,
  `Teams`,
  `Members`,
  `Invites`,
  `Repositories`,
]

function DocLinks({ links }: { links: DocLink[] }) {
  return (
    <div className={`ft-doclinks`}>
      {links.map((l) => (
        <a key={l.href} className={`ft-doclink`} href={l.href}>
          {l.label} <IcArrow size={13} />
        </a>
      ))}
    </div>
  )
}

function FeatureBlock({
  id,
  num,
  eyebrow,
  title,
  children,
  points,
  aside,
  docs,
  flip = false,
}: {
  id: string
  num: string
  eyebrow: string
  title: string
  children: ReactNode
  points?: Point[]
  aside?: ReactNode
  docs: DocLink[]
  flip?: boolean
}) {
  return (
    <section id={id} className={`ft-block${flip ? ` is-flip` : ``}`}>
      <div className={`shell ft-block-inner`}>
        <motion.div className={`ft-copy`} {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            {num} · {eyebrow}
          </motion.span>
          <h2 className={`section-title`}>{title}</h2>
          <div className={`ft-body`}>{children}</div>
          <DocLinks links={docs} />
        </motion.div>

        <motion.div
          className={`ft-aside`}
          {...sectionReveal}
          transition={{ ...sectionReveal.transition, delay: 0.08 }}
        >
          {aside}
          {points && (
            <ul className={`ft-points`}>
              {points.map((p) => (
                <li key={p.title} className={`ft-point`}>
                  <span className={`ft-point-title`}>{p.title}</span>
                  <span className={`ft-point-text`}>{p.text}</span>
                </li>
              ))}
            </ul>
          )}
        </motion.div>
      </div>
    </section>
  )
}

/* Thin docs strip after a reused section (its component stays untouched). */
function DocRow({ links }: { links: DocLink[] }) {
  return (
    <div className={`ft-docrow`}>
      <div className={`shell ft-docrow-inner`}>
        <span className={`ft-docrow-label`}>Docs</span>
        <DocLinks links={links} />
      </div>
    </div>
  )
}

export function FeaturesPage() {
  return (
    <>
      <SiteHeader />

      <main className={`ft-page`}>
        {/* ── Text hero: no film, sized to content ── */}
        <section className={`ft-hero`}>
          <motion.div
            className={`shell ft-hero-content`}
            variants={heroStagger}
            initial={`hidden`}
            animate={`visible`}
          >
            <motion.span className={`ft-hero-kicker`} variants={heroChild}>
              Features
            </motion.span>
            <motion.h1 className={`ft-hero-title`} variants={heroChild}>
              Everything in the Exponential issue tracker
            </motion.h1>
            <motion.p className={`ft-hero-sub`} variants={heroChild}>
              Issues, coding agents, reviews and support in one realtime
              tracker, with your agents working inside it over MCP.
            </motion.p>
            <motion.div className={`ft-hero-cta`} variants={heroChild}>
              <a className={`btn btn-primary`} href={LINKS.app.login}>
                Get started free <IcArrow size={12} />
              </a>
              <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
                Download
              </a>
            </motion.div>
          </motion.div>
        </section>

        {/* ── Section index ── */}
        <nav className={`ft-index`} aria-label={`Features on this page`}>
          <div className={`shell`}>
            <ol className={`ft-index-grid`}>
              {INDEX.map((item, i) => (
                <li key={item.id}>
                  <a className={`ft-index-link`} href={`#${item.id}`}>
                    <span className={`ft-index-num`}>
                      {String(i + 1).padStart(2, `0`)}
                    </span>
                    <span className={`ft-index-label`}>{item.label}</span>
                    <span className={`ft-index-line`}>{item.line}</span>
                  </a>
                </li>
              ))}
            </ol>
          </div>
        </nav>

        {/* ── 01 MCP: the idea the new frontpage is built on ── */}
        <FeatureBlock
          id={`mcp`}
          num={`01`}
          eyebrow={`MCP`}
          title={`Every run gets Exponential as its MCP server`}
          docs={[{ label: `Read the MCP docs`, href: `/docs/mcp/` }]}
          aside={
            <div className={`ft-mcp`}>
              <div className={`ft-stat`}>
                <span className={`ft-stat-num`}>{MCP_TOOL_COUNT}</span>
                <span className={`ft-stat-label`}>
                  tools across the whole tracker. Some only appear where they
                  apply, like the helpdesk tools.
                </span>
              </div>
              <ul className={`ft-chips`} aria-label={`Tool areas`}>
                {MCP_AREAS.map((a) => (
                  <li key={a} className={`ft-chip`}>
                    {a}
                  </li>
                ))}
              </ul>
            </div>
          }
        >
          <p>
            Start a Claude Code or Codex run from Exponential and the launcher
            wires in the Exponential MCP server with your own API key. The
            agent reads its issue, comments, moves status and opens its pull
            request as tool calls.
          </p>
          <p>
            The same server connects Claude, ChatGPT, Codex and Cursor on
            their own: over OAuth with a scope picker for teams and boards, or
            with a personal API key for scripts and CI.
          </p>
        </FeatureBlock>

        {/* ── 02 Issue tracking ── */}
        <FeatureBlock
          id={`issues`}
          num={`02`}
          eyebrow={`Issues`}
          title={`A fast issue tracker, live on every client`}
          docs={[{ label: `Read the issues docs`, href: `/docs/issues/` }]}
          flip
          points={[
            {
              title: `Realtime sync`,
              text: `Web, desktop, iOS and Android sync the same data live. A change on one shows up on the others without a refresh.`,
            },
            {
              title: `Your statuses`,
              text: `Six locked builtins per team, plus custom statuses in the categories your process needs.`,
            },
            {
              title: `Relations`,
              text: `Sub-issues, blocks and blocked by, duplicates, related. Writing #EXP-42 links two issues on its own.`,
            },
            {
              title: `Markdown`,
              text: `Descriptions and comments are plain markdown with tables, inline images and @mentions.`,
            },
          ]}
        >
          <p>
            Boards, statuses, priorities, labels and relations, without the
            setup ceremony. Mentions and subscriptions feed one inbox, with a
            daily email digest of whatever is still unread at a local hour
            you pick.
          </p>
        </FeatureBlock>

        {/* ── 03 Coding agents (reused section) ── */}
        <AgentsSection />
        <DocRow
          links={[
            { label: `Coding agents`, href: `/docs/coding/` },
            { label: `CLI and daemon`, href: `/docs/cli/` },
          ]}
        />

        {/* ── 04 Actions + automations (reused section) ── */}
        <ActionsSection />
        <DocRow
          links={[{ label: `Actions and automations`, href: `/docs/actions/` }]}
        />

        {/* ── 05 Reviews ── */}
        <FeatureBlock
          id={`reviews`}
          num={`05`}
          eyebrow={`Reviews`}
          title={`Every open PR in one queue`}
          docs={[
            { label: `Read the review docs`, href: `/docs/coding/#review-merge` },
          ]}
          points={[
            {
              title: `Confirmed squash merge`,
              text: `Read the diff and merge right there, after one confirm. The linked issues complete on merge.`,
            },
            {
              title: `Stacked PRs`,
              text: `A run can build on another open PR's branch. The stack nests in Reviews and merges in order.`,
            },
            {
              title: `Fix merge conflicts`,
              text: `When a merge fails on conflicts, one click hands the PR to a builtin action that rebases, resolves and merges.`,
            },
            {
              title: `Agent runs`,
              text: `PRs opened by actions and chats, with no issue attached, get their own group.`,
            },
          ]}
        >
          <p>
            Reviews collects your team&apos;s open pull requests across every
            board, whoever opened them. It is still a normal GitHub pull
            request, so merging it on GitHub completes the issue just the
            same.
          </p>
        </FeatureBlock>

        {/* ── 06 Workflows (no docs page yet: link the coding docs, claim
               only what the orchestrator does) ── */}
        <FeatureBlock
          id={`workflows`}
          num={`06`}
          eyebrow={`Workflows`}
          title={`Run a backlog as a graph`}
          docs={[{ label: `Read the coding docs`, href: `/docs/coding/` }]}
          flip
          points={[
            {
              title: `Blocks are the edges`,
              text: `The relations you already keep decide the order. Issues that do not depend on each other can run in parallel.`,
            },
            {
              title: `Every node is a run`,
              text: `A parent with its sub-issues becomes one node, run together on one branch.`,
            },
            {
              title: `Built on its blockers`,
              text: `Each node starts from the work it depends on, merged in, never rebased.`,
            },
            {
              title: `One final PR`,
              text: `Node PRs merge into the workflow branch. Your default branch gets one pull request at the end.`,
            },
          ]}
        >
          <p>
            Pick backlog issues from one repository and run them as a
            workflow. Your desktop app or CLI daemon orchestrates it on your
            own machine, starting each node once the work it depends on allows it.
          </p>
        </FeatureBlock>

        {/* ── 07 Devices + agent accounts ── */}
        <FeatureBlock
          id={`devices`}
          num={`07`}
          eyebrow={`Devices`}
          title={`Runs on your machines, on your subscription`}
          docs={[
            { label: `Read the CLI docs`, href: `/docs/cli/` },
            { label: `Agent accounts`, href: `/docs/coding/` },
          ]}
          points={[
            {
              title: `Desktop IDE`,
              text: `Runs execute in the desktop app on macOS, Windows or Linux, each in its own worktree.`,
            },
            {
              title: `Headless daemon`,
              text: `The exponential CLI turns a server into an always-on agent machine you start runs on remotely.`,
            },
            {
              title: `Several accounts`,
              text: `One machine can hold more than one Claude or Codex login. Each run picks the account it uses.`,
            },
            {
              title: `Auto-rotate`,
              text: `With auto-rotate on, every Claude run starts on the login with the most headroom left in its usage window.`,
            },
          ]}
        >
          <p>
            Agents run on your hardware with your own agent CLI and your own
            GitHub access. Start and steer them from the web or your phone. A
            live run is visible and steerable only by you; teammates see its
            status.
          </p>
        </FeatureBlock>

        {/* ── 08 Teams ── */}
        <FeatureBlock
          id={`teams`}
          num={`08`}
          eyebrow={`Teams`}
          title={`Built for teams from the first issue`}
          docs={[
            { label: `Read getting started`, href: `/docs/getting-started/` },
          ]}
          flip
          points={[
            {
              title: `Boards`,
              text: `Each board has its own prefix and an optional GitHub repository its runs code in.`,
            },
            {
              title: `Invite links`,
              text: `Create a link, share it, revoke it any time before it is accepted.`,
            },
            {
              title: `Every member moderates`,
              text: `Members triage, manage statuses and labels, and answer support.`,
            },
            {
              title: `Owners own settings`,
              text: `Team settings and deletes belong to owners, and their controls stay hidden from everyone else.`,
            },
          ]}
        >
          <p>
            Create a team, add boards and invite people with a link. Everyone
            works on the same board, live, whichever client they use.
          </p>
        </FeatureBlock>

        {/* ── 09 Feedback widget + helpdesk + platforms (reused section) ── */}
        <CollabSection />
        <DocRow
          links={[
            { label: `Feedback and helpdesk`, href: `/docs/feedback/` },
            { label: `Feedback widget`, href: `/docs/widget/` },
          ]}
        />

        {/* ── 10 Apps ── */}
        <FeatureBlock
          id={`apps`}
          num={`10`}
          eyebrow={`Apps`}
          title={`Native on every screen`}
          docs={[{ label: `Read the apps docs`, href: `/docs/apps/` }]}
          points={[
            {
              title: `iOS and Android`,
              text: `Native apps with push for assignments, comments, mentions and pull requests, each opening the issue.`,
            },
            {
              title: `Start and steer`,
              text: `Launch a run from your phone, watch it live and answer its questions.`,
            },
            {
              title: `The Work screen`,
              text: `An issue, its run, the changes and the results as faces of one screen.`,
            },
            {
              title: `Desktop`,
              text: `macOS, Windows and Linux. The IDE where your runs execute, and it updates itself.`,
            },
          ]}
        >
          <p>
            The same tracker in native apps, not a wrapped website. Everything
            syncs live between them, so a run you start at the desk is one
            tap away on your phone.
          </p>
        </FeatureBlock>

        <FooterCTA />
      </main>
      <SiteFooter />
    </>
  )
}

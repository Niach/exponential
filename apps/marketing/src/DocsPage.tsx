import {
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcChev } from "./components/icons"
import { WebDemo } from "./webui/WebDemo"
import { DOCS_NAV } from "./lib/docs-nav"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `what-is`, num: `01`, label: `What is Exponential` },
  { id: `quickstart`, num: `02`, label: `Quickstart` },
  { id: `browse`, num: `03`, label: `Browse the docs` },
  { id: `community`, num: `04`, label: `Community & help` },
]

/* Card blurbs for the hub grid, keyed by DOCS_NAV path. */
const CARD_DESCRIPTIONS: Record<string, string> = {
  "/docs/getting-started/": `Sign up, create your first project, connect GitHub, invite your team.`,
  "/docs/issues/": `The board, statuses, markdown, mentions, notifications, and how issues link to PRs.`,
  "/docs/coding/": `Hand issues to Claude from the desktop IDE — single runs, batch runs, steer, review, merge.`,
  "/docs/feedback/": `Feedback projects, public boards, and the built-in email helpdesk.`,
  "/docs/widget/": `Embed the feedback button on any site — snippet, JS API, screenshots.`,
  "/docs/mcp/": `Connect Claude, ChatGPT, Cursor, or any MCP client to your issues.`,
  "/docs/apps/": `The desktop IDE and the iOS / Android companions — install, push, steer.`,
  "/docs/self-host/": `Run the whole stack on your own server with Docker Compose.`,
}

export function DocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Docs</h1>
            <p>
              Everything about Exponential — issue tracking, coding with
              Claude, feedback boards, the helpdesk, the apps, and the API.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href="/docs/getting-started/">
                Get started
              </a>
              <a className="btn btn-ghost" href="/docs/self-host/">
                Self-host docs <IcArrow size={12} />
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/">
          {/* ── 01 What is Exponential ── */}
          <DocsSection id="what-is" num="01" label="What is Exponential">
            <h2>What is Exponential</h2>
            <p>
              Exponential is an issue tracker that closes the loop:{` `}
              <strong>feedback in, issues on a board, agent-coded pull
              requests out</strong>. User reports arrive through the{` `}
              <a href="/docs/widget/">embeddable widget</a> or a{` `}
              <a href="/docs/feedback/">public feedback board</a>, your team
              triages them as <a href="/docs/issues/">issues</a>, and the{` `}
              <a href="/docs/coding/">desktop IDE hands issues to Claude</a> —
              running locally on your machine — which implements, pushes, and
              opens the GitHub PR. Merging the PR completes the issue and can
              notify the person who reported it.
            </p>
            <p>
              It syncs in real time across web, iOS, Android, and the native
              desktop app, and it&apos;s the same product either way you run
              it: the free cloud at{` `}
              <a href={LINKS.app.login}>app.exponential.at</a> or{` `}
              <a href="/docs/self-host/">self-hosted</a> on your own server
              with no limits.
            </p>

            <div className="docs-embed">
              <WebDemo view="board" interactive={false} />
            </div>
          </DocsSection>

          {/* ── 02 Quickstart ── */}
          <DocsSection id="quickstart" num="02" label="Quickstart">
            <h2>Quickstart</h2>
            <p>From zero to a merged, agent-written PR in five steps:</p>
            <ol>
              <li>
                <strong>Sign up</strong> at{` `}
                <a href={LINKS.app.register}>app.exponential.at</a> — a
                personal team is created for you automatically. Details in{` `}
                <a href="/docs/getting-started/">Getting started</a>.
              </li>
              <li>
                <strong>Create your first project</strong> with the first-run
                wizard: pick a name and a short prefix (issues become{` `}
                <code>EXP-1</code>, <code>EXP-2</code>, …), and connect a
                GitHub repository if you want coding sessions and PRs.
              </li>
              <li>
                <strong>File your first issue</strong> — title, markdown
                description, status, priority, labels. See{` `}
                <a href="/docs/issues/">Issues &amp; boards</a>.
              </li>
              <li>
                <strong>Start coding</strong>: install the{` `}
                <a href={LINKS.downloadPage}>desktop app</a>, open the issue,
                and hit <strong>Start coding</strong> — Claude runs in the
                embedded terminal, plans first, implements, and opens the PR
                itself. See <a href="/docs/coding/">Coding with Claude</a>.
              </li>
              <li>
                <strong>Merge</strong> — the issue moves to In Review when the
                PR opens and completes to Done when it merges.
              </li>
            </ol>
          </DocsSection>

          {/* ── 03 Browse the docs ── */}
          <DocsSection id="browse" num="03" label="Browse the docs">
            <h2>Browse the docs</h2>
            <div className="docs-cards">
              {DOCS_NAV.filter((p) => p.path !== `/docs/`).map((page) => (
                <a key={page.path} className="docs-card" href={page.path}>
                  <span className="docs-card-title">
                    {page.label} <IcChev size={13} />
                  </span>
                  <span className="docs-card-desc">
                    {CARD_DESCRIPTIONS[page.path]}
                  </span>
                </a>
              ))}
            </div>
          </DocsSection>

          {/* ── 04 Community & help ── */}
          <DocsSection id="community" num="04" label="Community & help">
            <h2>Community &amp; help</h2>
            <p>
              Exponential is developed in the open at{` `}
              <a href={LINKS.github.repo}>github.com/Niach/exponential</a> —
              issues, code, and release notes all live there.
            </p>
            <p>
              Found a bug or want a feature? Post it on the{` `}
              <a href="https://app.exponential.at">
                public feedback board
              </a>{` `}
              — it&apos;s a real Exponential feedback project, and reports are
              triaged (and often fixed by Claude) in the open. You can also use
              the feedback button in the corner of this very site: it&apos;s
              the <a href="/docs/widget/">embeddable widget</a>, running for
              real.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

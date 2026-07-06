import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { IdeDemo } from "./ide/Ide"
import { MobileDemo } from "./mobile/MobileDemo"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `getting-started`, num: `01`, label: `Getting started` },
  { id: `issues`, num: `02`, label: `Issues` },
  { id: `desktop-apps`, num: `03`, label: `Desktop IDE` },
  { id: `start-coding`, num: `04`, label: `Start coding` },
  { id: `mobile`, num: `05`, label: `Mobile apps` },
  { id: `feedback-widget`, num: `06`, label: `Feedback widget` },
  { id: `mcp`, num: `07`, label: `MCP & API` },
]

const WIDGET_SNIPPET = `<script>
  (function (w, d, u) {
    if (w.ExponentialWidget) return;
    var q = [], api = { q: q };
    ["init","identify","setCustomData","open","close"].forEach(function (m) {
      api[m] = function () { q.push([m, [].slice.call(arguments)]); };
    });
    w.ExponentialWidget = api;
    var s = d.createElement("script");
    s.async = true; s.src = u;
    d.head.appendChild(s);
  })(window, document, "https://app.exponential.at/widget/v1/loader.js");
  ExponentialWidget.init({ key: "expw_YOUR_KEY" });
</` + `script>`

export function DocsPage() {
  return (
    <>
      <SiteHeader />

      <section className="docs-hero">
        <div className="shell docs-hero-content">
          <h1>Docs</h1>
          <p>Issues, the desktop IDE, coding sessions, mobile apps, and the API.</p>
          <div className="docs-hero-cta">
            <a className="btn btn-primary" href="#getting-started">
              Get started
            </a>
            <a className="btn btn-ghost" href="/docs/self-host/">
              Self-host docs <IcArrow size={12} />
            </a>
          </div>
        </div>
      </section>

      <DocsLayout sections={SECTIONS}>
        {/* ── 01 Getting started ── */}
        <DocsSection id="getting-started" num="01" label="Getting started">
          <h2>Getting started</h2>
          <p>
            Sign up free at{` `}
            <a href={LINKS.app.register}>app.exponential.at</a> — or{` `}
            <a href="/docs/self-host/">self-host</a>. Your first workspace is
            created automatically when you sign in.
          </p>

          <h3>Create a project</h3>
          <p>
            Every project is backed by exactly one GitHub repository. The
            first-run wizard walks you through it: install the GitHub App,
            pick the repo, name the project, and choose a short prefix (e.g.{` `}
            <code>EXP</code>) — issues are numbered with it (<code>EXP-1</code>,{` `}
            <code>EXP-2</code>, …).
          </p>

          <h3>Invite members</h3>
          <p>
            Open <strong>Workspace Settings → Members</strong> and generate an
            invite link. Anyone who follows it joins the workspace.
          </p>
        </DocsSection>

        {/* ── 02 Issues ── */}
        <DocsSection id="issues" num="02" label="Issues">
          <h2>Issues</h2>
          <p>
            Each issue has a status (<strong>Backlog</strong>,{` `}
            <strong>Todo</strong>, <strong>In Progress</strong>,{` `}
            <strong>Done</strong>, <strong>Cancelled</strong>,{` `}
            <strong>Duplicate</strong>), a priority (Urgent → Low), optional
            labels, an assignee, and an optional due date. Change any of them
            inline from the list or the detail view. The filter bar has{` `}
            <strong>All Issues</strong> / <strong>Active</strong> /{` `}
            <strong>Backlog</strong> presets plus a popover for filtering by
            status, priority, or label.
          </p>

          <div className="docs-embed">
            <IdeDemo view="board" interactive={false} />
          </div>

          <h3>Markdown descriptions</h3>
          <p>
            Descriptions and comments are GitHub-flavored markdown and render
            identically on web, iOS, Android, and desktop: headings, lists,
            task lists (<code>- [ ]</code>), code blocks, blockquotes, links,
            and inline images. Type <code>@</code> to mention a teammate —
            they&apos;re notified and auto-subscribed to the issue.
          </p>

          <h3>Recurring issues</h3>
          <p>
            Give an issue a recurrence interval and the next occurrence is
            created the moment you complete the current one.
          </p>

          <h3>One issue, one pull request</h3>
          <p>
            An issue that gets coded maps to exactly one branch —{` `}
            <code>exp/&lt;IDENTIFIER&gt;</code> — and one pull request. The PR
            is linked on the issue and its state (open, merged) is tracked
            automatically.
          </p>
        </DocsSection>

        {/* ── 03 Desktop IDE ── */}
        <DocsSection id="desktop-apps" num="03" label="Desktop IDE">
          <h2>Desktop IDE</h2>
          <p>
            The native desktop app for macOS and Linux is a full git IDE —
            and the one client that runs coding sessions. Get it from the{` `}
            <a href={LINKS.downloadPage}>download page</a>.
          </p>
          <p>
            Open a project and its repository clones automatically. Inside:
            the issue board and issue tabs, a file tree, a source-control
            panel (stage, commit, push/pull, history, side-by-side diffs),
            and an embedded terminal.
          </p>

          <div className="docs-embed">
            <IdeDemo view="source-control" interactive={false} />
          </div>

          <p>
            Sign in with your usual account — <code>app.exponential.at</code>,
            or your own domain if you self-host.
          </p>
        </DocsSection>

        {/* ── 04 Start coding ── */}
        <DocsSection id="start-coding" num="04" label="Start coding">
          <h2>Start coding</h2>
          <p>
            <strong>Start coding</strong> on any issue hands it to Claude —
            on your machine, on your Claude subscription. You need{` `}
            <code>git</code> and the <code>claude</code> CLI on your{` `}
            <code>PATH</code>, authenticated. Nothing else.
          </p>

          <h3>What happens</h3>
          <ol>
            <li>
              The app creates a git worktree on a fresh{` `}
              <code>exp/&lt;IDENTIFIER&gt;</code> branch and mints a
              short-lived GitHub token for the session.
            </li>
            <li>
              Claude opens in the embedded terminal, seeded with the issue.
              It proposes a plan first, then implements.
            </li>
            <li>
              When done, it commits, pushes, and opens the pull request
              itself — linked back to the issue.
            </li>
          </ol>

          <div className="docs-embed">
            <IdeDemo view="issue" interactive={false} />
          </div>

          <h3>Stop &amp; steer</h3>
          <p>
            It&apos;s a real terminal — type into it any time to redirect or
            answer a question, and hit <strong>Stop</strong> to end the
            session. While it runs, the live terminal also streams to your
            other devices, so you can watch and steer from your phone.
          </p>
        </DocsSection>

        {/* ── 05 Mobile apps ── */}
        <DocsSection id="mobile" num="05" label="Mobile apps">
          <h2>Mobile apps</h2>
          <p>
            Native iOS and Android companion apps: triage the board, review
            issues, and comment on the go — everything syncs in real time.
            Push notifications cover assignments, comments, mentions, and PR
            updates. Workspace and project setup happens on web or desktop.
          </p>

          <div className="docs-embed docs-embed-phone">
            <MobileDemo />
          </div>

          <DocsCallout kind="note" title="Push on self-hosted">
            Push works out of the box on the cloud. Self-hosted instances
            point at the push relay — see the{` `}
            <a href="/docs/self-host/#push">self-host docs</a>.
          </DocsCallout>
        </DocsSection>

        {/* ── 06 Feedback widget ── */}
        <DocsSection id="feedback-widget" num="06" label="Feedback widget">
          <h2>Feedback widget</h2>
          <p>
            Embed a feedback button on any website. Visitors report a bug with
            a screenshot — captured in the browser, annotatable with
            rectangles, arrows, and freehand lines — and the submission lands
            as an issue in the project you choose, screenshot attached.
          </p>
          <p>
            Create a widget in <strong>Workspace Settings → Feedback
            widget</strong> (workspace owners only). Each config gets a public{` `}
            <code>expw_</code> key and a domain allowlist. Then paste the
            snippet:
          </p>
          <DocsCode language="html">{WIDGET_SNIPPET}</DocsCode>
          <p>
            Optionally call <code>ExponentialWidget.identify(&#123;email,
            name&#125;)</code> to attach your signed-in user, and{` `}
            <code>setCustomData(&#123;…&#125;)</code> to add context to every
            submission.
          </p>
        </DocsSection>

        {/* ── 07 MCP & API ── */}
        <DocsSection id="mcp" num="07" label="MCP & API">
          <h2>MCP &amp; API</h2>
          <p>
            Every instance exposes an MCP server at <code>/api/mcp</code>.
            Point Claude Code, Cursor, or any MCP-aware tool at it and it can
            list, create, and edit issues, projects, labels, and comments:
          </p>
          <DocsCode language="json">{`
{
  "mcpServers": {
    "exponential": {
      "url": "https://app.exponential.at/api/mcp"
    }
  }
}
`}</DocsCode>
          <p>
            <strong>Authentication:</strong> OAuth for interactive tools (the
            standard MCP flow), or an API key as a bearer token for headless
            agents and scripts.
          </p>
        </DocsSection>
      </DocsLayout>

      <SiteFooter />
    </>
  )
}

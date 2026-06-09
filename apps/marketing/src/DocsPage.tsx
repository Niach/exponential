import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import {
  DocsMockupAgentSettings,
  DocsMockupDesktopAgent,
  DocsMockupIssueList,
  DocsMockupPhoneConnect,
  DocsMockupPlanComment,
  DocsMockupSidebar,
} from "./components/DocsMockups"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `getting-started`, num: `01`, label: `Getting started` },
  { id: `issues`, num: `02`, label: `Issues` },
  { id: `desktop-apps`, num: `03`, label: `Desktop apps` },
  { id: `mobile`, num: `04`, label: `Mobile apps` },
  { id: `agents`, num: `05`, label: `AI agents` },
  { id: `feedback`, num: `06`, label: `Public feedback` },
  { id: `integrations`, num: `07`, label: `Integrations` },
]

export function DocsPage() {
  return (
    <>
      <SiteHeader />

      <section className="docs-hero">
        <div className="hero-atmos" aria-hidden />
        <div className="shell docs-hero-content fade-in">
          <h1>
            How to use Exponential.
          </h1>
          <p>
            Everything you need to know — issues, desktop and mobile apps,
            local AI agents, and integrations.
          </p>
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
            <a href="https://app.exponential.at">app.exponential.at</a> — or{` `}
            <a href="/docs/self-host/">self-host</a> on your own server. Either
            way the experience is identical. Then grab the{` `}
            <a href="#desktop-apps">desktop</a> and{` `}
            <a href="#mobile">mobile</a> apps — everything stays in sync.
          </p>

          <h3>Create a workspace</h3>
          <p>
            The first user to sign in creates a workspace automatically. Give
            it a name and you're up. The first user becomes the workspace admin
            and can manage settings, invite links, and member roles later.
          </p>

          <h3>Invite your team</h3>
          <p>
            Open <strong>Workspace Settings</strong> from the sidebar, switch to
            the <strong>Invites</strong> tab, and generate an invite link. Share
            it with your team — anyone who follows the link joins the workspace.
          </p>

          <h3>Create a project</h3>
          <p>
            Click the <strong>+</strong> button next to "Projects" in the
            sidebar. Give the project a name and a short prefix (e.g.{` `}
            <code>EXP</code>) — every issue in that project will be numbered
            with it (<code>EXP-1</code>, <code>EXP-2</code>, …).
          </p>

          <DocsMockupSidebar />

          <DocsCallout kind="tip" title="Multiple workspaces">
            You can create as many workspaces as you like. Each workspace has
            its own projects, members, and settings. Useful for separating
            teams, clients, or open-source work.
          </DocsCallout>
        </DocsSection>

        {/* ── 02 Issues ── */}
        <DocsSection id="issues" num="02" label="Issues">
          <h2>Issues</h2>
          <p>
            Issues are the core unit of work. Each issue lives in a project,
            has a status, priority, optional labels, and an optional due date.
          </p>

          <h3>Creating issues</h3>
          <p>
            Click <strong>+ New Issue</strong> in the top right of the project
            view. Fill in a title, pick a status and priority, add labels if
            you like, and set a due date. Hit "Create more" to stay in the
            dialog and file a batch of issues without closing it.
          </p>

          <h3>Status workflow</h3>
          <p>
            Every issue follows a five-state workflow: <strong>Backlog</strong>
            {` `}(dimmed circle), <strong>Todo</strong> (open circle),{` `}
            <strong>In Progress</strong> (half-filled circle),{` `}
            <strong>Done</strong> (filled check), and{` `}
            <strong>Cancelled</strong> (crossed out). Click the status icon on
            any row to change it inline.
          </p>

          <h3>Markdown everywhere</h3>
          <p>
            Issue descriptions and comments are GitHub-flavored markdown, and
            the same dialect renders identically on web, iOS, Android, macOS,
            and Linux: bold, italic, strikethrough, headings, lists,{` `}
            <strong>task lists</strong> (<code>- [ ]</code>), code blocks,
            blockquotes, links, and inline images. Type{` `}
            <code>@</code> to mention a teammate — they're notified and
            auto-subscribed to the issue.
          </p>

          <h3>Labels and filtering</h3>
          <p>
            Create labels in the label picker (inside the issue dialog) or from
            Workspace Settings. The filter bar at the top of the issue list has
            tab presets — <strong>All Issues</strong>, <strong>Active</strong>
            {` `}(In Progress + Todo), and <strong>Backlog</strong>. Open the
            filter popover for multi-category filtering by status, priority,
            or label.
          </p>

          <DocsMockupIssueList />

          <h3>Recurring issues</h3>
          <p>
            Give an issue a recurrence interval — daily, weekly, or monthly —
            and the next occurrence is created automatically the moment you
            complete the current one. Great for standups, releases, and
            maintenance chores.
          </p>

          <h3>Notifications &amp; inbox</h3>
          <p>
            Assignments, comments, mentions, status changes, and agent events
            land in your <strong>Inbox</strong> (in the sidebar) — and as push
            notifications on your phone. You're auto-subscribed to issues you
            create, get assigned, comment on, or are mentioned in; a subscribe
            toggle on every issue lets you opt in or out.
          </p>

          <h3>Due dates</h3>
          <p>
            Set a due date on any issue from the create or edit dialog. Due
            dates show on the right side of each row. If you connect{` `}
            <a href="#integrations">Google Calendar</a>, issues with due dates
            automatically appear as all-day events on your calendar.
          </p>
        </DocsSection>

        {/* ── 03 Desktop apps ── */}
        <DocsSection id="desktop-apps" num="03" label="Desktop apps">
          <h2>Desktop apps</h2>
          <p>
            Native desktop apps for <strong>macOS</strong> (Swift) and{` `}
            <strong>Linux</strong> (GTK4) — full trackers with one superpower:
            they turn your machine into a workspace agent that runs Claude or
            Codex locally, in a real terminal, against your repos.
          </p>

          <h3>Download &amp; sign in</h3>
          <p>
            Grab the app for{` `}
            <a href={LINKS.downloads.macos}>macOS</a> or{` `}
            <a href={LINKS.downloads.linux}>Linux</a>. On first launch, enter
            your server URL — <code>app.exponential.at</code> for the cloud,
            or your own domain if you self-host — and sign in with your usual
            account.
          </p>

          <h3>Automatic agent registration</h3>
          <p>
            When you sign in, the app registers your device as an{` `}
            <strong>agent member</strong>: one agent user per physical machine,
            joined to every workspace you belong to with the{` `}
            <code>Agent</code> role. No tokens to copy, no daemon to install —
            it shows up in <strong>Workspace Settings → Agents</strong>{` `}
            immediately, and teammates can assign issues to it like to any
            member.
          </p>

          <DocsMockupAgentSettings />

          <h3>What the agent needs</h3>
          <ul>
            <li>
              The repo cloned locally — the agent works in git worktrees next
              to your checkout and never touches your working tree.
            </li>
            <li>
              The <code>claude</code> or <code>codex</code> CLI installed and
              authenticated — the agent runs <em>your</em> subscription on{` `}
              <em>your</em> hardware. Nothing is delegated to a third-party
              cloud.
            </li>
            <li>
              GitHub connected (see{` `}
              <a href="#integrations">Integrations</a>) so the agent can open
              pull requests.
            </li>
          </ul>

          <h3>The embedded terminal</h3>
          <p>
            Agent sessions run inside an embedded{` `}
            <strong>ghostty</strong> terminal in the app — a real terminal
            emulator, not a log view. Watch the agent think, scroll back
            through its output, and take over the session with your keyboard
            at any time.
          </p>

          <DocsMockupDesktopAgent />

          <DocsCallout kind="note" title="While the app is open">
            Agents run while the desktop app is open. Close the app and the
            device shows as offline; assigned issues simply wait until it's
            back.
          </DocsCallout>
        </DocsSection>

        {/* ── 04 Mobile apps ── */}
        <DocsSection id="mobile" num="04" label="Mobile apps">
          <h2>Mobile apps</h2>
          <p>
            Native iOS and Android clients with full offline support. Create,
            edit, and comment on issues from your phone — everything syncs in
            real time.
          </p>

          <h3>Download</h3>
          <p>
            <strong>iOS</strong> — get it from the{` `}
            <a href={LINKS.downloads.ios}>App Store</a>.{` `}
            <strong>Android</strong> — get it on{` `}
            <a href={LINKS.downloads.android}>Google Play</a>, or build from
            source with <code>bun run android:build</code>.
          </p>

          <h3>Connect to your server</h3>
          <p>
            Open the app, enter your server URL —{` `}
            <code>app.exponential.at</code> for the cloud, or your own domain
            for self-hosted — and sign in. That's it.
          </p>

          <DocsMockupPhoneConnect />

          <h3>Approve agent plans on the go</h3>
          <p>
            When a desktop agent finishes a plan, a push notification lands on
            your phone. Open it, read the plan, and approve, request changes,
            or cancel — the agent on your desk picks up your decision in
            real time.
          </p>

          <h3>Multi-server</h3>
          <p>
            Need to talk to more than one instance? Open{` `}
            <strong>Settings</strong> in the app, tap{` `}
            <strong>Add server</strong>, and connect to a second (or third)
            instance. Switch between them with a single tap. Issues from all
            servers appear in a unified home view.
          </p>

          <h3>Offline-first</h3>
          <p>
            Both apps work without a network connection. Edits are queued
            locally and sync when connectivity returns — on the subway, on a
            plane, wherever.
          </p>

          <DocsCallout kind="note" title="Push notifications">
            Push works out of the box with the cloud. Self-hosted instances
            need the push relay — see the{` `}
            <a href="/docs/self-host/#push">self-host docs</a> for setup.
          </DocsCallout>
        </DocsSection>

        {/* ── 05 AI agents ── */}
        <DocsSection id="agents" num="05" label="AI agents">
          <h2>AI agents</h2>
          <p>
            Exponential has first-class support for AI coding agents — and
            they run on <strong>your</strong> hardware, not in someone else's
            cloud. Assign an issue, review the plan, get a pull request.
          </p>

          <h3>Adding an agent to your workspace</h3>
          <p>
            Install the <a href="#desktop-apps">desktop app</a> on the machine
            that should do the work and sign in. The device registers itself
            as an agent member and joins every workspace you belong to — no
            install command, no API keys to paste. You'll see it under{` `}
            <strong>Workspace Settings → Agents</strong> with a live
            online/offline status.
          </p>

          <h3>How agents work</h3>
          <ol>
            <li>
              <strong>Assign an issue</strong> to the device's agent user —
              just like you'd assign it to a teammate.
            </li>
            <li>
              <strong>The agent reads the issue</strong>, drafts a plan with
              the claude or codex CLI, and posts it as a comment.
            </li>
            <li>
              <strong>You get a push notification</strong> — review the plan on
              your phone, the web, or any client.
            </li>
            <li>
              <strong>Approve, request changes, or cancel.</strong> If you
              request changes the agent revises the plan and posts a new
              revision.
            </li>
            <li>
              <strong>On approval</strong>, the agent creates a git worktree
              and codes in the embedded ghostty terminal — watch it live or
              take over any time — then opens a GitHub pull request linked
              back to the issue.
            </li>
          </ol>

          <DocsMockupPlanComment />

          <h3>Questions welcome</h3>
          <p>
            If the agent gets stuck it posts a <strong>question</strong>{` `}
            comment and pauses. Answer from any device and it resumes with
            your guidance.
          </p>

          <h3>MCP for Claude Code / Cursor</h3>
          <p>
            Point any MCP-aware tool at your instance's endpoint and it can
            list, create, edit, and comment on issues directly from your IDE.
            Use OAuth to authenticate interactively, or set up a bearer token
            for headless agents.
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
            The MCP server exposes tools covering issues, projects, labels,
            and comments. See <a href="#integrations">Integrations</a> for the
            full list.
          </p>
        </DocsSection>

        {/* ── 06 Public feedback ── */}
        <DocsSection id="feedback" num="06" label="Public feedback">
          <h2>Public feedback</h2>
          <p>
            Every Exponential instance — cloud and self-hosted — has a built-in
            feedback path. Click <strong>Send feedback</strong> in the sidebar
            and a pre-filled issue opens in the shared{` `}
            <a href={LINKS.app.feedback}>
              cloud feedback workspace
            </a>
            . Bugs, feature requests, and questions all land in one place.
          </p>
          <p>
            Self-hosted instances deep-link to the cloud feedback workspace
            automatically, so there's a single canonical backlog no matter how
            many instances are running.
          </p>

          <h3>Your own public workspaces</h3>
          <p>
            You can make any workspace public. In{` `}
            <strong>Workspace Settings</strong>, toggle{` `}
            <strong>Public</strong> and choose a write policy —{` `}
            <strong>anyone can create</strong> (open feedback board) or{` `}
            <strong>members only</strong> (read-only for visitors). Public
            workspaces are accessible without sign-in at{` `}
            <code>{`/w/<workspace-slug>`}</code>.
          </p>
        </DocsSection>

        {/* ── 07 Integrations ── */}
        <DocsSection id="integrations" num="07" label="Integrations">
          <h2>Integrations</h2>
          <p>
            Integrations are opt-in and per-user. Connect them from the user
            dropdown in the sidebar.
          </p>

          <h3>GitHub</h3>
          <p>
            Exponential integrates with GitHub through a{` `}
            <strong>GitHub App</strong> — install it on your account or org
            from <strong>Account → Integrations</strong> and pick the repos
            it may touch. The server mints short-lived, per-repo tokens on
            demand; no personal access tokens are ever stored.
          </p>
          <p>
            Each agent-driven issue maps to <strong>one branch and one pull
            request</strong>. When the PR merges, Exponential notices —
            instantly via webhook on the cloud, or via polling for
            self-hosted instances behind NAT — and records it on the issue
            timeline.
          </p>

          <h3>Google Calendar</h3>
          <p>
            One-way sync from issues to your primary Google Calendar. Issues
            with a <strong>due date</strong> and a non-closed status appear as
            all-day events. Events update when the issue changes and are
            removed when the issue is done, cancelled, or archived. Failures
            are logged on the issue but never block your work.
          </p>
          <p>
            To connect, open the user dropdown in the sidebar and go to{` `}
            <strong>Integrations</strong>. Click{` `}
            <strong>Connect Google Calendar</strong> and authorize access.
            That's it — due-date issues start syncing immediately.
          </p>
          <DocsCallout kind="note" title="Self-hosted?">
            Google Calendar requires a Google OAuth client configured on the
            server. See the{` `}
            <a href="/docs/self-host/#environment">
              self-host environment variables
            </a>{` `}
            for setup.
          </DocsCallout>

          <h3>MCP endpoint</h3>
          <p>
            Exponential exposes a built-in MCP server at{` `}
            <code>/api/mcp</code> so AI agents and IDE tools can interact with
            your issues programmatically. Use it with Claude Code, Cursor, or
            any MCP-compatible client.
          </p>
          <p>
            <strong>Authentication:</strong> OAuth2 for interactive tools (the
            standard MCP auth flow), or an API key / bearer token for headless
            agents and scripts.
          </p>
          <p>
            <strong>Key tools available:</strong>
          </p>
          <ul>
            <li>List, create, update, and delete issues</li>
            <li>List and create projects</li>
            <li>List, create, and manage labels</li>
            <li>Add and list comments on issues</li>
            <li>List workspace members and manage assignments</li>
          </ul>
          <DocsCode language="json">{`
{
  "mcpServers": {
    "exponential": {
      "url": "https://app.exponential.at/api/mcp"
    }
  }
}
`}</DocsCode>
        </DocsSection>
      </DocsLayout>

      <SiteFooter />
    </>
  )
}

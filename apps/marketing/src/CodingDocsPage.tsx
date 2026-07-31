import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { IdeDemo } from "./ide/Ide"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `how-it-works`, num: `01`, label: `How it works` },
  { id: `setup`, num: `02`, label: `Setup` },
  { id: `start-coding`, num: `03`, label: `Start coding` },
  { id: `single-runs`, num: `04`, label: `Single runs` },
  { id: `batch-runs`, num: `05`, label: `Batch runs` },
  { id: `watch-steer`, num: `06`, label: `Watch & steer` },
  { id: `review-merge`, num: `07`, label: `Review & merge` },
  { id: `git-ide`, num: `08`, label: `The git IDE` },
]

export function CodingDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Coding agents</h1>
            <p>
              Hand issues to a coding agent from the desktop IDE — Claude
              Code, Codex, or pi. It plans, implements, and opens the pull
              request. On your machine, on your own agent subscription.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href={LINKS.downloadPage}>
                Get the desktop app <IcArrow size={12} />
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/coding/">
          {/* ── 01 How it works ── */}
          <DocsSection id="how-it-works" num="01" label="How it works">
            <h2>How it works</h2>
            <p>
              The <a href="/docs/apps/">desktop app</a> is the client that
              runs coding sessions. When you start one, it hands the issue to
              {` `}
              <strong>your agent running locally</strong> — Claude Code, Codex
              or pi, on your machine, your checkout, your own agent
              subscription. Nothing executes in a cloud sandbox, and your code
              never routes through Exponential&apos;s servers.
            </p>
            <p>The server&apos;s role is deliberately small:</p>
            <ul>
              <li>
                It mints <strong>short-lived, repo-scoped GitHub tokens</strong>
                {` `}
                through the team&apos;s GitHub App connection, so the run can
                push without any long-lived credential on disk.
              </li>
              <li>
                It <strong>opens and links pull requests</strong> when the
                agent calls the built-in MCP tool — and tracks the PR through
                to merge, completing the issue.
              </li>
            </ul>
            <p>
              Because the agent is local, coding is unmetered:{` `}
              <strong>no plan gates it</strong>, on any tier.
            </p>
          </DocsSection>

          {/* ── 02 Setup ── */}
          <DocsSection id="setup" num="02" label="Setup">
            <h2>Setup</h2>
            <ol>
              <li>
                <strong>Install the desktop app</strong> from the{` `}
                <a href={LINKS.downloadPage}>download page</a> — macOS,
                Windows, or Linux.
              </li>
              <li>
                <strong>Have <code>git</code> plus whichever agent CLIs you
                use on your <code>PATH</code></strong> — <code>claude</code>,
                {` `}
                <code>codex</code>, <code>pi</code> — each signed in to its own
                account. The app checks all three but only requires the one
                you pick for the run. That&apos;s the entire dependency list —
                no <code>gh</code>, no tokens to paste.
              </li>
              <li>
                <strong>Sign in</strong> — to{` `}
                <code>app.exponential.at</code> or your self-hosted URL.
              </li>
              <li>
                <strong>Open a repo-backed board.</strong> The IDE clones
                the repository automatically. (Connect a repo in{` `}
                <strong>Team settings → Repositories</strong> if you
                haven&apos;t — see{` `}
                <a href="/docs/getting-started/#connect-github">
                  Getting started
                </a>
                .)
              </li>
            </ol>
            <p>
              Under the hood, the launcher wires a scoped MCP config into the
              run carrying a personal API key — that&apos;s how the agent
              drives Exponential itself: updating issue status, posting
              comments, and opening the PR, all as tools.
            </p>
          </DocsSection>

          {/* ── 03 Start coding ── */}
          <DocsSection id="start-coding" num="03" label="Start coding">
            <h2>Start coding</h2>
            <p>
              Hit <strong>Start coding</strong> on any issue — or check
              several on the board and start them together. One dialog covers
              both:
            </p>

            <div className="docs-embed">
              <IdeDemo view="issue" />
            </div>
            <p className="docs-embed-caption">
              Live demo — click Start coding on the issue to open the dialog.
            </p>

            <ul>
              <li>
                An <strong>agent tab strip</strong> —{` `}
                <strong>Claude Code</strong>, <strong>Codex</strong> or{` `}
                <strong>pi</strong>. The strip narrows to the CLIs the app
                finds installed.
              </li>
              <li>
                A <strong>searchable multi-issue picker</strong> — one checked
                issue launches a single run, two or more launch a batch.
              </li>
              <li>
                <strong>Model</strong> and <strong>Effort</strong> pickers, per
                agent — each agent offers its own models and its own effort
                vocabulary (Codex calls it Reasoning, pi calls it Thinking).
              </li>
              <li>
                <strong>Skip permissions</strong> (Claude and Codex) — off
                leaves the agent in its own guarded auto mode, on hands it a
                full bypass.
              </li>
              <li>
                <strong>Dynamic workflows (ultracode)</strong> — Claude only.
                Lets the run organize its own workflow; it takes over the
                effort setting.
              </li>
              <li>
                <strong>Plan mode</strong> — Claude only. It proposes a plan
                you approve before it touches code.
              </li>
            </ul>
            <p>
              Defaults are <strong>per agent, not per mode</strong>: single and
              batch runs prefill identically. Out of the box that&apos;s{` `}
              <strong>plan mode on</strong>, <strong>ultracode off</strong> and
              {` `}
              <strong>skip permissions off</strong> — change them under{` `}
              <strong>Settings → Agents</strong> on the desktop, per agent, and
              every future run starts from your values. Every run uses exactly
              one repository.
            </p>
            <p>
              The same dialog has an <strong>Actions</strong> tab — the
              team&apos;s saved prompts, run with the same agent, model and
              effort pickers. See <a href="/docs/actions/">Actions</a>.
            </p>
          </DocsSection>

          {/* ── 04 Single runs ── */}
          <DocsSection id="single-runs" num="04" label="Single runs">
            <h2>Single runs</h2>
            <p>One issue, one branch, one PR:</p>
            <ol>
              <li>
                The app creates a <strong>git worktree</strong> on a fresh
                {` `}
                <code>exp/&lt;IDENTIFIER&gt;</code> branch — your main
                checkout stays untouched, and several runs can work the same
                repo side by side.
              </li>
              <li>
                The agent opens in the embedded terminal, seeded with the
                issue. With Claude&apos;s plan mode on it{` `}
                <strong>plans first</strong>; you approve before implementation
                starts.
              </li>
              <li>
                It implements, commits, pushes, and{` `}
                <strong>opens the pull request itself</strong> via the built-in
                MCP tool — the server opens the PR through the GitHub App and
                links it to the issue.
              </li>
              <li>
                The issue flips to <strong>In Review</strong> and merging the
                PR completes it to <strong>Done</strong> — the default;
                retarget either event in{` `}
                <a href="/docs/issues/#branches-prs">Team settings → Statuses</a>
                .
              </li>
            </ol>
          </DocsSection>

          {/* ── 05 Batch runs ── */}
          <DocsSection id="batch-runs" num="05" label="Batch runs">
            <h2>Batch runs</h2>
            <p>
              Check <strong>two or more issues</strong> in the dialog (or use
              the board&apos;s bulk-select bar) and you get a batch run:{` `}
              <strong>one agent session</strong> given all the issues at
              once, working on <strong>one shared branch</strong> (
              <code>exp/batch-&lt;id&gt;</code>), ending in{` `}
              <strong>one combined PR</strong> linked to every issue in the
              batch. Merging that PR completes them all.
            </p>
            <p>
              The batch is deliberately loose — the issues go over as a list
              and the agent organizes the work. Overlapping issues are fine,
              and often the point.
            </p>
            <h3>When to batch</h3>
            <ul>
              <li>
                <strong>Related fixes</strong> — five small bugs in one screen
                make one coherent session and one reviewable PR.
              </li>
              <li>
                <strong>Sweeping changes</strong> — a rename, an API
                migration, a copy sweep across the codebase, filed as several
                issues.
              </li>
              <li>
                <strong>Feedback triage</strong> — bulk-select a morning&apos;s
                worth of widget reports and clear them in one run.
              </li>
            </ul>
            <DocsCallout kind="note" title="Batch size">
              A run takes up to 30 issues, and the dialog shows a cost hint on
              large batches — every checked issue adds to the prompt, so big
              batches are token-hungry.
            </DocsCallout>
          </DocsSection>

          {/* ── 06 Watch & steer ── */}
          <DocsSection id="watch-steer" num="06" label="Watch & steer">
            <h2>Watch &amp; steer</h2>
            <p>
              The embedded terminal is a <strong>real PTY</strong>, not a log
              view — type into it any time to answer a question or redirect
              the run, and hit <strong>Stop</strong> to end the session.
            </p>
            <p>
              While a session runs, your other devices see it live: the{` `}
              <strong>Agents</strong> view on web and mobile shows the running
              session with a live activity feed, and you can{` `}
              <strong>send steer messages</strong> from your phone — the agent
              picks them up mid-run.
            </p>
          </DocsSection>

          {/* ── 07 Review & merge ── */}
          <DocsSection id="review-merge" num="07" label="Review & merge">
            <h2>Review &amp; merge</h2>
            <p>You never have to leave the IDE to land the work:</p>
            <ul>
              <li>
                The issue&apos;s <strong>Changes</strong> tab shows the
                branch&apos;s full diff against the default branch,
                side-by-side.
              </li>
              <li>
                The <strong>Reviews</strong> list in the sidebar collects the
                team&apos;s open PRs, across every board — open one, read the
                diff, and <strong>merge from right there</strong>. The linked
                issues complete on merge. A merge that hits conflicts offers
                the <a href="/docs/actions/">Fix merge conflicts</a> action.
              </li>
            </ul>
            <p>
              Prefer GitHub&apos;s review UI? The PR is a completely normal
              pull request — review and merge it there and the issue completes
              just the same.
            </p>
          </DocsSection>

          {/* ── 08 The git IDE ── */}
          <DocsSection id="git-ide" num="08" label="The git IDE">
            <h2>The git IDE</h2>
            <p>
              Around the coding flow sits a git IDE. Open a board and its
              repository <strong>clones automatically</strong> — that clone is
              the <strong>trunk</strong>, kept level with the default branch by
              a background sync. Coding runs work in their own worktrees, off
              to the side.
            </p>

            <div className="docs-embed">
              <IdeDemo view="source-control" interactive={false} />
            </div>

            <p>
              The editor is <strong>read-only</strong> by design: changes
              arrive as pull requests, not local commits. The{` `}
              <strong>files rail</strong> browses the trunk,{` `}
              <strong>Source control</strong> walks its commit history and
              renders any commit&apos;s diff side-by-side, and an issue&apos;s
              {` `}
              <strong>Changes</strong> tab diffs its branch against the default
              branch. The one write affordance is an escape hatch — discard
              local changes and hard-reset to the remote, behind a confirm.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

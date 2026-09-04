import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { DocShot } from "./components/DocShot"
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
              Hand issues to a coding agent from the desktop IDE: Claude Code,
              Codex, or pi. It plans, implements, and opens the pull request.
              On your machine, on your own agent subscription.
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
              <strong>your agent running locally</strong>: Claude Code, Codex
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
                agent calls the built-in MCP tool, then tracks the PR through
                to merge and completes the issue.
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
                <a href={LINKS.downloadPage}>download page</a> (macOS, Windows,
                or Linux).
              </li>
              <li>
                <strong>Have <code>git</code> plus whichever agent CLIs you
                use on your <code>PATH</code></strong> (<code>claude</code>,
                {` `}
                <code>codex</code>, <code>pi</code>), each signed in to its own
                account. The app checks all three but only requires the one
                you pick for the run. That&apos;s the entire dependency list:
                no <code>gh</code>, no tokens to paste.
              </li>
              <li>
                <strong>Sign in</strong> to{` `}
                <code>app.exponential.at</code> or your self-hosted URL.
              </li>
              <li>
                <strong>Open a repo-backed board.</strong> The IDE clones
                the repository automatically. (Connect a repo in{` `}
                <strong>Team settings → Repositories</strong> if you
                haven&apos;t. See{` `}
                <a href="/docs/getting-started/#connect-github">
                  Getting started
                </a>
                .)
              </li>
            </ol>
            <p>
              Under the hood, the launcher wires a scoped MCP config into the
              run carrying a personal API key. That&apos;s how the agent
              drives Exponential itself: updating issue status, posting
              comments, and opening the PR, all as tools.
            </p>
            <h3>Agent accounts and usage</h3>
            <p>
              Each machine reports, read-only, which account every installed
              agent CLI is signed in to and how much of its rate-limit window
              is spent — never the credential itself. You see it on the
              machine&apos;s <strong>Device settings</strong> (a{` `}
              <strong>Login</strong> or <strong>Switch account</strong> pill
              plus usage cards inside each agent&apos;s tab) and, mid-run,
              from the session&apos;s <strong>…</strong> menu →{` `}
              <strong>Usage</strong>: a <strong>Current session</strong> group
              and the weekly windows (<strong>All models</strong> and, where
              the agent reports one, a per-model card).
            </p>
            <p>
              A signed-out agent on a remote machine can be signed in from any
              client: the machine runs the agent&apos;s own login flow and its
              code and link come back as an ordinary answerable card, so a
              headless server never needs a browser or a keyboard.
            </p>
          </DocsSection>

          {/* ── 03 Start coding ── */}
          <DocsSection id="start-coding" num="03" label="Start coding">
            <h2>Start coding</h2>
            <p>
              Hit <strong>Start coding</strong> on any issue, or check
              several on the board and start them together. One dialog covers
              both:
            </p>

            <div className="docs-embed">
              <IdeDemo view="issue" />
            </div>
            <p className="docs-embed-caption">
              Live demo: click Start coding on the issue to open the dialog.
            </p>

            <DocShot
              view="issue-detail"
              platform="desktop"
              caption="The desktop IDE on a live issue"
            />

            <ul>
              <li>
                Three tabs — <strong>Issues</strong>,{` `}
                <strong>Actions</strong> and <strong>Chat</strong> — on every
                client. Issues codes an issue, Actions runs one of the{` `}
                <a href="/docs/actions/">team&apos;s saved prompts</a>, and
                Chat takes a free <strong>Prompt</strong> plus a{` `}
                <strong>Repository</strong> and starts an agent session with
                no issue attached, in its own worktree.
              </li>
              <li>
                An <strong>agent picker</strong>:{` `}
                <strong>Claude Code</strong>, <strong>Codex</strong> or{` `}
                <strong>pi</strong>. An agent you are not signed in to is
                dimmed.
              </li>
              <li>
                A <strong>searchable multi-issue picker</strong>. Check one
                issue for a single run, two or more for a batch.
              </li>
              <li>
                A <strong>Device</strong> picker when you have more than one
                machine — your desktops and any{` `}
                <a href="/docs/cli/#daemon">CLI daemon</a>, plus servers
                teammates shared with the team. One is your{` `}
                <strong>Default device</strong> and is preselected.
              </li>
              <li>
                <strong>Model</strong> and <strong>Effort</strong> pickers, per
                agent. Each agent offers its own models and its own effort
                vocabulary (Codex calls it Reasoning, pi calls it Thinking).
              </li>
              <li>
                <strong>Ultracode</strong>, Claude only. Lets the run organize
                its own workflow; it takes over the effort setting.
              </li>
              <li>
                <strong>Plan mode</strong>, Claude and pi. It proposes a plan
                you approve before it touches code — in the terminal, or from
                the plan card in the session view on web and mobile.
              </li>
              <li>
                <strong>Resume previous session</strong>, offered when the
                issue already has a recorded run to continue. It relaunches
                that exact transcript, with the agent it was recorded on.
              </li>
            </ul>

            <DocShot
              view="start-coding-chat"
              caption="The launcher's Chat tab: a free prompt on a repository, with the same agent options"
            />
            <p>
              Defaults are <strong>per agent, not per mode</strong>: single and
              batch runs prefill identically. Out of the box that&apos;s{` `}
              <strong>plan mode on</strong> and <strong>ultracode off</strong>.
              Change them under <strong>Settings → This device → Agents</strong> on the
              desktop, per agent, and every future run starts from your values.
              Permissions are not a setting: every run hands Claude and Codex a
              full bypass, and plan mode still asks you to approve the plan
              before any code is written. Every run uses exactly one
              repository.
            </p>
            <p>
              Which branch a run starts from is resolved, never assumed: a{` `}
              board&apos;s own <strong>Branch</strong> pin wins, then the
              team&apos;s per-repository default-branch override, then
              GitHub&apos;s. A batch whose issues would resolve to different
              base branches is refused.
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
                <code>exp/&lt;IDENTIFIER&gt;</code> branch. Your main
                checkout stays untouched, and several runs can work the same
                repo side by side.
              </li>
              <li>
                The agent opens in the embedded terminal, seeded with the
                issue. With plan mode on it{` `}
                <strong>plans first</strong>; you approve before implementation
                starts.
              </li>
              <li>
                It implements, commits, pushes, and{` `}
                <strong>opens the pull request itself</strong> via the built-in
                MCP tool. The server opens the PR through the GitHub App and
                links it to the issue.
              </li>
              <li>
                The issue flips to <strong>In Review</strong> and merging the
                PR completes it to <strong>Done</strong>. Both targets are
                configurable in{` `}
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
              The batch is deliberately loose. The issues go over as a list
              and the agent organizes the work. Overlapping issues are fine,
              and often the point.
            </p>
            <h3>When to batch</h3>
            <ul>
              <li>
                <strong>Related fixes</strong>: five small bugs in one screen
                make one coherent session and one reviewable PR.
              </li>
              <li>
                <strong>Sweeping changes</strong>: a rename, an API
                migration, a copy sweep across the codebase, filed as several
                issues.
              </li>
              <li>
                <strong>Feedback triage</strong>: bulk-select a morning&apos;s
                worth of widget reports and clear them in one run.
              </li>
            </ul>
            <DocsCallout kind="note" title="Batch size">
              A run takes up to 30 issues, and the dialog shows a cost hint on
              large batches. Every checked issue adds to the prompt, so big
              batches are token-hungry.
            </DocsCallout>
          </DocsSection>

          {/* ── 06 Watch & steer ── */}
          <DocsSection id="watch-steer" num="06" label="Watch & steer">
            <h2>Watch &amp; steer</h2>
            <p>
              The embedded terminal is a <strong>real PTY</strong>, not a log
              view. Type into it any time to answer a question or redirect
              the run, and hit <strong>Stop</strong> to end the session.
            </p>
            <p>
              While a session runs, your other devices see it live: the{` `}
              <strong>Devices</strong> view on web and mobile shows the running
              session with a live activity feed, and you can{` `}
              <strong>send steer messages</strong> from your phone. The agent
              picks them up mid-run. The desktop IDE shows the same thing in
              reverse: sessions running on your <em>other</em> machines,
              including a CLI daemon, appear as chips beside its own terminal
              tabs and open the same watch-and-steer view.
            </p>
            <p>
              What the composer takes:
            </p>
            <ul>
              <li>
                Plain text, and up to <strong>four images</strong> per message
                (attach, or paste and drop on the web) — on every kind of run,
                chat and action runs included.
              </li>
              <li>
                Agent <strong>slash commands</strong>, from a{` `}
                <code>/</code> typeahead filtered to what the session&apos;s
                agent supports. There are exactly two:{` `}
                <code>/compact</code> (&ldquo;Compact the conversation
                context&rdquo;, optionally with instructions) and{` `}
                <code>/clear</code> (&ldquo;Start a fresh conversation
                (context is discarded)&rdquo;, behind a confirm — the worktree
                files are kept). While the agent folds its context the view
                shows a <strong>Compacting context…</strong> strip, and a{` `}
                <strong>Context compacted</strong> marker stays in the
                transcript.
              </li>
              <li>
                Answers to the agent&apos;s questions, its permission prompts
                and its plan card — all answerable remotely, not just at the
                desk.
              </li>
            </ul>
            <p>
              A session whose host machine goes offline reads{` `}
              <strong>Paused</strong> rather than spinning; the agent picks up
              where it left off when the machine comes back.
            </p>
            <p>
              A run you started makes no report. When the agent finishes its
              turn it waits for your next reply, in the desktop app, in a
              terminal, and on a daemon alike, with no idle timeout. End it
              yourself with <strong>Stop</strong> in the desktop terminal or{` `}
              <strong>Kill session</strong> in the web and mobile session
              view.
            </p>
            <p>
              Runs an <a href="/docs/actions/#automations">automation</a>{` `}
              started are the ones that <strong>report back</strong>: a
              one-paragraph summary that shows on the run. Those runs end on
              that report, and the Automations tab&apos;s{` `}
              <strong>Recent automated runs</strong> keeps them.
            </p>

            <DocShot
              view="terminal"
              platform="desktop"
              caption="The embedded terminal, docked under the workspace"
            />
          </DocsSection>

          {/* ── 07 Review & merge ── */}
          <DocsSection id="review-merge" num="07" label="Review & merge">
            <h2>Review &amp; merge</h2>
            <p>You never have to leave the IDE to land the work:</p>
            <ul>
              <li>
                A session&apos;s pinned <strong>Latest changes</strong> bar
                shows the branch&apos;s diff against its base, side-by-side,
                with <strong>Merge</strong> right next to it.
              </li>
              <li>
                The <strong>Reviews</strong> list in the rail collects the
                team&apos;s open PRs, across every board. Open one, read the
                diff, and <strong>merge from right there</strong>. The linked
                issues complete on merge. When a merge fails on conflicts,{` `}
                <strong>Fix conflicts</strong> replaces{` `}
                <strong>Merge</strong> in place and hands the PR to the{` `}
                <a href="/docs/actions/#builtins">Fix merge conflicts</a>{` `}
                builtin.
              </li>
            </ul>
            <p>
              Merging a PR also <strong>ends the live coding sessions</strong>
              {` `}on its issues — except the session that merged its own pull
              request, which always keeps running. Teams that would rather
              keep every session alive turn the switch off under{` `}
              <a href="/docs/issues/#branches-prs">Team settings → Statuses</a>
              .
            </p>
            <p>
              Prefer GitHub&apos;s review UI? The PR is a completely normal
              pull request. Review and merge it there and the issue completes
              just the same.
            </p>

            <DocShot
              view="review-diff"
              platform="desktop"
              caption="A PR's diff in the desktop IDE, with Merge in its header"
            />
          </DocsSection>

          {/* ── 08 The git IDE ── */}
          <DocsSection id="git-ide" num="08" label="The git IDE">
            <h2>The git IDE</h2>
            <p>
              Around the coding flow sits a git IDE. Open a board and its
              repository <strong>clones automatically</strong>. That clone is
              the <strong>trunk</strong>, kept level with the default branch by
              a background sync. Coding runs work in their own worktrees, off
              to the side.
            </p>

            <DocShot
              view="source-control"
              platform="desktop"
              caption="Source Control: the trunk's history, its graph and the working tree"
            />

            <p>
              The editor itself is <strong>view-only</strong> by design:
              changes arrive as pull requests, not local commits. The{` `}
              <strong>Files</strong> rail browses the trunk, and{` `}
              <strong>Source Control</strong> walks the commit history and
              renders any commit&apos;s diff side-by-side. It holds the two
              write affordances, both behind a confirm:{` `}
              <strong>Commit &amp; push local changes</strong> for the odd
              tweak that shouldn&apos;t wait for a PR, and{` `}
              <strong>Discard changes &amp; reset…</strong> as the escape
              hatch back to the remote.
            </p>
            <p>
              The history is drawn as a real <strong>graph</strong>: a kept{` `}
              <code>exp/…</code> PR branch renders as its own lane and curves
              back into the trunk at the squash commit that landed it. The
              Files tree has a <strong>worktree switcher</strong> on top, so
              you can browse a run&apos;s branch — its tree, its git status
              and its diffs — without leaving the trunk view, and{` `}
              <strong>Settings → This device → Worktrees</strong> prunes the
              ones whose work has landed.
            </p>
            <p>
              The terminal dock is ordinary too. Its <strong>+</strong>{` `}
              opens a plain <strong>New shell</strong>, or one of the agents
              as a steerable <a href="/docs/actions/#builtins">chat</a>{` `}
              session with its own worktree.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

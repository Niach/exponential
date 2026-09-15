import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { DocShot } from "./components/DocShot"
import { UiDemo } from "./components/UiDemo"
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
              Hand issues to a coding agent from the desktop IDE: Claude Code
              or Codex. It plans, implements, and opens the pull request.
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
              <strong>your agent running locally</strong>: Claude Code or
              Codex, on your machine, your checkout, your own agent
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
                <code>codex</code>), each signed in to its own account. The
                app checks both but only requires the one you pick for the
                run. That&apos;s the entire dependency list: no{` `}
                <code>gh</code>, no tokens to paste.
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
              is spent — never the credential itself. A machine can hold more
              than one Claude or Codex login, and a run picks the{` `}
              <strong>Account</strong> it uses.
            </p>
            <p>
              You see it in three places: on the machine&apos;s{` `}
              <strong>Device settings</strong> (a <strong>Login</strong> or
              {` `}<strong>Switch account</strong> pill plus usage cards
              inside each agent&apos;s tab), in the <strong>Accounts</strong>
              {` `}section of the <strong>Devices</strong> page (every
              account you own, one tab per agent, with the machines that hold
              it), and mid-run on the run itself. On web and desktop that is
              the context ring under the composer. On a phone it is the
              run&apos;s <strong>…</strong> menu.
            </p>

            <DocShot
              view="usage"
              caption="Accounts on the Devices page: each agent account, the machines that hold it, and what is left of each window"
            />

            <p>
              A signed-out agent on a remote machine can be signed in from any
              client: the machine runs the agent&apos;s own login flow, its
              link and code come back as an ordinary answerable card, and you
              type the code straight into the client you are already in — so a
              headless server never needs a browser or a keyboard.
            </p>
          </DocsSection>

          {/* ── 03 Start coding ── */}
          <DocsSection id="start-coding" num="03" label="Start coding">
            <h2>Start coding</h2>
            <p>
              Hit <strong>Start coding</strong> on any issue, or check several
              on the board and start them together. Every play button lands on
              the same place — the <strong>Agent page composer</strong>, with
              what you picked already chipped above the prompt:
            </p>

            <div className="docs-embed">
              <IdeDemo view="issue" />
            </div>
            <p className="docs-embed-caption">
              Live demo: click Start coding on the issue to open the composer.
            </p>

            <DocShot
              view="issue-detail"
              platform="desktop"
              caption="An issue in the desktop IDE with its run live: the Issue | Run switch, the property tray, and Stop"
            />

            <UiDemo id="subject-chips" />
            <p className="docs-embed-caption">
              The subject row, drawn by the composer&apos;s own components:
              issue chips — two or more make a batch — or one action chip,
              each dropped by its ✕.
            </p>

            <ul>
              <li>
                One <strong>subject</strong>, as chips over the prompt:{` `}
                <strong>issue chips</strong> (one issue codes that issue, two
                or more make a batch) <em>or</em> one{` `}
                <strong>action chip</strong> that runs one of the{` `}
                <a href="/docs/actions/">team&apos;s saved prompts</a>. Picking
                the other kind swaps it — nothing is ever disabled. The{` `}
                <strong>#</strong> button opens the issue picker,{` `}
                <strong>▶</strong> the action picker.
              </li>
              <li>
                <strong>Free text</strong> in the box. With no subject it is
                the prompt of a plain chat session; beside a subject it is
                additional instructions. A <strong>Repository</strong> pick
                appears for a subject-less chat: pick one and it gets its own
                worktree, leave it out and it runs in a scratch directory with
                the Exponential MCP tools wired up either way. Images can be
                dropped or pasted in.
              </li>
              <li>
                An <strong>agent picker</strong> on the muted options line
                under the box: <strong>Claude Code</strong> or{` `}
                <strong>Codex</strong>. Only the agents the chosen machine
                reports are offered.
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
                vocabulary (Codex calls it Reasoning). Effort is fixed at
                launch; a Claude run can still change model from its composer.
              </li>
              <li>
                <strong>Ultracode</strong>, Claude only. Lets the run organize
                its own workflow; it takes over the effort setting.
              </li>
              <li>
                <strong>Plan mode</strong>, Claude and Codex. It proposes
                a plan you approve before it touches code, as a card in the
                session view — on every client, including your phone.
              </li>
              <li>
                <strong>Resume previous session</strong>, offered when the
                issue already has a recorded run to continue. It relaunches
                that exact transcript, with the agent it was recorded on.
              </li>
              <li>
                An <strong>Account</strong> picker on machines that hold more
                than one login for the agent, and an{` `}
                <strong>MCP servers</strong> picker once your team has any:
                pick which of them this run gets. See{` `}
                <a href="/docs/mcp/#your-own-servers">MCP servers</a>.
              </li>
            </ul>

            <DocShot
              view="chat-issues"
              caption="The Agent page composer with two issues chipped above the prompt, the machine and agent pickers under it: one run for a single issue, a batch for several"
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
                The run opens as a <strong>session</strong>, seeded with the
                issue. On web and desktop it is the <strong>Run</strong> face
                of the issue&apos;s tab.
                With plan mode on it <strong>plans first</strong>; you approve
                before implementation starts.
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
              Chip <strong>two or more issues</strong> in the composer (or
              use the board&apos;s bulk-select bar) and you get a batch run:{` `}
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
              A run takes <em>at most 30 issues</em>. Every checked issue adds
              to the prompt, so big batches are token-hungry.
            </DocsCallout>
          </DocsSection>

          {/* ── 06 Watch & steer ── */}
          <DocsSection id="watch-steer" num="06" label="Watch & steer">
            <h2>Watch &amp; steer</h2>
            <p>
              A run is a <strong>session</strong> with one address,{` `}
              <code>/t/&lt;team&gt;/sessions/&lt;id&gt;</code>. What it shows
              is not a log or a terminal — it is the agent&apos;s own
              narration, the tool calls it makes (collapsed into lines like
              &ldquo;Ran 4 commands · edited 2 files&rdquo;, with an
              edit&apos;s diff foldable under its row), and the cards it wants
              answered.
            </p>
            <p>
              On web and desktop, work lives in <strong>tabs</strong> along
              the top of the window. An issue and its run share one tab, and
              the <strong>Issue | Run</strong> switch in its header flips
              between them. Both faces use the same header: the title, then
              the issue&apos;s properties in a tray, with{` `}
              <strong>Stop</strong> or <strong>Resume</strong> at its end. A
              run with no issue (a chat, an action, a batch) gets a tab of its
              own.
            </p>
            <p>
              Every live run of yours gets a tab automatically, grouped by
              agent at the front of the strip. A live run&apos;s tab
              can&apos;t be closed. Once the run ends it becomes an ordinary
              tab you close yourself. The same runs are listed on the{` `}
              <strong>Agent</strong> page, <strong>Running</strong> then{` `}
              <strong>Past</strong>, and a running row carries{` `}
              <strong>Merge</strong> and <strong>Open issue</strong> buttons.
              On a phone the Agent page is where you open them. Live sessions
              are yours alone — teammates see the status badge on the issue,
              never the transcript.
            </p>

            <DocShot
              view="steering"
              platform="desktop"
              caption="A run in the desktop IDE: the same header as its issue, the transcript, and the agent's question waiting for an answer"
            />

            <p>What the composer takes:</p>
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
                Answers to the agent&apos;s questions and its plan card. The
                options are numbered buttons — keys <code>1</code> to{` `}
                <code>9</code> and <code>Enter</code> pick them — and typing
                your own answer into the composer answers the card just as
                well. The plan always shows in full; question cards and your
                own messages fold behind <strong>Show more</strong>.
              </li>
            </ul>
            <p>
              The composer is one box with its send button inside. The row
              under it says whether the run is in <strong>Plan mode</strong>,
              holds the attach button, and names the model the run is on. A
              Claude run can switch model right there, which sends{` `}
              <code>/model</code> to the agent; Codex shows its model as a
              label. On web and desktop the row ends in a{` `}
              <strong>context ring</strong> that fills as the context window
              does. Click it for the run&apos;s usage: its context, the
              account&apos;s limits, and the machine&apos;s other accounts you
              can switch the run to.
            </p>
            <p>
              Everything else about the run is picked when you start it: the
              agent, its effort and plan mode. A live session is steered with
              words.
            </p>
            <p>
              The transcript keeps the <strong>whole run</strong>, not the
              last few hundred events. Each client renders a window of it and
              pulls the rest in as you scroll to the top; past what your
              client already holds, <strong>Load earlier</strong> fetches the
              next page from the machine that ran the session — the full
              transcript lives on <em>that</em> machine, never on our servers.
              Past transcripts stay there for as long as that machine&apos;s
              {` `}<strong>Settings → Sessions</strong> allows (unlimited by
              default). The <strong>Agent</strong> page shows your 20 most
              recent runs; when an issue has more than one run of yours, its
              {` `}<strong>Run</strong> toggle reads <strong>Runs</strong> and
              the run&apos;s page carries a picker that opens any of them.
            </p>
            <p>
              A session whose host machine goes offline reads{` `}
              <strong>Paused</strong> rather than spinning; the agent picks up
              where it left off when the machine comes back. One that hits its
              agent&apos;s usage limit is marked{` `}
              <strong>Rate limited</strong> with the time it resets, instead
              of going quiet — the run stays live and steerable, it simply
              cannot make a call until then. You can see how much is left on
              any machine under <strong>Accounts</strong> on the{` `}
              <strong>Devices</strong> page.
            </p>
            <p>
              A run you started makes no report. When the agent finishes its
              turn it waits for your next reply, in the desktop app and on a
              daemon alike, with no idle timeout. End it yourself with{` `}
              <strong>Stop</strong>. On web and desktop it sits in the
              run&apos;s header (in the property tray for an issue run); on a
              phone it is in the top bar. Once a run has ended,{` `}
              <strong>Resume</strong> takes its place and relaunches it.
            </p>
            <p>
              Runs an <a href="/docs/actions/#automations">automation</a>{` `}
              started are the ones that <strong>end themselves</strong>: the
              agent closes the run when its work is done, and the Automations
              tab&apos;s <strong>Recent automated runs</strong> keeps them.
            </p>

            <h3>Chat</h3>
            <p>
              Not every question is an issue. <strong>Agent</strong> in the
              sidebar on web and desktop, and the chat button beside the
              phone&apos;s tab bar, open the Agent page. With nothing chipped,
              its composer starts a conversation with your agent that is bound
              to no issue and needs no repository. It runs on one of your
              machines like any other session, with the Exponential tools
              wired up, so it can read and write the tracker while you talk to
              it. Your runs are listed under the prompt, running first, then
              past. Open an ended one to read it back, or to{` `}
              <strong>Resume</strong> it if its machine still can.
            </p>

            <DocShot
              view="chat"
              caption="The Agent page: the composer, the machine and agent it runs on, and your running and past runs"
            />
          </DocsSection>

          {/* ── 07 Review & merge ── */}
          <DocsSection id="review-merge" num="07" label="Review & merge">
            <h2>Review &amp; merge</h2>
            <p>You never have to leave the IDE to land the work:</p>
            <ul>
              <li>
                A run&apos;s changes are a full-page face of its tab. Once the
                run has edited files, its header switch gains a{` `}
                <strong>+N -M</strong> segment that opens the diff in place of
                the transcript. A diff card in the transcript opens the same
                page, scoped to that turn. <strong>Merge PR</strong> sits in
                the header once the pull request is open.
              </li>
              <li>
                The <strong>Reviews</strong> list in the rail collects the
                team&apos;s open PRs, across every board. Open one, read the
                diff, and <strong>merge from right there</strong>. The linked
                issues complete on merge. When a merge fails on conflicts,{` `}
                <strong>Fix conflicts</strong> replaces{` `}
                <strong>Merge</strong> in place and hands the PR to the{` `}
                <a href="/docs/actions/#builtins">Fix merge conflicts</a>{` `}
                builtin. An issue whose own merge hits a conflict offers the
                same swap in its header, beside <strong>Retry merge</strong>.
              </li>
              <li>
                Pull requests opened by an{` `}
                <a href="/docs/actions/">action</a> or chat run, with no issue
                attached, get their own <strong>Agent runs</strong> group in
                Reviews, with the action name, branch and PR number, and count
                toward the Reviews badge. The run itself shows{` `}
                <strong>Merge</strong> too: in its header, on its running row
                in the Agent lists, and on the phone&apos;s{` `}
                <strong>Changes</strong> bar.
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
              There is a terminal too, and it is an ordinary one: a plain
              shell on the trunk clone, opened with the button beside your
              account in the rail or <kbd>⌘T</kbd>. It fills the working area
              the way an issue or a run does, and its tabs sit in the bar along the
              bottom of the window — a bar that is not there at all until you
              open one. Agents never run in it: a coding session talks to the
              agent directly, and the terminal is yours.
            </p>

            <DocShot
              view="terminal"
              platform="desktop"
              caption="A plain shell in the IDE, one tab in the bar along the bottom"
            />
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

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
  { id: `single-runs`, num: `04`, label: `Single sessions` },
  { id: `batch-runs`, num: `05`, label: `Several issues` },
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
                through the team&apos;s GitHub App connection, so the session can
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
              session carrying a personal API key. That&apos;s how the agent
              drives Exponential itself: updating issue status, posting
              comments, and opening the PR, all as tools.
            </p>
            <h3>Agent accounts and usage</h3>
            <p>
              Each machine reports, read-only, which account every installed
              agent CLI is signed in to and how much of its rate-limit window
              is spent — never the credential itself. A machine can hold more
              than one Claude or Codex login, and a session picks the{` `}
              <strong>Account</strong> it uses.
            </p>
            <p>
              You see it in two places. The <strong>Devices</strong> page
              lists each machine with the logins it holds underneath it —
              which account, whether it still works, and what is left of each
              window; repairing one (a <strong>Login</strong> or{` `}
              <strong>Switch account</strong> pill) happens right there, on
              the login&apos;s own row. And mid-session the context ring beside
              the composer opens the same usage overlay — the same one, on
              every client, phones included.
            </p>

            <DocShot
              view="agents"
              caption="The Devices page: each machine with the agent logins it holds, their health, and what is left of each window"
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
              caption="An issue in the desktop IDE: the property tray ending in Start coding, and the chip of the thread already working it"
            />

            <UiDemo id="subject-chips" />
            <p className="docs-embed-caption">
              The subject row, drawn by the composer&apos;s own components:
              issue chips, or one prompt chip, each dropped by its ✕.
            </p>

            <ul>
              <li>
                One <strong>subject</strong>, as chips over the prompt:{` `}
                <strong>issue chips</strong> (one issue or several, one
                thread either way), <em>or</em> one <strong>prompt chip</strong>
                {` `}from the <a href="/docs/actions/">team&apos;s saved
                prompts</a>, <em>or</em> one <strong>agent chip</strong> that
                turns the send into a message to that{` `}
                <a href="/docs/actions/#triggers">team agent</a>. Picking
                another kind swaps it — nothing is ever disabled. The{` `}
                <strong>+</strong> menu holds <strong>Implement issue</strong>
                {` `}(a searchable list) and <strong>Message an agent</strong>;
                typing <code>/</code> lists the prompts.
              </li>
              <li>
                <strong>Free text</strong> in the box. With no subject it is
                the whole request; beside a subject it is additional
                instructions. A <strong>Repository</strong> pick appears for a
                free request: pick one and the session gets its own worktree,
                leave it out and it runs in a scratch directory with the
                Exponential MCP tools wired up either way. Files and images
                can be dropped or pasted in.
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
                A <strong>Model</strong> picker on the options line and{` `}
                <strong>Effort</strong> in the <strong>+</strong> menu, per
                agent. Each agent offers its own models and its own effort
                vocabulary (Codex calls it Reasoning). A live session changes
                both from its composer, <code>/model</code> and{` `}
                <code>/effort</code>.
              </li>
              <li>
                <strong>Ultracode</strong>, Claude only, in the{` `}
                <strong>+</strong> menu. Lets the session organize its own
                workflow; it takes over the effort setting.
              </li>
              <li>
                <strong>Plan mode</strong>, Claude and Codex. It proposes
                a plan you approve before it touches code, as a card in the
                session view — on every client, including your phone.
              </li>
              <li>
                <strong>Resume previous session</strong>, offered when the
                issue already has a recorded session to continue. It relaunches
                that exact transcript, with the agent it was recorded on.
              </li>
              <li>
                An <strong>Account</strong> picker on machines that hold more
                than one login for the agent, and an{` `}
                <strong>MCP servers</strong> entry in the <strong>+</strong>
                {` `}menu once your team has any: pick which of them this
                session gets. See{` `}
                <a href="/docs/mcp/#your-own-servers">MCP servers</a>.
              </li>
            </ul>

            <DocShot
              view="chat-issues"
              caption="The Agent page composer with two issues chipped above the prompt, the machine and agent pickers under it: one thread, whether it takes one issue or several"
            />
            <p>
              Defaults are <strong>per agent</strong>: every start prefills
              the same way. Out of the box that&apos;s{` `}
              <strong>plan mode on</strong> and <strong>ultracode off</strong>.
              Change them under <strong>Settings → This device → Agents</strong> on the
              desktop, per agent, and every future session starts from your values.
              Permissions are not a setting: every session hands Claude and Codex a
              full bypass, and plan mode still asks you to approve the plan
              before any code is written. Every session uses exactly one
              repository.
            </p>
            <p>
              Which branch a session starts from is resolved, never assumed: a{` `}
              board&apos;s own <strong>Branch</strong> pin wins, then the
              team&apos;s per-repository default-branch override, then
              GitHub&apos;s. A start whose issues would resolve to different
              base branches is refused.
            </p>
          </DocsSection>

          {/* ── 04 Single sessions ── */}
          <DocsSection id="single-runs" num="04" label="Single sessions">
            <h2>Single sessions</h2>
            <p>One issue, one branch, one PR:</p>
            <ol>
              <li>
                The app creates a <strong>git worktree</strong> on a fresh
                {` `}
                <code>exp/&lt;IDENTIFIER&gt;</code> branch. Your main
                checkout stays untouched, and several sessions can work the same
                repo side by side.
              </li>
              <li>
                The session opens seeded with the issue, as a{` `}
                <strong>thread</strong>: its Conversation, the issue&apos;s chip,
                the session&apos;s own work one tab over, and the Guide. With
                plan mode on it <strong>plans first</strong>; you approve
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

          {/* ── 05 Several issues ── */}
          <DocsSection id="batch-runs" num="05" label="Several issues">
            <h2>Several issues at once</h2>
            <p>
              Chip <strong>two or more issues</strong> in the composer (or
              use the board&apos;s bulk-select bar) and you get{` `}
              <strong>one thread</strong> given all the issues at once, on{` `}
              <strong>one branch</strong>, ending in <strong>one PR</strong>
              {` `}linked to every issue it worked. Merging that PR completes
              them all.
            </p>
            <p>
              The request is deliberately loose. The issues go over as a list
              and the agent organizes the work. Overlapping issues are fine,
              and often the point.
            </p>
            <h3>When to start them together</h3>
            <ul>
              <li>
                <strong>Related fixes</strong>: five small bugs in one screen
                make one coherent thread and one reviewable PR.
              </li>
              <li>
                <strong>Sweeping changes</strong>: a rename, an API
                migration, a copy sweep across the codebase, filed as several
                issues.
              </li>
              <li>
                <strong>Feedback triage</strong>: bulk-select a morning&apos;s
                worth of widget reports and clear them in one thread.
              </li>
            </ul>
            <DocsCallout kind="note" title="Size">
              A start takes <em>at most 30 issues</em>. Every checked issue adds
              to the prompt, so big requests are token-hungry.
            </DocsCallout>
            <p>
              <strong>A thread grows as the work does.</strong> The agent
              starts child sessions for the parts it can run side by side, and
              work it finds out of scope becomes a new issue. Every session of
              the tree shows as a card in the thread&apos;s Conversation and
              nests under it on the Threads page; a child that opens its own
              pull request stacks it on the root&apos;s, and{` `}
              <strong>Merge stack</strong> lands them together.
            </p>
          </DocsSection>

          {/* ── 06 Watch & steer ── */}
          <DocsSection id="watch-steer" num="06" label="Watch & steer">
            <h2>Watch &amp; steer</h2>
            <p>
              Every start is a <strong>thread</strong> with one address,{` `}
              <code>/t/&lt;team&gt;/threads/&lt;id&gt;</code>. Its header is
              the title over one strip of tabs: <strong>Conversation</strong>,
              a chip per linked issue, a chip per session of the tree, and the{` `}
              <strong>Guide</strong>. The Conversation holds your request, one
              card per session the agent started, its replies, and the
              questions it asked, answered right in their card. A session chip
              opens that session&apos;s own work in place, which is not a log
              or a terminal — it is the agent&apos;s own narration, the tool
              calls it makes (collapsed into lines like &ldquo;Ran 4 commands ·
              edited 2 files&rdquo;, with an edit&apos;s diff foldable under
              its row), and the cards it wants answered.
            </p>
            <p>
              <strong>Threads</strong> in the sidebar lists every thread of
              the team in three bands: <strong>Needs you</strong> (a question,
              a plan or a usage wall waits on a person), <strong>Running</strong>
              {` `}and <strong>Recent</strong> (the last day, then the last
              twenty). A row is its mark, its title, one quiet word (question ·
              ready to merge · stack) and the device it runs on; a live tree
              nests its sessions under the root. The rail&apos;s{` `}
              <strong>Active</strong> section keeps the threads that need you
              or are running one click away, and on web and desktop an open
              thread is a tab along the top of the window. An unread thread
              reads bold until you open it.
            </p>

            <DocShot
              view="steering"
              platform="desktop"
              caption="A session in the desktop IDE: the thread's strip, the agent's work, and its question waiting for an answer"
            />

            <p>What the composer takes:</p>
            <ul>
              <li>
                Plain text, and up to <strong>four images</strong> and four
                files per message (attach, or paste and drop on the web) — in
                every thread, whatever started it.
              </li>
              <li>
                <strong>Slash commands</strong>, from a <code>/</code>{` `}
                typeahead: <code>/compact</code> (&ldquo;Compact the
                conversation context&rdquo;, optionally with instructions),{` `}
                <code>/model</code> and <code>/effort</code>, then the
                team&apos;s <a href="/docs/actions/">prompts</a>, whose body
                lands in your draft. Commands the agent CLI injects on its own
                never show. While the agent folds its context the view shows
                a <strong>Compacting context…</strong> strip, and a{` `}
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
              under it says whether the session is in <strong>Plan mode</strong>,
              holds the attach button, and names the model the session is on. A
              Claude session can switch model right there, which sends{` `}
              <code>/model</code> to the agent; Codex shows its model as a
              label. On web and desktop the row ends in a{` `}
              <strong>context ring</strong> that fills as the context window
              does. Click it for the session&apos;s usage: its context, the
              account&apos;s limits, and the machine&apos;s other accounts you
              can switch the session to.
            </p>
            <p>
              The agent and the machine are picked when you start; effort,
              ultracode, MCP servers and plan mode change mid-session from the
              composer&apos;s <strong>+</strong> menu. Everything else is
              steered with words.
            </p>
            <p>
              The transcript keeps the <strong>whole session</strong>, not the
              last few hundred events. Each client renders a window of it and
              pulls the rest in as you scroll to the top; past what your
              client already holds, <strong>Load earlier</strong> fetches the
              next page from the machine that ran the session — the full
              transcript lives on <em>that</em> machine, never on our servers.
              Past transcripts stay there for as long as that machine&apos;s
              {` `}<strong>Settings → Sessions</strong> allows (unlimited by
              default). <strong>Threads</strong> keeps the last day of threads
              in Recent, then the last twenty; an issue names every thread
              that worked it, one chip each under its header.
            </p>
            <p>
              A session whose host machine goes offline reads{` `}
              <strong>Paused</strong> rather than spinning; the agent picks up
              where it left off when the machine comes back. One that hits its
              agent&apos;s usage limit is marked{` `}
              <strong>Rate limited</strong> with the time it resets, instead
              of going quiet — the session stays live and steerable, it simply
              cannot make a call until then. You can see how much is left on
              any machine under <strong>Accounts</strong> on the{` `}
              <strong>Devices</strong> page.
            </p>
            <p>
              A session you started makes no report. When the agent finishes its
              turn it waits for your next reply, in the desktop app and on a
              daemon alike, with no idle timeout. End it yourself with{` `}
              <strong>Stop</strong>, in the thread&apos;s header while a
              session chip is open; on a phone it is in the top bar. Writing
              to an ended thread continues it in a new session on the same
              branch.
            </p>
            <p>
              Sessions a <a href="/docs/actions/#triggers">team agent</a>{` `}
              started on a schedule or an event are the ones that{` `}
              <strong>end themselves</strong>: the agent closes the session
              when its work is done, and reports into its own page.
            </p>

            <h3>Free requests and agents</h3>
            <p>
              Not every question is an issue. <strong>Agent</strong> in the
              sidebar on web and desktop, and the chat button beside the
              phone&apos;s tab bar, open the Agent page: the home of every
              client, a greeting over the composer and nothing else. With
              nothing chipped, the send starts a thread bound to no issue
              that needs no repository. It runs on one of your machines like
              any other session, with the Exponential tools wired up, so it
              can read and write the tracker while you talk to it. Every
              thread lives on the Threads page; open an ended one to read it
              back, or write to it to continue.
            </p>
            <p>
              A <strong>team agent</strong> is a saved definition: a name and
              icon, the device and login it runs on, markdown instructions,
              and triggers (a schedule, or an event such as an issue being
              created). Every member has a private conversation with each
              agent — <strong>Message an agent</strong> in the composer&apos;s{` `}
              <strong>+</strong> menu, or the agent&apos;s row in the sidebar.
              Its page reads like a chat: the agent&apos;s reports, your
              messages, and one card per thread it started for you.
            </p>

            <DocShot
              view="chat"
              caption="The Agent page: the greeting and the one composer, with the machine, login and model it will use under it"
            />
          </DocsSection>

          {/* ── 07 Review & merge ── */}
          <DocsSection id="review-merge" num="07" label="Review & merge">
            <h2>Review &amp; merge</h2>
            <p>You never have to leave the IDE to land the work:</p>
            <ul>
              <li>
                The thread&apos;s <strong>Guide</strong> tab is the pull
                request&apos;s body, written by the agent: a Summary, then one
                section per change with its screenshots and a{` `}
                <strong>Changes</strong> row that opens that part of the diff.
                The Guide gets a dot in the strip as soon as it holds anything.
              </li>
              <li>
                While the PR is open the Guide ends in the one{` `}
                <strong>Merge</strong> capsule; a PR stacked on another open
                one reads <strong>Merge stack</strong> and lands the whole
                stack, bottom up. The linked issues complete on merge. The
                thread header&apos;s <strong>…</strong> menu holds{` `}
                <strong>Close PR</strong>. On the Threads page a thread whose
                PR waits reads <strong>ready to merge</strong>, a stacked one{` `}
                <strong>stack</strong>, and its mark turns green while the PR
                is open and violet once it is merged.
              </li>
              <li>
                A merge that hits conflicts comes back to the thread: write
                to it and the session rebases, pushes and tries again.
              </li>
            </ul>
            <p>
              Prefer GitHub&apos;s review UI? The PR is a completely normal
              pull request. Review and merge it there and the issue completes
              just the same.
            </p>

            <DocShot
              view="guide"
              platform="desktop"
              caption="A thread's Guide in the desktop IDE: the sections, a Changes row each, the Merge capsule at the bottom"
            />
          </DocsSection>

          {/* ── 08 The git IDE ── */}
          <DocsSection id="git-ide" num="08" label="The git IDE">
            <h2>The git IDE</h2>
            <p>
              Around the coding flow sits a git IDE. Open a board and its
              repository <strong>clones automatically</strong>. That clone is
              the <strong>trunk</strong>, kept level with the default branch by
              a background sync. Coding sessions work in their own worktrees, off
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
              you can browse a session&apos;s branch — its tree, its git status
              and its diffs — without leaving the trunk view, and{` `}
              <strong>Settings → This device → Worktrees</strong> prunes the
              ones whose work has landed.
            </p>
            <p>
              There is a terminal too, and it is an ordinary one: a plain
              shell on the trunk clone, opened with the button beside your
              account in the rail or <kbd>⌘T</kbd>. It fills the working area
              the way an issue or a session does, and its tabs sit in the bar along the
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

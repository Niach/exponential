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
              The <a href="/docs/apps/">desktop app</a> and the{` `}
              <a href="/docs/cli/#daemon">CLI daemon</a> are the clients that
              run coding sessions. When you start one, the machine hands the
              issue to
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
                <strong>Have <code>git</code> on your <code>PATH</code>,
                plus a coding agent</strong>: Claude Code&apos;s{` `}
                <code>claude</code> CLI, or Codex, which needs no install (the
                app downloads its own pinned Codex build the first time you
                sign in to it). One agent is enough. The app&apos;s first-run
                step, <strong>Set up this device</strong>, shows what this
                machine has: <strong>Git</strong>{` `}
                under <strong>Required</strong>, <strong>Claude Code</strong>{` `}
                and <strong>Codex</strong> under{` `}
                <strong>Coding agents</strong>, each with its{` `}
                <strong>Install</strong>, <strong>Sign in</strong> or{` `}
                <strong>Update</strong>. That&apos;s the entire dependency
                list: no <code>gh</code>, no tokens to paste.
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
              The <strong>Devices</strong> page lists each machine with the
              logins it holds underneath it: which account, whether it still
              works, and what is left of each window. Repairing one happens
              right there, in the login&apos;s <strong>⋯</strong> menu
              (<strong>Sign in</strong>, <strong>Sign out</strong>,{` `}
              <strong>Remove account</strong>), and{` `}
              <strong>Add account</strong> signs in another one. With{` `}
              <strong>Auto-rotate accounts</strong> on in the machine&apos;s{` `}
              <strong>Device settings</strong>, a session that runs into a
              usage limit moves to another login of the same agent by
              itself, or waits for the reset. Mid-session, a
              usage limit shows as a <strong>Usage limit</strong> card above
              the composer with <strong>Switch account</strong>: pick another
              login and the same session continues on it, on the same model,
              between turns.
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
              Hit <strong>Start</strong> in any issue&apos;s header, or check
              several on the board and hit <strong>Start coding</strong> on
              the selection bar. Every play button lands on
              the same place — the <strong>Agent page composer</strong>, with
              what you picked already chipped above the prompt.
            </p>
            <p>
              The button is always there. While something is missing it is a
              dashed amber capsule, and a click opens{` `}
              <strong>Ready to code?</strong>: <strong>GitHub connected</strong>,{` `}
              <strong>Repository connected</strong> and{` `}
              <strong>Device online</strong>, with the fix for the current step
              inline (<strong>Connect GitHub</strong>,{` `}
              <strong>Choose repository</strong>, <strong>Open Devices</strong>,{` `}
              <strong>Get the desktop app</strong> or{` `}
              <strong>Set up a server</strong>).
            </p>

            <div className="docs-embed">
              <IdeDemo view="issue" />
            </div>
            <p className="docs-embed-caption">
              Live demo: click Start on the issue to open the composer.
            </p>

            <DocShot
              view="issue-detail"
              platform="desktop"
              caption="An issue in the desktop IDE: Start in its header, and under the title the chip of the thread already working it"
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
                {` `}(a searchable list), <strong>Message an agent</strong> and{` `}
                <strong>Add file or image</strong>; typing <code>/</code>{` `}
                lists the prompts.
              </li>
              <li>
                <strong>Free text</strong> in the box. With no subject it is
                the whole request; beside a subject it is additional
                instructions. A <strong>Repository</strong> pick appears for a
                free request when the team has several repositories, and the
                session gets its own worktree in it; a request with no
                repository runs in a scratch directory, with the Exponential
                MCP tools wired up either way. Files and images can be dropped
                or pasted in.
              </li>
              <li>
                The muted options line under the box holds{` `}
                <strong>Device</strong>, <strong>Account</strong>,{` `}
                <strong>Model</strong>, <strong>Plan</strong> and, for a free
                request, <strong>Repository</strong>. There is no separate
                agent picker: the <strong>Account</strong> picker lists the
                machine&apos;s Claude Code and Codex logins by brand mark and
                email, and picking a login picks its agent.
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
                vocabulary (Codex calls it Reasoning). Claude also takes a{` `}
                <strong>Subagents</strong> model in the <strong>+</strong>{` `}
                menu. A live session changes model and effort from its
                composer, <code>/model</code> and <code>/effort</code>.
              </li>
              <li>
                <strong>Ultracode</strong>, Claude only, in the{` `}
                <strong>+</strong> menu. Lets the session organize its own
                workflow; it takes over the effort setting.
              </li>
              <li>
                <strong>Plan</strong>, Claude and Codex alike: plan mode is
                Exponential&apos;s own. The session researches, may draft UI on
                its branch to picture it, and then posts one plan card with
                its pictures, choice questions and even a small interactive
                preview, which you answer before it touches code — on every
                client, including your phone.
              </li>
              <li>
                <strong>MCP servers</strong> in the <strong>+</strong> menu
                once your team has any: pick which of them this session gets.
                See <a href="/docs/mcp/#your-own-servers">MCP servers</a>.
              </li>
              <li>
                <strong>Computer use</strong> in the <strong>+</strong> menu
                when the machine offers it. See{` `}
                <a href="#computer-use">Computer use</a>.
              </li>
            </ul>

            <DocShot
              view="chat-issues"
              caption="The Agent page composer with two issues chipped above the prompt, the device, account and model under it: one thread, whether it takes one issue or several"
            />
            <p>
              Defaults are <strong>per agent</strong>: every start prefills
              the same way. Out of the box that&apos;s{` `}
              <strong>plan mode on</strong> for Claude and{` `}
              <strong>ultracode off</strong>. Change them under{` `}
              <strong>Settings → This device → Coding CLIs</strong> on the
              desktop, or in any machine&apos;s <strong>Device settings</strong>{` `}
              from any client, and every future session starts from your values.
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
                <strong>thread</strong>: the issue moves to{` `}
                <strong>In Progress</strong>, and the thread&apos;s{` `}
                <strong>Conversation</strong> tab is the session&apos;s own
                work, beside the issue&apos;s chip and the Guide. With plan
                mode on it <strong>plans first</strong>; you approve before
                implementation starts.
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
              the title over one strip of tabs that never scrolls away: a{` `}
              <strong>+</strong> (<strong>Link an issue</strong> or{` `}
              <strong>New issue</strong>), <strong>Conversation</strong>, a
              chip per linked issue, a chip per session the agent started, and
              the <strong>Guide</strong> pinned on the right; deeper sessions
              and sub-issues fold under their chip&apos;s chevron. While the
              thread is one session, its Conversation is that session&apos;s
              own work. Once the agent starts a child session, the
              Conversation becomes the thread&apos;s own: your messages, one
              card per session, each child&apos;s report when it ends, its
              questions and plans, answered right in their card, and the
              tasks it suggests for you to start. A session chip opens that
              session&apos;s work in place.
            </p>
            <p>
              A session&apos;s work is not a log or a terminal: it is the
              agent&apos;s own narration and the tool calls it makes, one
              line each (&ldquo;Ran ./gradlew :app:startupProfile&rdquo;,
              &ldquo;Thought for 5s&rdquo;, a failed test run in red), with
              grouped calls folded into lines like &ldquo;Ran 4 commands ·
              edited 2 files&rdquo; and an edit&apos;s diff foldable under
              its row. Sessions name themselves: an explicit name or the
              agent&apos;s own title, else its issues, its prompt, or your
              first message.
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
              caption="A session in the desktop IDE: the thread's strip, the agent's work, its question waiting above the composer"
            />

            <h3>The Dock</h3>
            <p>
              The transcript is the one scroll. Whatever waits on you floats
              above the composer as a card: a <strong>Plan ready</strong> or{` `}
              <strong>Question</strong> card, the session&apos;s task list
              (&ldquo;3 to do · 2 completed&rdquo;), your queued messages
              (&ldquo;2 queued&rdquo;), its suggested tasks, and status
              banners: the device being offline, <strong>Connection lost</strong>{` `}
              with <strong>Reconnect</strong>, <strong>Usage limit</strong>{` `}
              with <strong>Switch account</strong>, <strong>Compacting</strong>{` `}
              or <strong>Rate limit</strong>. Several cards pile up, the front
              one whole; hover or tap the pile to fan it out. Click a card and
              it opens as a sheet over the composer with the whole content and
              its answer.
            </p>

            <DocShot
              view="dock-card"
              caption="Dock cards: a question, a usage banner with Switch account, and the pile fanned out"
            />

            <p>
              A plan opens with its decision at the bottom:{` `}
              <strong>Yes</strong> (its menu also holds{` `}
              <strong>Yes, and start with a fresh context</strong>) or{` `}
              <strong>No, keep planning</strong>, which opens a field for
              what to change. A plan&apos;s choice questions are option rows,
              or picture tiles when the agent drew its options, and are
              answered in the card. Question options are numbered buttons —
              keys <code>1</code> to <code>9</code> and <code>Enter</code>{` `}
              pick them — and typing your own answer into the composer answers
              the card just as well. Answered cards stay in the transcript,
              closed, with the answer shown.
            </p>

            <DocShot
              view="plan-card"
              platform="desktop"
              caption="A plan card: the plan with its picture, an interactive preview, and a choice question"
            />

            <h3>The composer</h3>
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
                never show. While the agent folds its context a{` `}
                <strong>Compacting</strong> card reads{` `}
                <strong>Compacting context…</strong>, and a{` `}
                <strong>Context compacted</strong> marker stays in the
                transcript.
              </li>
              <li>
                <strong>Quick replies</strong>: when the agent offers short
                answers, they sit as pills directly above the composer. A tap
                sends one at once, even mid-turn; the ✕ dismisses them.
              </li>
            </ul>
            <p>
              The composer is one box with one row inside it: the{` `}
              <strong>+</strong> menu (<strong>Effort</strong>,{` `}
              <strong>Ultracode</strong>, <strong>MCP servers</strong>,{` `}
              <strong>Computer use</strong>, <strong>Add file or image</strong>),
              the device glyph, the model, the <strong>Plan</strong> switch,
              and the <strong>context ring</strong> beside Send, which turns
              into Stop while the agent works. A Claude session can switch
              model right there, which sends <code>/model</code> to the agent;
              Codex shows its model as a label. The ring fills as the context
              window does; click it for <strong>Usage</strong>: what the
              context holds, layer by layer.
            </p>
            <p>
              The device glyph moves the session: pick another of the
              team&apos;s machines under <strong>Move to device</strong> and
              the session ends where it is and continues there, on the same
              branch, with its conversation; the glyph reads{` `}
              <strong>Moving to &lt;device&gt;…</strong> until it lands. A
              machine that is offline, has no login for the agent, or needs an
              update says why on its row. Everything else is steered with
              words.
            </p>
            <p>
              The transcript keeps the <strong>whole session</strong>, not the
              last few hundred events. Each client renders a window of it and
              pulls the rest in as you scroll to the top; past what your
              client already holds, <strong>Load earlier</strong> fetches the
              next page from the machine that ran the session — the full
              transcript lives on <em>that</em> machine, never on our servers.
              Past transcripts stay there for as long as that machine&apos;s
              {` `}<strong>Settings → This device → Sessions</strong>{` `}
              (<strong>Keep session history</strong>) allows (Unlimited by
              default). <strong>Threads</strong> keeps the last day of threads
              in Recent, then the last twenty; an issue names every thread
              that worked it, one chip each under its header.
            </p>
            <p>
              A session whose host machine goes offline is paused rather than
              spinning: a card says the device is offline, and the agent picks
              up where it left off when the machine comes back, as it does
              when the desktop app or the daemon restarts. One that hits its
              agent&apos;s usage limit is marked{` `}
              <strong>Rate limited</strong> with the time it resets, instead
              of going quiet — the session stays live and steerable, it simply
              cannot make a call until then, unless you switch its account.
              You can see how much is left on any machine under its row on the{` `}
              <strong>Devices</strong> page.
            </p>
            <p>
              A session you started makes no report. When the agent finishes its
              turn it waits for your next reply, in the desktop app and on a
              daemon alike, with no idle timeout. <strong>Stop</strong>, in
              the thread&apos;s header (on a phone, in the top bar), interrupts
              the turn it is on: the messages you had queued come back into
              your composer, and the session waits for you. Quitting the
              desktop app or stopping the daemon only suspends its sessions:
              they pick up again in the same worktree when it is back.
              Writing to an ended thread revives it: the same session goes
              back to work on its branch, with every message you sent
              meanwhile.
            </p>
            <p>
              Sessions another session started (a thread&apos;s child sessions,
              a thread a <a href="/docs/actions/#triggers">team agent</a>{` `}
              started) are the ones that <strong>end themselves</strong>: the
              agent closes the session when its work is done and its report
              goes back to whoever started it.
            </p>

            <h3 id="computer-use">Computer use</h3>
            <p>
              On the desktop app (Linux on X11, macOS, Windows) and the CLI
              daemon, a session can drive the machine&apos;s screen: turn on{` `}
              <strong>Computer use</strong> in the machine&apos;s{` `}
              <strong>Device settings</strong> (with the{` `}
              <strong>Computer use model</strong> its screen-driving subagents
              run on), or per session in the composer&apos;s{` `}
              <strong>+</strong> menu. On macOS the readiness block asks for{` `}
              <strong>Screen Recording</strong> and{` `}
              <strong>Accessibility</strong>. It works in the foreground only:
              while the agent drives, your keyboard and pointer are held, the
              desktop app shows <strong>Computer use in progress</strong>, and
              pressing <kbd>Esc</kbd> hands both back and stops the
              session&apos;s turn.
            </p>
            <h3>Free requests and agents</h3>
            <p>
              Not every question is an issue. <strong>Agent</strong> in the
              sidebar on web and desktop, and the chat button beside the
              phone&apos;s tab bar, open the Agent page: the home of every
              client, a greeting over the composer and nothing else. With
              nothing chipped, the send starts a thread bound to no issue: it
              works in the repository you pick, or in a scratch directory when
              there is none. It runs on one of your machines like
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
              messages, and one card per thread it started for you. An agent
              answers a quick question itself and starts a thread for real
              work; the thread&apos;s report comes back into the conversation
              when it ends. A trigger firing arrives there as a message too,
              and the conversation itself never ends. Agents are set up under{` `}
              <a href="/docs/actions/#triggers">Settings → Agents</a>.
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
                <strong>Show complete diff</strong>, at the bottom of the
                Guide, opens the whole diff: a file tree with{` `}
                <strong>Filter files</strong> beside every changed file, its
                unchanged lines folded. Files no section names land in{` `}
                <strong>Other changes</strong>.
              </li>
              <li>
                While the PR is open the thread header carries the one{` `}
                <strong>Merge PR</strong> pill (on a phone, the Merge capsule
                at the bottom of the Guide); a PR stacked on another open one
                reads <strong>Merge stack</strong> and lands the whole stack,
                bottom up, and the Guide opens with the{` `}
                <strong>Stack</strong> it belongs to. The linked issues
                complete on merge. The thread header&apos;s{` `}
                <strong>…</strong> menu holds <strong>Close PR</strong>. On
                the Threads page a thread whose
                PR waits reads <strong>ready to merge</strong>, a stacked one{` `}
                <strong>stack</strong>, and its mark turns green while the PR
                is open and violet once it is merged.
              </li>
              <li>
                A merge refused for conflicts says so in a toast: write to the
                thread and the session rebases and pushes, then merge again.
              </li>
            </ul>
            <p>
              With <strong>Yolo mode</strong> on in{` `}
              <strong>Settings → General</strong>, nobody has to press Merge: a
              thread&apos;s pull requests merge by themselves once none of its
              sessions is working, the one at the bottom of a stack first.
            </p>
            <p>
              Prefer GitHub&apos;s review UI? The PR is a completely normal
              pull request. Review and merge it there and the issue completes
              just the same.
            </p>

            <DocShot
              view="guide"
              platform="desktop"
              caption="A thread's Guide in the desktop IDE: its stack, a section with its screenshots, Merge stack in the header"
            />

            <DocShot
              view="review-diff"
              caption="Show complete diff: the file tree with its filter beside every file's diff"
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
              and its diffs — without leaving the trunk view. Worktrees clean
              up after themselves: when a session ends its worktree goes
              (work it had not pushed is first pushed to an{` `}
              <code>exp/leftover-…</code> branch the conversation names), and{` `}
              <strong>Settings → This device → Worktrees</strong> lists and
              prunes the rest.
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

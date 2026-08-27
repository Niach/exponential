import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { DocShot } from "./components/DocShot"

const SECTIONS: DocsSectionType[] = [
  { id: `board`, num: `01`, label: `The board` },
  { id: `statuses`, num: `02`, label: `Statuses & priorities` },
  { id: `writing`, num: `03`, label: `Writing issues` },
  { id: `mentions`, num: `04`, label: `Mentions & refs` },
  { id: `detail`, num: `05`, label: `Issue detail` },
  { id: `notifications`, num: `06`, label: `Notifications` },
  { id: `branches-prs`, num: `07`, label: `Branches & PRs` },
]

const TASK_LIST_SNIPPET = `## Repro
1. Open the board on a **narrow** viewport
2. Drag an issue between columns

## Acceptance
- [ ] Drop indicator visible while dragging
- [x] Board scrolls when dragging near the edge

\`\`\`ts
// the culprit — offset ignores the scrolled container
const y = event.clientY - rect.top
\`\`\``

export function IssuesDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Issues &amp; boards</h1>
            <p>
              The core loop: file issues, triage them on the board, and track
              them from Backlog to a merged pull request.
            </p>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/issues/">
          {/* ── 01 The board ── */}
          <DocsSection id="board" num="01" label="The board">
            <h2>The board</h2>
            <p>
              A board is a list of issues grouped by status. Change status,
              priority, assignee, labels, and due date inline from the row.
              Click through for the full detail view.
            </p>

            <DocShot view="board" priority />

            <p>
              The <strong>Filter</strong> popover keeps the board focused:
              drill into any combination of <strong>status</strong>,{` `}
              <strong>priority</strong>, and <strong>labels</strong>. Active
              filters show as removable pills under the bar.
            </p>
            <p>
              Need to move many issues at once? <strong>Bulk select</strong>
              {` `}
              rows and change status or priority, add labels, or hand the
              whole selection to an agent as a{` `}
              <a href="/docs/coding/#batch-runs">batch coding run</a>.
            </p>
            <p>
              Deleting a board is a <strong>48-hour soft delete</strong>. An
              owner can restore it from the trash until the purge sweep runs.
            </p>
          </DocsSection>

          {/* ── 02 Statuses & priorities ── */}
          <DocsSection id="statuses" num="02" label="Statuses & priorities">
            <h2>Statuses &amp; priorities</h2>
            <p>
              Every team starts with seven built-in statuses{` `}
              (<strong>Backlog</strong>, <strong>Todo</strong>,{` `}
              <strong>In Progress</strong>, <strong>In Review</strong>,{` `}
              <strong>Done</strong>, <strong>Cancelled</strong>,{` `}
              <strong>Duplicate</strong>) and adds its own under{` `}
              <strong>Team settings → Statuses</strong>. Any member manages
              them; the seven builtins are locked (never renamed, recolored or
              deleted) but can be reordered.
            </p>
            <p>
              Every status sits in one of six <strong>categories</strong>{` `}
              (<code>backlog</code>, <code>unstarted</code>,{` `}
              <code>started</code>, <code>completed</code>,{` `}
              <code>cancelled</code>, <code>duplicate</code>), and the category
              is what the clients reason about: the board groups by it,{` `}
              <code>completed</code> stamps the completion timestamp, and{` `}
              <code>duplicate</code> points at the issue it duplicates. A custom
              status needs a name, a color and a category;{` `}
              <code>started</code> caps at four.
            </p>
            <p>
              Priorities are <strong>Urgent</strong>, <strong>High</strong>,
              {` `}
              <strong>Medium</strong>, <strong>Low</strong>, or none. An
              optional <strong>due date</strong> shows on the row with a
              calendar marker as it approaches.
            </p>
          </DocsSection>

          {/* ── 03 Writing issues ── */}
          <DocsSection id="writing" num="03" label="Writing issues">
            <h2>Writing issues</h2>
            <p>
              Descriptions and comments are{` `}
              <strong>GitHub-flavored markdown</strong>, and the same text
              renders identically on web, iOS, Android, and desktop, with no
              client-specific dialects. Supported and round-trippable:
            </p>
            <ul>
              <li>
                <strong>Inline</strong>: bold, italic, strikethrough, and{` `}
                <code>inline code</code>.
              </li>
              <li>
                <strong>Blocks</strong>: headings H1–H3, bullet and ordered
                lists, task lists (<code>- [ ]</code> / <code>- [x]</code>,
                checkable from any client), blockquotes, and fenced code
                blocks.
              </li>
              <li>
                <strong>Links and images</strong>: paste or drop an image
                straight into the editor; it uploads as an attachment and
                embeds in place, pre-sized so nothing jumps while loading.
              </li>
            </ul>
            <DocsCode language="markdown">{TASK_LIST_SNIPPET}</DocsCode>
            <DocsCallout kind="note" title="Deliberately not supported">
              Underline has no GFM representation, so it doesn&apos;t exist
              here. What you write must survive a round-trip through plain
              markdown on every client.
            </DocsCallout>
          </DocsSection>

          {/* ── 04 Mentions & refs ── */}
          <DocsSection id="mentions" num="04" label="Mentions & refs">
            <h2>Mentions &amp; refs</h2>
            <h3>@-mentions</h3>
            <p>
              Type <code>@</code> in any description or comment editor and an
              autocomplete offers your teammates. A mentioned member is{` `}
              <strong>notified and auto-subscribed</strong> to the issue, and
              their mention renders as a name pill on every client.
            </p>
            <h3>#-issue references</h3>
            <p>
              Type <code>#</code> and pick an issue, or just write{` `}
              <code>#EXP-42</code>. When the identifier resolves to an issue
              in the same team, every client renders it as a clickable pill
              that jumps straight to that issue. Unknown identifiers stay
              plain text, so pasting logs or commit messages never produces
              broken links.
            </p>
          </DocsSection>

          {/* ── 05 Issue detail ── */}
          <DocsSection id="detail" num="05" label="Issue detail">
            <h2>Issue detail</h2>
            <p>
              The full-page view puts the description front and center with a
              properties rail (status, priority, assignee, labels, due date)
              and the conversation below.
            </p>

            <DocShot view="issue-detail" />

            <p>
              The <strong>activity timeline</strong> interleaves comments with
              events: status changes, label changes, assignments, PR opened,
              PR merged. <strong>Subscribe</strong> to any issue to get its
              notifications. Commenting, being assigned, or being mentioned
              subscribes you automatically.
            </p>
          </DocsSection>

          {/* ── 06 Notifications ── */}
          <DocsSection id="notifications" num="06" label="Notifications">
            <h2>Notifications</h2>
            <p>
              The inbox collects everything addressed to you: assignments,
              comments on subscribed issues, @-mentions, PR opened / merged,
              and status changes.
            </p>

            <DocShot view="inbox" />

            <p>
              On iOS and Android the same events arrive as{` `}
              <strong>push notifications</strong> the moment they happen.
            </p>
            <h3>The daily email digest</h3>
            <p>
              Email is push-first, never a firehose: there are no per-event
              notification emails. Notifications still unread bundle into{` `}
              <strong>one digest a day</strong>, sent at a local hour you
              choose (08:00 by default). Read them in the app and no email ever
              comes.
            </p>
            <p>
              Tune it under <strong>Account → Notifications</strong>: per-type
              preferences, the send hour, and an <strong>hourly</strong>{` `}
              cadence if once a day is too slow. Every digest carries a
              one-click unsubscribe.
            </p>
          </DocsSection>

          {/* ── 07 Branches & PRs ── */}
          <DocsSection id="branches-prs" num="07" label="Branches & PRs">
            <h2>Branches &amp; PRs</h2>
            <p>
              An issue that gets coded maps to one branch{` `}
              (<code>exp/&lt;IDENTIFIER&gt;</code>, e.g.{` `}
              <code>exp/EXP-42</code>) and one linked pull request. The PR
              state (open, merged) is tracked on the issue automatically.
            </p>
            <h3>PR automation</h3>
            <p>
              What a PR event does to the issue is a per-team setting:{` `}
              <strong>Team settings → Statuses → PR automation</strong>. Out of
              the box, opening the PR moves the issue to{` `}
              <strong>In Review</strong> and merging it completes the issue to
              {` `}
              <strong>Done</strong>. Point either event at any of your
              team&apos;s statuses instead, or set it to{` `}
              <strong>Do nothing</strong> and move issues by hand.
            </p>
            <p>
              The one exception:{` `}
              <a href="/docs/coding/#batch-runs">batch coding runs</a>. A batch
              works several issues in one session on a shared{` `}
              <code>exp/batch-&lt;id&gt;</code> branch and opens{` `}
              <strong>one combined PR linked to every issue</strong> in the
              batch. Merging that single PR completes them all.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

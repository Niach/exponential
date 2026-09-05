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
              Need to move many issues at once? Click the checkbox gutter to{` `}
              <strong>bulk select</strong> rows, and the bar in the filter row
              sets <strong>status</strong>, <strong>priority</strong>,{` `}
              <strong>assignee</strong> and <strong>labels</strong>, deletes
              the selection, or hands the whole thing to an agent as a{` `}
              <a href="/docs/coding/#batch-runs">batch coding run</a>.
            </p>
            <h3>Archiving and deleting a board</h3>
            <p>
              Owners get two ways to put a board away, both under{` `}
              <strong>Team settings → Boards</strong>:
            </p>
            <ul>
              <li>
                <strong>Archive board</strong> hides it and all of its issues
                from the whole team — sidebar, search, pickers, every issue
                list — without deleting anything. Archived boards collect in
                an <strong>Archived boards</strong> card, and{` `}
                <strong>Unarchive</strong> brings one back exactly as it was.
                There is no time limit.
              </li>
              <li>
                <strong>Move to trash</strong> is a{` `}
                <strong>48-hour soft delete</strong>. The board sits in the{` `}
                <strong>Trash</strong> card with the time left on it, and{` `}
                <strong>Restore</strong> works until the purge sweep runs and
                deletes it (and its attachments) for good.
              </li>
            </ul>
            <p>
              Either way the board&apos;s prefix stays reserved, so nothing
              renumbers behind you.
            </p>
          </DocsSection>

          {/* ── 02 Statuses & priorities ── */}
          <DocsSection id="statuses" num="02" label="Statuses & priorities">
            <h2>Statuses &amp; priorities</h2>
            <p>
              Every team starts with six built-in statuses{` `}
              (<strong>Backlog</strong>,{` `}
              <strong>In Progress</strong>, <strong>In Review</strong>,{` `}
              <strong>Done</strong>, <strong>Cancelled</strong>,{` `}
              <strong>Duplicate</strong>) and adds its own under{` `}
              <strong>Team settings → Statuses</strong>. Any member manages
              them; the six builtins are locked (never renamed, recolored or
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
              <code>started</code> caps at four. No builtin lives in{` `}
              <code>unstarted</code> — it starts empty on every team and reads
              &ldquo;No statuses yet.&rdquo; until you add one.
            </p>

            <DocShot
              view="settings-statuses"
              caption="Team settings → Statuses: the six categories, the locked builtins, and a custom status"
            />
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
                checkable from any client), blockquotes, tables, and fenced
                code blocks.
              </li>
              <li>
                <strong>Links and images</strong>: paste or drop an image
                straight into the editor; it uploads as an attachment and
                embeds in place, pre-sized so nothing jumps while loading.
              </li>
            </ul>
            <DocsCode language="markdown">{TASK_LIST_SNIPPET}</DocsCode>
            <h3>Tables</h3>
            <p>
              Tables render as a real grid and are edited in place. On the web
              and in the desktop IDE, hovering a table reveals a{` `}
              <strong>+</strong> on each axis to append a row or column, and
              clicking a row or column head opens its menu:{` `}
              <strong>Insert column left / right</strong>,{` `}
              <strong>Move column left / right</strong>,{` `}
              <strong>Delete column</strong> (and the row equivalents), plus{` `}
              <strong>Delete table</strong>. On a phone you edit cells
              directly, and <strong>Delete table</strong> is on the keyboard
              bar. Tables live at the top level of a document: one nested in a
              list item or a quote is lifted out when the text round-trips.
            </p>
            <h3>Emoji</h3>
            <p>
              An emoji picker sits on the toolbar and the comment composer,
              and typing <code>:</code> opens the same catalog as a
              typeahead, with your recents first. Emoji are inserted as plain
              unicode, never as a <code>:shortcode:</code>, so they read the
              same everywhere the markdown ends up.
            </p>
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
              The full-page view puts the description front and center: a
              properties card (status, priority, assignee, labels, due date,
              board) sits under the title, then the <strong>Relations</strong>
              {` `}card, then the conversation. On web and desktop the content
              area is a rounded card floating on a darker ground; phones run
              full-bleed.
            </p>

            <DocShot view="issue-detail" />

            <h3>Relations</h3>
            <p>
              <strong>Add relation</strong> on the card offers{` `}
              <strong>Parent of</strong>, <strong>Sub-issue of</strong>,{` `}
              <strong>Blocking</strong>, <strong>Blocked by</strong>,{` `}
              <strong>Duplicate of</strong> and <strong>Related to</strong>,
              then a picker for the other issue; on iOS and Android the same
              list lives in the properties sheet. Writing <code>#EXP-42</code>
              {` `}in a description or comment links the two issues as related
              on its own, and marking an issue as a duplicate shows up in the
              card too. Every link shows on both issues, and adding or removing
              one lands in both activity feeds.
            </p>
            <h3>Activity and comments</h3>
            <p>
              The <strong>activity timeline</strong> interleaves comments with
              events: issue created, status changes, label changes,
              assignments, priority changes, relations, PR opened, PR merged —
              each led by its own icon and ending in its time. You follow an
              issue automatically when you create it, comment on it, get
              assigned or get mentioned; there is no subscribe button.
            </p>
            <p>
              Comments thread <strong>one level deep</strong>: every comment
              card ends with a <strong>Leave a reply…</strong> row. On web and
              desktop the reply composer opens right there; on iOS and Android
              the docked composer switches to <strong>Replying to</strong>,
              with an <strong>×</strong> back to a plain comment. Replies sit
              under their comment with a smaller avatar and edit and delete
              like any comment. A comment an agent posted over MCP says{` `}
              <strong>via MCP</strong> in its header.
            </p>
            <p>
              Comments carry <strong>attachments</strong>, not just markdown:{` `}
              <strong>Add image</strong> and <strong>Attach files</strong> in
              the composer upload on send, images render as previews under the
              comment body and other files as chips, and editing a comment
              adds or removes them.
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
              <strong>push notifications</strong> the moment they happen. The
              desktop app can raise real <strong>OS notifications</strong> for
              them too: its own{` `}
              <strong>Settings → Notifications</strong> pane has a{` `}
              <strong>Desktop notifications</strong> switch, per machine, and
              the per-type switches below it apply to those as well. Nothing
              notifies you about your own actions, or about a PR your own
              agent opened.
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
              Tune it under <strong>Settings → Notifications</strong>: per-type
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
            <h3>The base branch</h3>
            <p>
              A board picks the branch its work starts from. The board create
              and settings forms have one <strong>Repository</strong> select
              (<strong>No repository</strong>, the connected repos, or{` `}
              <strong>Connect another repository…</strong>) and a{` `}
              <strong>Branch</strong> picker under it. The repository&apos;s
              own default is tagged, and picking it clears the pin. Without a
              board pin, the team&apos;s per-repository default-branch
              override applies, and without that, GitHub&apos;s default —
              resolved live, never assumed.
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
              The same card carries one more switch,{` `}
              <strong>&ldquo;When a pull request merges, end its coding
              sessions&rdquo;</strong>, on by default. Turn it off and a merge
              leaves live <a href="/docs/coding/">coding sessions</a> running.
              Either way, the session that merged its own pull request always
              keeps running.
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

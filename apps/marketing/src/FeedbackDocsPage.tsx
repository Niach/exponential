import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"

const SECTIONS: DocsSectionType[] = [
  { id: `collect`, num: `01`, label: `Collect feedback` },
  { id: `conversation`, num: `02`, label: `The reporter conversation` },
  { id: `triage`, num: `03`, label: `Triage` },
]

export function FeedbackDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Feedback &amp; reporters</h1>
            <p>
              Collect user reports with the embeddable widget, answer the
              person who sent each one right from the issue, and fix the bugs
              on the same boards your team already works.
            </p>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/feedback/">
          {/* ── 01 Collect feedback ── */}
          <DocsSection id="collect" num="01" label="Collect feedback">
            <h2>Collect feedback</h2>
            <p>
              The <a href="/docs/widget/">embeddable widget</a> is how the
              outside world reaches your team: paste its snippet on your site
              and visitors can report bugs and ideas without ever leaving the
              page. Your boards stay private. Members triage what comes in.
            </p>
            <ul>
              <li>
                Each widget picks a <strong>board</strong>: every submission
                lands there as an ordinary issue, titled from the first line
                of the message, with the annotated screenshot attached and
                the metadata (page URL, browser, custom data) in the
                description.
              </li>
              <li>
                The reporter is <strong>auto-subscribed</strong>. Resolve the
                issue and the person who reported it is notified.
              </li>
              <li>
                Connect the board to your app&apos;s repository and{` `}
                <a href="/docs/coding/">your agents can fix reported bugs</a>
                {` `}straight off the board.
              </li>
            </ul>
          </DocsSection>

          {/* ── 02 The reporter conversation ── */}
          <DocsSection
            id="conversation"
            num="02"
            label="The reporter conversation"
          >
            <h2>The reporter conversation</h2>
            <p>
              A submission <strong>is</strong> an issue, and the conversation
              with the person who sent it happens in that issue&apos;s
              comments. There is no separate inbox to watch and nothing to
              escalate: the report, the fix and the reporter are one thing.
            </p>
            <p>How it runs when the reporter left an email:</p>
            <ul>
              <li>
                They get a confirmation email with a{` `}
                <strong>private link</strong> to their report: a page showing
                what they sent, their pictures, every reply you addressed to
                them, and a reply box. No account needed.
              </li>
              <li>
                On the issue, the comment composer grows a{` `}
                <strong>Reply to reporter</strong> toggle. It is off by
                default, so an ordinary comment stays with the team. Switch it
                on and that comment is emailed to the reporter and marked{` `}
                <em>to reporter</em> on the issue. A toast tells you whether
                the mail actually went out.
              </li>
              <li>
                When they answer through their link, the answer{` `}
                <strong>shows up as a comment</strong> on the issue, under
                their name (or &ldquo;Anonymous visitor&rdquo;), and lands as
                a <strong>reporter reply</strong> row in the inbox of everyone
                subscribed to the issue and its assignee.
              </li>
              <li>
                An answer on an issue that was already done{` `}
                <strong>reopens it</strong>, back to Backlog, so a &ldquo;this
                still happens&rdquo; never goes unseen.
              </li>
            </ul>
            <p>
              Team comments never reach the reporter unless you flip the
              toggle. Reporter comments can be removed by any member but
              edited by nobody, and whatever a reporter writes is shown as
              plain text, never rendered as markdown. Agents and scripts do
              the same over MCP: <code>exponential_comments_create</code> with{` `}
              <code>audience: &quot;reporter&quot;</code>.
            </p>
            <DocsCallout kind="note" title="Email is the only way back">
              The private link is all the reporter has, so replies to them
              need a mail transport. On a self-hosted instance without SMTP
              or SES the reports still land as issues, but the toast will tell
              you the reply was saved and not sent: see{` `}
              <a href="/docs/self-host/#email">Self-host → Email</a>.
            </DocsCallout>
          </DocsSection>

          {/* ── 03 Triage ── */}
          <DocsSection id="triage" num="03" label="Triage">
            <h2>Triage</h2>
            <p>
              Feedback lands as ordinary issues on a board, so triage is the
              workflow your team already knows: set priority, label it, move
              noise to <strong>Cancelled</strong>, mark repeats as{` `}
              <strong>Duplicate</strong>, and pull real bugs into{` `}
              <strong>In Progress</strong>. Or bulk-select a batch of them and{` `}
              <a href="/docs/coding/#batch-runs">hand it to an agent</a>.
            </p>
            <p>
              When the fix lands, the <strong>reporter is notified</strong>.
              The loop closes with the person who reported it.
            </p>
            <DocsCallout kind="tip" title="Dogfood">
              Exponential&apos;s own feedback runs exactly this setup: the
              feedback button in the corner of this site is the real widget,
              reporters get their answers from the issue, and agents fix the
              bugs.
            </DocsCallout>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { DocShot } from "./components/DocShot"

const SECTIONS: DocsSectionType[] = [
  { id: `collect`, num: `01`, label: `Collect feedback` },
  { id: `helpdesk`, num: `02`, label: `Helpdesk & support inbox` },
  { id: `escalate`, num: `03`, label: `Escalate to an issue` },
  { id: `triage`, num: `04`, label: `Triage` },
]

export function FeedbackDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Feedback &amp; helpdesk</h1>
            <p>
              Collect user reports with the embeddable widget, answer
              reporters by email from a shared support inbox, and fix the bugs
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
                Each widget picks a <strong>target board</strong>: feedback
                reports land there as ordinary issues, with the annotated
                screenshot attached and the metadata (page URL, browser,
                custom data) in the description.
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

          {/* ── 02 Helpdesk & support inbox ── */}
          <DocsSection id="helpdesk" num="02" label="Helpdesk & support inbox">
            <h2>Helpdesk &amp; support inbox</h2>
            <p>
              One switch gives your whole team a helpdesk:{` `}
              <strong>Settings → Feedback widget → Helpdesk</strong> (Team
              plan). With it on, support requests from the widget open
              {` `}
              <strong>tickets in your team&apos;s shared Support inbox</strong>:
              {` `}standalone email conversations with the reporter, not
              issues. Every member sees the inbox and can answer.
            </p>

            <DocShot view="support-inbox" />

            <p>How a thread runs:</p>
            <ul>
              <li>
                The reporter gets a confirmation email with a{` `}
                <strong>magic link</strong> to their own{` `}
                <code>/support/…</code> page (a live view of the
                conversation, no account needed). They can reply from that
                page or just answer the email.
              </li>
              <li>
                Your side happens in the <strong>Support inbox</strong>:
                replies you write there are emailed to the reporter from your
                team.
              </li>
              <li>
                <strong>Internal notes</strong> stay internal, visible to your
                team on the thread and never sent to the reporter.
              </li>
              <li>
                <strong>Closing</strong> a thread stops further replies but
                keeps the transcript readable at the reporter&apos;s link.
              </li>
            </ul>
          </DocsSection>

          {/* ── 03 Escalate to an issue ── */}
          <DocsSection id="escalate" num="03" label="Escalate to an issue">
            <h2>Escalate to an issue</h2>
            <p>
              When a ticket turns out to be a real bug, it becomes work with
              {` `}
              <strong>one click</strong>: escalate it into an issue on any
              board. The ticket keeps the conversation with the reporter.
              The issue carries the work: status, priority, assignee, labels,
              and a <a href="/docs/coding/">coding session</a> if the board
              has a repository.
            </p>
            <p>
              The thread shows its linked issue, so whoever answers support
              always knows where the fix stands and can tell the reporter the
              moment it ships.
            </p>
          </DocsSection>

          {/* ── 04 Triage ── */}
          <DocsSection id="triage" num="04" label="Triage">
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
              the helpdesk answers the support requests, and agents fix the
              bugs.
            </DocsCallout>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

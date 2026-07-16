import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { WebDemo } from "./webui/WebDemo"

const SECTIONS: DocsSectionType[] = [
  { id: `projects`, num: `01`, label: `Feedback projects` },
  { id: `public-boards`, num: `02`, label: `Public boards` },
  { id: `helpdesk`, num: `03`, label: `Helpdesk` },
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
              Collect user reports on a public board, answer reporters by
              email, and fix the bugs on the same board your team already
              works.
            </p>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/feedback/">
          {/* ── 01 Feedback projects ── */}
          <DocsSection id="projects" num="01" label="Feedback projects">
            <h2>Feedback projects</h2>
            <p>
              Create a project with the <strong>Feedback</strong> template and
              you get a board built for user reports. It behaves like any
              other project — issues, statuses, labels, comments — plus the
              feedback-specific machinery:
            </p>
            <ul>
              <li>
                It can be made <strong>publicly readable</strong> (next
                section) — the only thing in Exponential that can ever be
                public.
              </li>
              <li>
                It&apos;s the landing place for the{` `}
                <a href="/docs/widget/">embeddable widget</a>: submissions
                from your site arrive as issues here, screenshots attached.
              </li>
              <li>
                It can run the <strong>helpdesk</strong> — email
                conversations with reporters, threaded onto the issue.
              </li>
              <li>
                A repository is optional — connect your app&apos;s repo and
                {` `}
                <a href="/docs/coding/">Claude can fix reported bugs</a>{` `}
                straight off the feedback board.
              </li>
            </ul>
          </DocsSection>

          {/* ── 02 Public boards ── */}
          <DocsSection id="public-boards" num="02" label="Public boards">
            <h2>Public boards</h2>
            <p>
              Open the project&apos;s <strong>Settings</strong> and make it
              public: the board becomes readable at your team URL by anyone,
              {` `}
              <strong>no sign-in required</strong>. Visitors see a read-only
              view — issues, statuses, and (if you allow it) the discussion —
              so users can check whether something is already reported and
              watch it progress to Done.
            </p>
            <p>Owner-only toggles control how much visitors see:</p>
            <ul>
              <li>
                <strong>Show comments</strong> (on by default) — the issue
                discussion is visible.
              </li>
              <li>
                <strong>Show activity</strong> (off by default) — status
                changes and other timeline events are visible.
              </li>
            </ul>
            <p>
              Privacy is preserved throughout: visitors see{` `}
              <strong>anonymized member identities</strong> (like{` `}
              <code>Member 4f2a</code>) instead of names and emails, branch
              and PR details stay hidden, and nothing else in the team — not
              even sibling projects — is exposed. Public boards carry a small
              &quot;Powered by Exponential&quot; footer and are{` `}
              <strong>free on every tier</strong>.
            </p>
          </DocsSection>

          {/* ── 03 Helpdesk ── */}
          <DocsSection id="helpdesk" num="03" label="Helpdesk">
            <h2>Helpdesk</h2>
            <p>
              Enable the helpdesk per feedback project (Pro) and every widget
              report opens an <strong>email conversation</strong> with the
              person who filed it — support without a separate support tool.
            </p>

            <div className="docs-embed">
              <WebDemo view="support" />
            </div>

            <p>How a thread runs:</p>
            <ul>
              <li>
                The reporter gets a confirmation email with a{` `}
                <strong>magic link</strong> to their own{` `}
                <code>/support/…</code> page — a live view of the
                conversation, no account needed. They can reply from that page
                or just answer the email.
              </li>
              <li>
                Your side happens in the <strong>Support inbox</strong>:
                replies you write there are emailed to the reporter from your
                team.
              </li>
              <li>
                <strong>Internal notes</strong> stay internal — visible to
                your team on the thread, never sent to the reporter.
              </li>
              <li>
                <strong>Closing</strong> a thread stops further replies but
                keeps the transcript readable at the reporter&apos;s link.
              </li>
            </ul>
          </DocsSection>

          {/* ── 04 Triage ── */}
          <DocsSection id="triage" num="04" label="Triage">
            <h2>Triage</h2>
            <p>
              Feedback lands as ordinary issues on the board, so triage is the
              workflow your team already knows: set priority, label it, move
              noise to <strong>Cancelled</strong>, mark repeats as{` `}
              <strong>Duplicate</strong>, and promote real bugs to{` `}
              <strong>Todo</strong> — or bulk-select a batch of them and{` `}
              <a href="/docs/coding/#batch-runs">hand it to Claude</a>.
            </p>
            <p>
              When the fix lands and the issue resolves, the{` `}
              <strong>reporter is notified</strong> — the loop closes with the
              person who cared enough to report it.
            </p>
            <DocsCallout kind="tip" title="Dogfood">
              Exponential&apos;s own feedback board runs exactly this setup —
              widget on the marketing site, public board, helpdesk, and
              Claude fixing reports. See it live at{` `}
              <a href="https://app.exponential.at">app.exponential.at</a>.
            </DocsCallout>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

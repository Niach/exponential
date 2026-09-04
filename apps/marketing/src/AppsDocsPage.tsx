import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { DocShot } from "./components/DocShot"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `desktop`, num: `01`, label: `Desktop` },
  { id: `mobile`, num: `02`, label: `Mobile` },
  { id: `push`, num: `03`, label: `Push notifications` },
  { id: `steer`, num: `04`, label: `Steer from anywhere` },
]

export function AppsDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Mobile &amp; desktop apps</h1>
            <p>
              Native apps on every platform: the desktop IDE that runs coding
              sessions, and iOS / Android companions that keep you in the
              loop.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href={LINKS.downloadPage}>
                Download <IcArrow size={12} />
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/apps/">
          {/* ── 01 Desktop ── */}
          <DocsSection id="desktop" num="01" label="Desktop">
            <h2>Desktop</h2>
            <p>
              The desktop app is a native Rust IDE, the client that runs{` `}
              <a href="/docs/coding/">coding sessions</a> and the git IDE
              around them. Grab it from the{` `}
              <a href={LINKS.downloadPage}>download page</a>:
            </p>
            <ul>
              <li>
                <strong>macOS</strong>: a notarized <code>.dmg</code>.
              </li>
              <li>
                <strong>Windows</strong>: a portable <code>.exe</code>, no
                installer.
              </li>
              <li>
                <strong>Linux</strong>: an <code>AppImage</code>.
              </li>
            </ul>
            <p>
              All three come from{` `}
              <a href={LINKS.github.releases}>GitHub Releases</a>, with
              checksums published alongside. On first launch, sign in to the
              cloud or point it at your{` `}
              <a href="/docs/self-host/">self-hosted</a> instance URL.
            </p>
            <h3>Self-updating</h3>
            <p>
              The app checks the latest release at launch and every four hours
              while running. An update shows a banner. Click it and the
              download streams, verifies against the published checksums, and
              swaps in place; hit <strong>Restart to update</strong> when
              it&apos;s ready. No package manager, no manual downloads.
            </p>
            <h3>System notifications</h3>
            <p>
              The desktop app raises real macOS, Windows and XDG
              notifications when something lands in your inbox, coalescing a
              burst into one toast and staying quiet while the window is
              already focused. It is per machine:{` `}
              <strong>Settings → Notifications → Desktop notifications</strong>
              , with the per-type switches below it applying to those too.
            </p>
          </DocsSection>

          {/* ── 02 Mobile ── */}
          <DocsSection id="mobile" num="02" label="Mobile">
            <h2>Mobile</h2>
            <p>
              Native <a href={LINKS.downloads.ios}>iOS</a> and{` `}
              <a href={LINKS.downloads.android}>Android</a> apps with
              everything synced in real time. Five tabs (six with the helpdesk
              on):
            </p>
            <ul>
              <li>
                <strong>Issues</strong>: your boards. Triage, edit,
                filter, comment.
              </li>
              <li>
                <strong>My Work</strong>: your inbox and the issues assigned
                to you, in one place.
              </li>
              <li>
                <strong>Support</strong>: the team&apos;s shared helpdesk
                inbox. Present only while the helpdesk is enabled.
              </li>
              <li>
                <strong>Devices</strong>: your machines and the coding
                sessions running on them, with a live activity feed and
                steering.
              </li>
              <li>
                <strong>Actions</strong>: the team&apos;s{` `}
                <a href="/docs/actions/">actions</a>, its automations, and the
                suggestions to start from.
              </li>
              <li>
                <strong>Reviews</strong>: every issue with an open PR, with a
                one-click merge.
              </li>
            </ul>
            <p>
              The round button beside the tab pill is{` `}
              <strong>New issue</strong> — or <strong>Start chat</strong>{` `}
              on the Devices and Actions surfaces, which opens the launcher on
              its <a href="/docs/coding/#start-coding">Chat</a> tab. Search
              sits in the board header next to <strong>Filter</strong>. Select
              issues and the bulk bar takes the tab bar&apos;s place, with
              status, priority, assignee, labels and delete on it. Full
              onboarding runs on mobile too: create boards, connect GitHub,
              and manage repos without ever opening a laptop.
            </p>

            <div className="docs-shot-row">
              <DocShot view="board" platform="ios" caption="iOS" />
              <DocShot view="board" platform="android" caption="Android" />
            </div>
          </DocsSection>

          {/* ── 03 Push notifications ── */}
          <DocsSection id="push" num="03" label="Push notifications">
            <h2>Push notifications</h2>
            <p>
              Everything that lands in your <a href="/docs/issues/#notifications">inbox</a>{` `}
              (assignments, comments, mentions, PR opened / merged) arrives as
              a push notification the moment it happens. Tapping one deep-links
              straight to the issue. Notification preferences from{` `}
              <strong>Settings → Notifications</strong> apply to push too.
            </p>
            <DocsCallout kind="note" title="Push on self-hosted">
              The cloud has push wired up out of the box. The App Store / Play
              Store apps push only for cloud accounts; self-hosted instances
              get web, desktop system notifications and email instead. Why, and the
              build-it-yourself escape hatch:{` `}
              <a href="/docs/self-host/#push">
                self-host docs: push notifications
              </a>
              .
            </DocsCallout>
          </DocsSection>

          {/* ── 04 Steer from anywhere ── */}
          <DocsSection id="steer" num="04" label="Steer from anywhere">
            <h2>Steer from anywhere</h2>
            <p>
              A coding session started on your desktop doesn&apos;t chain you
              to the desk. The <strong>Devices</strong> tab (and the matching
              view on web) shows every running session with a{` `}
              <strong>live activity feed</strong>. Watch the agent work in
              real time, and <strong>send steer messages</strong> mid-run:
              answer its question, veto an approach, add a constraint you
              forgot.
            </p>
            <p>
              You can also start one from here: the launcher&apos;s{` `}
              <strong>Issues</strong>, <strong>Actions</strong> and{` `}
              <strong>Chat</strong> tabs, its device and agent pickers, and
              the <code>/</code> slash commands in the composer are the same
              on the phone as on the desktop. The session&apos;s{` `}
              <strong>…</strong> menu holds <strong>Usage</strong> (how much
              of the agent&apos;s limits this run and this week have spent)
              and <strong>Kill session</strong>, and a PR that is open puts a
              {` `}
              <strong>Merge</strong> pill next to{` `}
              <strong>Latest changes</strong> — replaced by{` `}
              <strong>Fix conflicts</strong> if the merge hits conflicts.
            </p>
            <p>
              The full flow (starting runs, plan mode, batches, review) is
              in <a href="/docs/coding/">Coding agents</a>.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

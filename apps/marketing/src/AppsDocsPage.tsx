import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { MobileDemo } from "./mobile/MobileDemo"
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
              Native apps on every platform — the desktop IDE that runs coding
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
              The desktop app is a native Rust IDE — the client that runs{` `}
              <a href="/docs/coding/">coding sessions</a> and the git IDE
              around them. Grab it from the{` `}
              <a href={LINKS.downloadPage}>download page</a>:
            </p>
            <ul>
              <li>
                <strong>macOS</strong> — a notarized <code>.dmg</code>.
              </li>
              <li>
                <strong>Windows</strong> — a portable <code>.exe</code>, no
                installer.
              </li>
              <li>
                <strong>Linux</strong> — an <code>AppImage</code>.
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
              while running. An update shows a banner — click it and the
              download streams, verifies against the published checksums, and
              swaps in place; hit <strong>Restart to update</strong> when
              it&apos;s ready. No package manager, no manual downloads.
            </p>
          </DocsSection>

          {/* ── 02 Mobile ── */}
          <DocsSection id="mobile" num="02" label="Mobile">
            <h2>Mobile</h2>
            <p>
              Native <a href={LINKS.downloads.ios}>iOS</a> and{` `}
              <a href={LINKS.downloads.android}>Android</a> apps with
              everything synced in real time. Four tabs:
            </p>
            <ul>
              <li>
                <strong>Issues</strong> — your boards: triage, edit,
                filter, comment.
              </li>
              <li>
                <strong>My Work</strong> — your inbox and the issues assigned
                to you, in one place.
              </li>
              <li>
                <strong>Agents</strong> — running coding sessions with a live
                terminal feed and steering.
              </li>
              <li>
                <strong>Search</strong> — find any issue across your teams.
              </li>
            </ul>
            <p>
              The compose button floats over every tab — capture an issue the
              moment you think of it. Full onboarding runs on mobile too:
              create boards, connect GitHub, and manage repos without ever
              opening a laptop.
            </p>

            <div className="docs-embed docs-embed-phone">
              <MobileDemo />
            </div>
          </DocsSection>

          {/* ── 03 Push notifications ── */}
          <DocsSection id="push" num="03" label="Push notifications">
            <h2>Push notifications</h2>
            <p>
              Everything that lands in your <a href="/docs/issues/#notifications">inbox</a> —
              assignments, comments, mentions, PR opened / merged — arrives as
              a push notification the moment it happens. Tapping one deep-links
              straight to the issue. Notification preferences from{` `}
              <strong>Account → Notifications</strong> apply to push too.
            </p>
            <DocsCallout kind="note" title="Push on self-hosted">
              The cloud has push wired up out of the box. Self-hosted
              instances run a small push relay —{` `}
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
              to the desk. The <strong>Agents</strong> tab (and the matching
              view on web) shows every running session with a{` `}
              <strong>live terminal feed</strong> — watch Claude work in real
              time, and <strong>send steer messages</strong> mid-run: answer
              its question, veto an approach, add a constraint you forgot.
            </p>
            <p>
              The full flow — starting runs, plan mode, batches, review — is
              in <a href="/docs/coding/">Coding with Claude</a>.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

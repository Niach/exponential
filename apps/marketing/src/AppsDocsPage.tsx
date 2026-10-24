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
  { id: `team-apps`, num: `05`, label: `Team apps` },
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
              loop. Plus the apps your team builds inside Exponential.
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
                <strong>Linux</strong>: an <code>AppImage</code> for x86_64,
                glibc 2.39 or newer (Ubuntu 24.04 and later).
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
            <h3>The layout</h3>
            <p>
              The sidebar mirrors the web app: the team switcher,{` `}
              <strong>Search</strong> and <strong>New issue</strong> at the
              top, then <strong>Agent</strong>, Inbox,{` `}
              <strong>Threads</strong>, Devices and Drafts (while you have
              any), the team&apos;s <strong>Agents</strong>, its{` `}
              <strong>Apps</strong>, the <strong>Boards</strong>, and your{` `}
              <strong>Active</strong> threads. At the bottom sit the{` `}
              <strong>What&apos;s new</strong> card,{` `}
              <strong>Getting started</strong> and your account, whose menu
              holds What&apos;s new, About and Sign out. On its right, the
              {` `}<strong>Computer</strong> button opens this machine&apos;s
              tools (a new <strong>Terminal</strong>, its{` `}
              <strong>Files</strong> and <strong>Source Control</strong>),
              then the settings gear.
            </p>
            <p>
              The sidebar never leaves. Open an issue or a session from a list, or
              open Settings, and it folds into a narrow icon rail with that
              list beside it, so every entry stays one click away.
            </p>
            <p>
              The working area is a rounded card on a darker ground, the same
              cutout layout the web app uses. Work opens as tabs along its
              top: an issue, or a <strong>thread</strong> with its own strip
              (Conversation, the issue chips, the session chips, the Guide)
              under the title. Terminals fill the card instead, and open
              terminals get a bar of tabs under it — a bar that is not there
              at all while none is open.
            </p>

            <DocShot
              view="steering"
              platform="desktop"
              caption="A session in the desktop IDE: the thread's tab along the top, its strip, and the sidebar beside it"
            />
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
              everything synced in real time. Four tabs:
            </p>
            <ul>
              <li>
                <strong>Issues</strong>: your boards. Triage, edit,
                comment.
              </li>
              <li>
                <strong>Inbox</strong>: your notifications,{` `}
                <strong>My issues</strong> and your{` `}
                <strong>Drafts</strong>, as three tabs, with{` `}
                <strong>Mark all read</strong> on top.
              </li>
              <li>
                <strong>Devices</strong>: your machines, the team&apos;s
                shared ones, and the agent accounts they hold.
              </li>
              <li>
                <strong>Threads</strong>: the team&apos;s{` `}
                <a href="/docs/actions/#triggers">agents</a> on top, then
                every thread under Needs you, Running and Recent. The button
                in its header opens the team&apos;s{` `}
                <a href="#team-apps">Apps</a>.
              </li>
            </ul>
            <p>
              The round chat button beside the tab pill opens the{` `}
              <a href="/docs/coding/#start-coding">Agent page</a>: the one
              composer, where the app opens. <strong>New issue</strong> sits
              beside it. Search sits in the board header. Select
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
              (new feedback, assignments, comments, mentions, status changes,
              PR opened / merged, reporter replies, blocked sessions) arrives as
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
              to the desk. <strong>Threads</strong>, on the phone and on the
              web, lists every thread with a{` `}
              <strong>live activity feed</strong> one tap away. Watch the
              agent work in real time, and{` `}
              <strong>send steer messages</strong> mid-session: answer its
              question, veto an approach, add a constraint you forgot. What
              needs you (a <strong>Question</strong>, a{` `}
              <strong>Plan ready</strong>) floats as a card above the
              composer; tap it and it opens as a sheet from the bottom.
            </p>
            <p>
              You can also start one from here: the Agent composer&apos;s{` `}
              <strong>+</strong> menu, its device, account and model pickers,
              and the session composer — <code>/</code> for
              the prompts and commands, <code>@</code> for a teammate,{` `}
              <code>#</code> for an issue and <code>:</code> for an emoji —
              are the same on the phone as on the desktop. On the phone,{` `}
              <strong>Stop</strong> sits in a session&apos;s top bar, and the
              context ring beside the composer opens{` `}
              <strong>Usage</strong> (how much of the agent&apos;s limits this
              session and this week have spent). A thread is one screen
              paged by its strip: the <strong>Conversation</strong>, each
              linked issue, each session&apos;s work, and the{` `}
              <strong>Guide</strong>, swiped or tapped, with the composer
              floating over whichever page is up. The Guide carries the{` `}
              <strong>Merge</strong> capsule while the pull request is open
              (<strong>Merge stack</strong> for a stacked one), so a PR the
              thread opened merges from the phone too.
            </p>
            <p>
              The full flow (starting sessions, plan mode, several issues at
              once, review) is in <a href="/docs/coding/">Coding agents</a>.
            </p>
          </DocsSection>

          {/* ── 05 Team apps ── */}
          <DocsSection id="team-apps" num="05" label="Team apps">
            <h2>Team apps</h2>
            <p>
              Apps are small screens your team builds inside Exponential: a
              dashboard over a board, a working-hours tracker, a release
              checklist. You describe
              what you want, an agent session builds it, and everyone on the
              team can open it. An app is a declarative package (data, never
              code) that every client renders the same way.
            </p>
            <p>
              They live in the sidebar&apos;s <strong>Apps</strong> group,
              between Agents and Boards; its <strong>+</strong> opens{` `}
              <strong>New app</strong> (icon, colour, name and an optional
              description), and <strong>Create app</strong> opens the editor.
              The group lists only your team&apos;s apps. Any member can
              create an app; the creator or an owner can rename and delete it.
            </p>
            <DocShot
              view="app"
              caption="The app editor: the live preview, with its chat floating above the composer"
            />
            <h3>The editor</h3>
            <p>
              The editor shows the app&apos;s <strong>Preview</strong> and
              your chat with its editor session. Type into{` `}
              <strong>Ask for a change…</strong>: the session runs on your
              device (<strong>Start a device to edit this app</strong> while
              none is online) and publishes each change as a new version, and
              the preview repaints as it lands. The chat floats as a card over
              the preview; open it and <strong>Move to the side</strong>{` `}
              docks it into its own pane (<strong>Minimize</strong> floats it
              again). On a phone, the editor is three tabs:{` `}
              <strong>Chat</strong>, <strong>Preview</strong> and{` `}
              <strong>Versions</strong>.
            </p>
            <p>
              <strong>Versions</strong> lists every version (<code>v1</code>,
              {` `}<code>v2</code>, …) with its note, when and who; the newest
              is <strong>Current</strong>. Tap an older one to preview it,
              then <strong>Restore</strong> to publish it again as a new
              version.
            </p>
            <h3>What an app can read</h3>
            <p>
              Besides its own records, an app binds the team&apos;s data
              through <code>exp:</code> sources: the issues of a board (filtered
              by status, assignee, priority or a search), the boards, the
              members, the threads and agent chats, an issue&apos;s comments and
              its files, and the devices. Every client resolves them the same
              way, live, from the rows it already syncs, so an app never waits
              on a request.
            </p>
            <h3>What an app may do as you</h3>
            <p>
              An app can call any Exponential operation: set an issue&apos;s
              status, file an issue, post a comment, start a session. It lists
              what it uses, and the app&apos;s settings page shows those as{` `}
              <strong>Tools</strong> with a switch each. A switched-on tool runs
              as you without asking; anything else asks every time, worded for
              the operation and with the arguments shown.
            </p>
            <h3>Share an app as a file</h3>
            <p>
              Export an app from its menu, its editor or its settings page: you
              get one <code>.app.json</code> file with its name, icon, colour,
              current version and, if you switch Include records on, its
              records. To use one, open New app, choose Import… and paste the
              file or pick it. Anything wrong with it is listed under the field
              before you import. An imported app starts with no tools granted.
            </p>
            <h3>Records: an app&apos;s own data</h3>
            <p>
              An app can keep data of its own. Its package declares record
              kinds (a day of working hours, a todo, a note) with a small
              schema; the app reads them live through{` `}
              <code>exp:records</code> and writes them without a consent
              card, since they are its own rows. Every member of the team sees
              the same records on every device. Free teams keep up to 1,000
              records, Team plans 25,000; the app&apos;s settings page shows the
              count. Agents fill them too: the{` `}
              <a href="/docs/mcp/#tools"><code>exponential_apps</code></a>{` `}
              tool&apos;s <code>records_list</code>, <code>records_upsert</code>{` `}
              and <code>records_delete</code> ops, so a session can log your hours
              for you or seed sample data while it builds the app.
            </p>
            <p>
              <strong>Settings → Apps</strong> lists them too; each app&apos;s
              settings page holds its name, icon, colour and description,{` `}
              <strong>Open in the editor</strong>, the record count, and delete. On the phones,
              {` `}Apps opens from the Threads tab&apos;s header. Agents and MCP
              clients can build apps too, through the{` `}
              <a href="/docs/mcp/#tools"><code>exponential_apps</code></a>{` `}
              tool.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

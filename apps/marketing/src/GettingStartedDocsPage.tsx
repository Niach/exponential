import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcChev } from "./components/icons"
import { DocShot } from "./components/DocShot"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `sign-up`, num: `01`, label: `Sign up & your team` },
  { id: `first-board`, num: `02`, label: `Your first board` },
  { id: `connect-github`, num: `03`, label: `Connect GitHub` },
  { id: `invite`, num: `04`, label: `Invite your team` },
  { id: `plans`, num: `05`, label: `Plans` },
  { id: `next`, num: `06`, label: `Where to next` },
]

const NEXT_CARDS: { path: string; label: string; desc: string }[] = [
  {
    path: `/docs/coding/`,
    label: `Coding agents`,
    desc: `Hand your first issue to a coding agent from the desktop IDE.`,
  },
  {
    path: `/docs/feedback/`,
    label: `Feedback & reporters`,
    desc: `Collect reports with the widget and answer reporters from the issue.`,
  },
  {
    path: `/docs/widget/`,
    label: `Feedback widget`,
    desc: `Put the feedback button on your own site.`,
  },
  {
    path: `/docs/mcp/`,
    label: `MCP & API`,
    desc: `Drive your issues from Claude, ChatGPT, Codex, or Cursor.`,
  },
]

export function GettingStartedDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Getting started</h1>
            <p>
              From sign-up to your first board, connected repo, and invited
              teammates in about five minutes.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href={LINKS.app.login}>
                Sign up free <IcArrow size={12} />
              </a>
              <a className="btn btn-ghost" href="/docs/self-host/">
                Or self-host
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/getting-started/">
          {/* ── 01 Sign up & your team ── */}
          <DocsSection id="sign-up" num="01" label="Sign up & your team">
            <h2>Sign up &amp; your team</h2>
            <p>
              Sign in at <a href={LINKS.app.login}>app.exponential.at</a>{` `}
              with <strong>Continue with Apple</strong>,{` `}
              <strong>Continue with Google</strong>,{` `}
              <strong>Continue with email</strong> (a 6-digit code is mailed
              to you) or <strong>Login with passkey</strong>. It is one screen
              for signing in and signing up: your account is created on first
              sign-in, with no separate registration step. On first launch,
              {` `}<strong>Welcome to Exponential</strong> offers{` `}
              <strong>Create a team</strong> (pick a name, you become its
              owner) or <strong>Join a team</strong> (paste an invite link).
            </p>
            <p>
              Your account has one email address. Under{` `}
              <strong>Settings → Account → Sign-in methods</strong> you can
              {` `}<strong>Change</strong> it, <strong>Link</strong> Apple or
              Google so either one signs you in to the same account, and{` `}
              <strong>Add passkey</strong> under <strong>Passkeys</strong>.
            </p>
            <DocShot view="sign-in" caption="One screen for signing in and signing up" />
            <p>
              Everything in Exponential lives inside a team: boards, issues,
              labels, members, connected repositories. Teams are private and
              invite-only. The outside world reaches you only through the{` `}
              <a href="/docs/widget/">embeddable feedback widget</a>.
            </p>
            <DocsCallout kind="note" title="Self-hosting?">
              A self-hosted instance chooses its own sign-in methods:
              email/password or a mailed code, passkeys, Google, Apple,
              GitHub, or any OIDC provider (Authentik, Keycloak, Zitadel, …).
              See the{` `}
              <a href="/docs/self-host/">self-host docs</a>.
            </DocsCallout>
          </DocsSection>

          {/* ── 02 Your first board ── */}
          <DocsSection id="first-board" num="02" label="Your first board">
            <h2>Create your first board</h2>
            <p>
              Once your team exists, the wizard continues with{` `}
              <strong>Create your first board</strong>, then{` `}
              <strong>Invite your teammates</strong> and{` `}
              <strong>Set up your devices</strong> (each with{` `}
              <strong>Skip for now</strong>). Two choices on the board step
              matter:
            </p>
            <h3>Name and prefix</h3>
            <p>
              Pick an icon, a colour and a name; the{` `}
              <strong>Prefix</strong> is filled in from the name. It is a
              short uppercase code —{` `}
              <strong>one to four</strong> letters or digits, starting with a
              letter, unique in the team — that numbers every issue on the
              board. Prefix <code>EXP</code> gives you <code>EXP-1</code>,{` `}
              <code>EXP-2</code>, and so on.
              Those identifiers follow the issue everywhere: branch names (
              <code>exp/EXP-42</code> when a session works on that one issue),
              PR titles, and <code>#EXP-42</code> references in comments.
            </p>
            <h3>Repository (optional)</h3>
            <p>
              A board is just a board: issues, statuses, labels, comments. It
              works as well for ops, content, or user feedback as it does for
              code. Hit <strong>Connect a GitHub repository</strong> (during
              creation, or later in the board&apos;s settings) and the board
              gains the coding loop: coding sessions, branches, and pull
              requests. In the board&apos;s settings, a{` `}
              <strong>Branch</strong> picker under the repository pins the
              branch this board&apos;s work starts from; leave it on the
              repository&apos;s own default and it follows that.
            </p>
            <DocsCallout kind="tip" title="Coding gates on the repo">
              Any board with a connected repository can run coding sessions. A
              feedback board backed by your app&apos;s repo lets an agent fix
              user-reported bugs directly.
            </DocsCallout>
          </DocsSection>

          {/* ── 03 Connect GitHub ── */}
          <DocsSection id="connect-github" num="03" label="Connect GitHub">
            <h2>Connect GitHub</h2>
            <p>
              Repositories connect through the{` `}
              <strong>Exponential GitHub App</strong>, not personal access
              tokens: the server mints short-lived, per-repo installation
              tokens whenever one is needed. There is one guided page,{` `}
              <strong>Connect GitHub</strong>, reached from{` `}
              <strong>Connect GitHub</strong> in{` `}
              <strong>Settings → Repositories</strong>, from the{` `}
              <strong>Getting started</strong> checklist, or when you create a
              board. It has three steps:
            </p>
            <ol>
              <li>
                <strong>Connect your GitHub account</strong>. Any login works;
                this only links GitHub to your Exponential account.
              </li>
              <li>
                <strong>Install the Exponential app</strong> on the GitHub
                account or organization that owns the repositories, and grant
                the ones you want. GitHub sends you straight back.
              </li>
              <li>
                <strong>Pick a repository</strong>: add the repositories the
                team codes on. Each board points at one.
              </li>
            </ol>
            <p>
              Settings → Repositories then shows{` `}
              <strong>Connected as</strong> your login with{` `}
              <strong>Disconnect</strong>, the GitHub accounts with the app
              installed (each with <strong>Configure</strong>, to grant more
              repositories on GitHub), <strong>Install on another account</strong>,
              and <strong>Add repository</strong>.
            </p>
            <p>
              The connection is <strong>per person</strong>; the repositories
              are the team&apos;s. Every member connects their own GitHub
              account and adds the repositories they can push to, and each row
              says <strong>Shared by</strong> whom, so a teammate&apos;s repos
              appear without you needing their access. Disconnecting only
              unlinks your own account: repositories already added keep
              working. Whoever shared a repository (and any owner) can remove
              it or pin its <strong>default branch</strong>, independent of
              GitHub&apos;s.
            </p>
            <p>
              A board backed by a repository gets the full coding loop:
              {` `}
              <strong>Start</strong> on any issue, automatic{` `}
              <code>exp/&lt;IDENTIFIER&gt;</code> branches, server-opened pull
              requests linked back to the issue, and automatic completion when
              the PR merges. Details in{` `}
              <a href="/docs/coding/">Coding agents</a>.
            </p>
          </DocsSection>

          {/* ── 04 Invite your team ── */}
          <DocsSection id="invite" num="04" label="Invite your team">
            <h2>Invite your team</h2>
            <p>
              Owners open <strong>Settings → Members</strong>. Under{` `}
              <strong>Invite members</strong>, type a name and an email and
              hit <strong>Send invite</strong>: the person shows up on the
              roster at once (assignable before they accept) and gets the link
              by mail; if the mail can&apos;t go out, the link is shown to
              copy instead. <strong>Add without inviting</strong> adds them
              without sending anything (the row reads{` `}
              <strong>Not invited</strong> until you send one), and{` `}
              <strong>Generate invite link</strong> makes a link anyone can
              follow to join. Unaccepted invites wait under{` `}
              <strong>Pending invites</strong> with their expiry date, and you
              can revoke each one there.
            </p>
            <p>
              Roles are <strong>owner</strong> and <strong>member</strong>.
              Owners manage the team: general settings, members and invites,
              storage, widgets, billing, the Linear import, the estimate
              scale, boards (including archiving and deleting them),{` `}
              <a href="/docs/actions/">prompts</a> and{` `}
              <a href="/docs/actions/#triggers">agents</a> with their
              triggers. Every member can edit issues, triage feedback, manage
              comments, connect repositories, run prompts, message agents,
              and edit statuses and labels.
              There is no read-only seat.
            </p>
          </DocsSection>

          {/* ── 05 Plans ── */}
          <DocsSection id="plans" num="05" label="Plans">
            <h2>Plans</h2>
            <p>
              The cloud is per-seat: <strong>Free</strong> for teams of up to
              three, and one paid <strong>Team</strong> plan at €12/seat/month
              billed yearly (€15 billed monthly) with everything included.
              Full details are on the <a href="/pricing/">pricing page</a>.
            </p>
            <ul>
              <li>
                <strong>Coding needs no plan.</strong> Your agent runs on your
                machine, on your own agent subscription. Boards, repos, and
                coding sessions are unlimited on every tier, including Free.
              </li>
              <li>
                Every tier includes the{` `}
                <a href="/docs/widget/">feedback widget</a>. Free comes with
                one, Team with unlimited widgets.
              </li>
              <li>
                <strong>Self-hosting is free for everyone.</strong> It&apos;s
                open source under Apache-2.0. Self-hosted is the default mode:
                every plan limit is off, billing included. Optional{` `}
                <a href="/docs/self-host/#licensing">Enterprise Support</a>
                {` `}
                is available if you want an SLA.
              </li>
            </ul>
          </DocsSection>

          {/* ── 06 Where to next ── */}
          <DocsSection id="next" num="06" label="Where to next">
            <h2>Where to next</h2>
            <p>
              The app keeps its own version of this page:{` `}
              <strong>Getting started</strong>. It shows under an empty board,
              on the Agent page while the team has no board yet, and behind
              the <strong>Getting started</strong> entry at the bottom of the
              sidebar (on the phones, a row in Settings). It walks the same
              ground — <strong>Create a board</strong>,{` `}
              <strong>Connect GitHub</strong>, <strong>Add a device</strong>,
              {` `}<strong>Start a session</strong>,{` `}
              <strong>Invite your team</strong>,{` `}
              <strong>Set up a server</strong>, and on web and desktop{` `}
              <strong>Connect your tools</strong> (MCP) and{` `}
              <strong>Feedback widget</strong> — with the next open step
              expanded and its button one click away. Done steps get a check,
              the header counts how many are done, and the whole checklist
              goes away when it is complete. There is nothing to dismiss.
            </p>
            <DocShot
              view="getting-started"
              caption="Getting started: done steps checked, the next one expanded"
            />
            <div className="docs-cards">
              {NEXT_CARDS.map((card) => (
                <a key={card.path} className="docs-card" href={card.path}>
                  <span className="docs-card-title">
                    {card.label} <IcChev size={13} />
                  </span>
                  <span className="docs-card-desc">{card.desc}</span>
                </a>
              ))}
            </div>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

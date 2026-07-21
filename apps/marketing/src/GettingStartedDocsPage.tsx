import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcChev } from "./components/icons"
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
    label: `Coding with Claude`,
    desc: `Hand your first issue to Claude from the desktop IDE.`,
  },
  {
    path: `/docs/feedback/`,
    label: `Feedback & helpdesk`,
    desc: `Collect reports with the widget and answer reporters by email.`,
  },
  {
    path: `/docs/widget/`,
    label: `Feedback widget`,
    desc: `Put the feedback button on your own site.`,
  },
  {
    path: `/docs/mcp/`,
    label: `MCP & API`,
    desc: `Drive your issues from Claude, ChatGPT, or Cursor.`,
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
              teammates — about five minutes.
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
              with Google — your account is created on first sign-in, no
              separate registration step. On first launch you either create
              a team (pick a name, you become its owner) or join an existing
              one by pasting an invite link.
            </p>
            <p>
              Everything in Exponential lives inside a team: boards, issues,
              labels, members, connected repositories. Teams are private and
              invite-only — the outside world reaches you only through the{` `}
              <a href="/docs/widget/">embeddable feedback widget</a>.
            </p>
            <DocsCallout kind="note" title="Self-hosting?">
              A self-hosted instance chooses its own sign-in methods —
              email/password, Google, or any OIDC provider (Authentik,
              Keycloak, Zitadel, …). See the{` `}
              <a href="/docs/self-host/">self-host docs</a>.
            </DocsCallout>
          </DocsSection>

          {/* ── 02 Your first board ── */}
          <DocsSection id="first-board" num="02" label="Your first board">
            <h2>Create your first board</h2>
            <p>
              On your first visit a short wizard walks you through creating a
              board. Two choices matter:
            </p>
            <h3>Name and prefix</h3>
            <p>
              The <strong>prefix</strong> is a short uppercase code that
              numbers every issue on the board — prefix <code>EXP</code>{` `}
              gives you <code>EXP-1</code>, <code>EXP-2</code>, and so on.
              Those identifiers follow the issue everywhere: branch names (
              <code>exp/EXP-42</code>), PR titles, and{` `}
              <code>#EXP-42</code> references in comments.
            </p>
            <h3>Repository — optional</h3>
            <p>
              A board is just a board: issues, statuses, labels, comments —
              great for code, ops, content, or user feedback alike. Connect a
              {` `}
              <strong>GitHub repository</strong> (during creation or any time
              later) and the board gains the coding loop: coding sessions,
              branches, and pull requests.
            </p>
            <DocsCallout kind="tip" title="Coding gates on the repo">
              Any board with a connected repository can run coding sessions —
              a feedback board backed by your app&apos;s repo lets Claude fix
              user-reported bugs directly.
            </DocsCallout>
          </DocsSection>

          {/* ── 03 Connect GitHub ── */}
          <DocsSection id="connect-github" num="03" label="Connect GitHub">
            <h2>Connect GitHub</h2>
            <p>
              Repositories connect through a <strong>GitHub App</strong>, not
              personal access tokens: the server mints short-lived, per-repo
              installation tokens whenever one is needed, and nothing
              long-lived is ever stored. Each installation is claimed by a
              specific team, so access never bleeds across teams.
            </p>
            <ol>
              <li>
                Open <strong>Team settings → Repositories</strong> (or hit
                {` `}
                <strong>Connect GitHub</strong> inside the board-creation
                flow — same thing).
              </li>
              <li>
                Approve the GitHub App for your account or organization and
                choose which repositories it may access. GitHub sends you
                straight back when you&apos;re done.
              </li>
              <li>
                Pick repositories from the picker to register them with your
                team. The picker only offers repos from installations your
                team has claimed — to add more, grant the App access on
                GitHub&apos;s installation settings page.
              </li>
            </ol>
            <p>
              A board backed by a repository gets the full coding loop:
              {` `}
              <strong>Start coding</strong> on any issue, automatic{` `}
              <code>exp/&lt;IDENTIFIER&gt;</code> branches, server-opened pull
              requests linked back to the issue, and automatic completion when
              the PR merges. Details in{` `}
              <a href="/docs/coding/">Coding with Claude</a>.
            </p>
          </DocsSection>

          {/* ── 04 Invite your team ── */}
          <DocsSection id="invite" num="04" label="Invite your team">
            <h2>Invite your team</h2>
            <p>
              Open <strong>Team settings → Members</strong> and create an
              invite link — anyone who follows it joins your team. Invites are
              single-purpose tokens you can revoke any time before they&apos;re
              accepted.
            </p>
            <p>
              Roles are deliberately simple: <strong>owner</strong> and{` `}
              <strong>member</strong>. Owners manage the team itself —
              settings, members, repositories, widgets, billing, and board
              deletion. Everything else is for everyone:{` `}
              <strong>every member moderates</strong>, meaning any member can
              edit issues, triage feedback, and manage comments. There is no
              read-only seat.
            </p>
          </DocsSection>

          {/* ── 05 Plans ── */}
          <DocsSection id="plans" num="05" label="Plans">
            <h2>Plans</h2>
            <p>
              The cloud is per-seat: <strong>Free</strong> for individuals
              (one seat), <strong>Pro</strong> at $5/seat/month (billed
              yearly), and <strong>Business</strong> at $10/seat/month —
              full details on the <a href="/pricing/">pricing page</a>.
            </p>
            <ul>
              <li>
                <strong>Coding needs no plan.</strong> Claude runs on your
                machine, on your own Claude subscription — unlimited boards,
                repos, and coding sessions on every tier, including Free.
              </li>
              <li>
                Every tier includes the{` `}
                <a href="/docs/widget/">feedback widget</a> — Free has one
                widget, Pro has three plus the{` `}
                <a href="/docs/feedback/">helpdesk</a>, Business is unlimited.
              </li>
              <li>
                <strong>Self-hosting is free under 10 people</strong> — set
                {` `}
                <code>SELF_HOSTED=true</code> and every plan limit disappears,
                billing included. Companies with 10 or more people need a{` `}
                <a href="/docs/self-host/#licensing">commercial license</a>.
              </li>
            </ul>
          </DocsSection>

          {/* ── 06 Where to next ── */}
          <DocsSection id="next" num="06" label="Where to next">
            <h2>Where to next</h2>
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

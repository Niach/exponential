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
  { id: `what`, num: `01`, label: `What an action is` },
  { id: `authoring`, num: `02`, label: `Authoring` },
  { id: `inputs`, num: `03`, label: `Inputs` },
  { id: `running`, num: `04`, label: `Running one` },
  { id: `triggers`, num: `05`, label: `Triggers` },
  { id: `builtins`, num: `06`, label: `The builtins` },
]

export function ActionsDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Actions</h1>
            <p>
              Reusable team prompts your agents run on demand: deploys, code
              reviews, releases, runbooks. Saved once, run from the desktop,
              the web, or your phone.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href={LINKS.downloadPage}>
                Get the desktop app <IcArrow size={12} />
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/actions/">
          {/* ── 01 What an action is ── */}
          <DocsSection id="what" num="01" label="What an action is">
            <h2>What an action is</h2>
            <p>
              An action is a markdown prompt owned by the team, run as a full
              interactive agent session on a member&apos;s own machine. Where a
              {` `}
              <a href="/docs/coding/">coding session</a> starts from an issue,
              an action starts from a saved instruction. That gives the work
              that isn&apos;t issue-shaped a home too: ship a release, run a
              migration, restart a service, review a pull request.
            </p>
            <p>
              An action can name a <strong>repository</strong>, in which case
              the run gets its own git worktree on its own branch{` `}
              (<code>exp/&lt;slug&gt;-&lt;id&gt;</code>) — never the trunk
              checkout. Without a repository it runs in a scratch directory.
              Either way it runs on the member&apos;s own device, with their
              own agent subscription. Nothing executes on Exponential&apos;s
              servers, and no team secrets are involved.
            </p>
            <p>
              An action runs on demand, or by itself on a schedule or an
              event: see <a href="#triggers">Triggers</a>.
            </p>
          </DocsSection>

          {/* ── 02 Authoring ── */}
          <DocsSection id="authoring" num="02" label="Authoring">
            <h2>Authoring</h2>
            <p>
              There is no form to fill in. You author an action by running the
              built-in <strong>Create action</strong>. Describe what it should
              do and the agent writes it, registering it for the team through
              the MCP API. Editing and deleting are{` `}
              <strong>owner-only</strong>; running is open to every member.
            </p>
            <p>
              Opening an action in the <strong>Actions</strong> list opens its
              page. It has three parts:
            </p>
            <ul>
              <li>
                <strong>Prompt</strong>: the name, icon, description, composer
                hint, repository and the markdown prompt itself.
              </li>
              <li>
                <strong>Triggers</strong>: what starts the action without
                you, covered <a href="#triggers">below</a>.
              </li>
              <li>
                <strong>Runs</strong>: every run of the action, newest first.
              </li>
            </ul>
            <p>
              The desktop app and wide web show the three as sections of one
              page; phones show them as tabs.
            </p>

            <DocShot
              view="action-page"
              caption="An action's page: Prompt, Triggers, Runs"
            />
          </DocsSection>

          {/* ── 03 Inputs ── */}
          <DocsSection id="inputs" num="03" label="Inputs">
            <h2>Inputs</h2>
            <p>
              An action can declare up to <strong>10 typed pick inputs</strong>,
              each optional or required. Whoever runs it picks them, and the
              values are appended to the prompt:
            </p>
            <ul>
              <li>
                <code>repo</code>: one of the team&apos;s connected
                repositories.
              </li>
              <li>
                <code>board</code>: one of the team&apos;s boards.
              </li>
              <li>
                <code>pr</code>: an issue with an open pull request.
              </li>
              <li>
                <code>icon</code>: a glyph from the shared icon set.
              </li>
            </ul>
            <p>
              There is no free-text input type. Whatever the runner wants to
              add goes into the composer&apos;s{` `}
              <strong>Additional instructions</strong> box, whose hint text is
              the action&apos;s prompt placeholder; it lands in the run&apos;s
              prompt under a heading of that name.
            </p>
          </DocsSection>

          {/* ── 04 Running one ── */}
          <DocsSection id="running" num="04" label="Running one">
            <h2>Running one</h2>
            <p>
              On the desktop, actions live in their own rail entry, and the
              {` `}
              <a href="/docs/coding/#start-coding">Agent page composer</a>{` `}
              takes one as a chip (the ▶ button), with the same agent, model
              and effort pickers as an issue run.
            </p>
            <p>
              From the web (<strong>Actions</strong> in the sidebar) or the{` `}
              <a href="/docs/apps/">mobile apps</a>, <strong>Run</strong>{` `}
              hands the action to one of your online desktops and drops you
              into the live session, the same watch-and-steer view as a coding
              run. All four clients edit actions in full; the writes are still
              owner-only. A run that opens a pull request shows{` `}
              <strong>Merge</strong> in its session view, and the PR is listed
              under <strong>Agent runs</strong> in{` `}
              <a href="/docs/coding/#review-merge">Reviews</a>.
            </p>
            <p>
              Starting from scratch? A curated catalog of{` `}
              <strong>suggestions</strong>, seeds that prefill the creator
              run, some carrying a trigger, sits behind the lightbulb next to
              {` `}
              <strong>New action</strong> (on desktop web it is Getting
              started&apos;s <strong>Suggested actions</strong> tab; on the
              phone it is the <strong>Suggestions</strong> tab beside{` `}
              <strong>Actions</strong>).
            </p>
            <DocsCallout kind="note" title="A machine has to be online">
              Actions always execute on one of your own machines. Starting one
              from the web or your phone needs a desktop app, or the{` `}
              <a href="/docs/cli/#daemon">CLI daemon</a>, online. A start to an
              offline machine is refused right away rather than queued.
            </DocsCallout>
          </DocsSection>

          {/* ── 05 Triggers ── */}
          <DocsSection id="triggers" num="05" label="Triggers">
            <h2>Triggers</h2>
            <p>
              A trigger starts an action without anyone pressing{` `}
              <strong>Run</strong>. It is a <strong>schedule</strong> or an
              {` `}
              <strong>event</strong>, bound to one <strong>device</strong>,
              with its own on/off switch. Triggers live in the{` `}
              <strong>Triggers</strong> part of the action&apos;s page, and
              adding, editing and deleting them is{` `}
              <strong>owner-only</strong>.
            </p>
            <p>
              <strong>Add trigger</strong> opens the form:
            </p>
            <ul>
              <li>
                <strong>Schedule</strong>: daily, weekly on a weekday, or
                monthly on a day from 1 to 28, at a time in the device&apos;s
                local time.
              </li>
              <li>
                <strong>Event</strong>: an issue is created, its status,
                assignee or priority changes, a label is added, or a pull
                request is opened or merged. Board, label, priority and
                status filters narrow it.
              </li>
              <li>
                <strong>Device</strong>: the desktop app or{` `}
                <a href="/docs/cli/#daemon">CLI daemon</a> that starts the
                run.
              </li>
              <li>
                <strong>Account</strong>, <strong>model</strong> and{` `}
                <strong>effort</strong>: optional pins for the run.
              </li>
            </ul>

            <DocShot
              view="trigger-editor"
              caption="The trigger form: schedule or event, device, account, model, effort"
            />

            <DocsCallout kind="note" title="A triggered run fills in no inputs">
              A trigger can only be switched on while every input of its
              action is optional.
            </DocsCallout>
            <p>
              Triggers run locally. There is no server scheduler: the bound
              device starts the run itself. If it was offline at a scheduled
              time, it catches up with one run when it is back. Withdrawing a
              shared machine from a team pauses the triggers bound to it.
            </p>
            <p>
              In the Actions list, a row shows a clock while its action has a
              schedule trigger and a bolt while it has an event trigger; the
              glyph is muted while no trigger of that kind is switched on. A
              triggered run carries the same glyph under{` `}
              <strong>Runs</strong> and stays out of the Agent page&apos;s
              Recent list.
            </p>
          </DocsSection>

          {/* ── 06 The builtins ── */}
          <DocsSection id="builtins" num="06" label="The builtins">
            <h2>The builtins</h2>
            <p>
              Two builtins ship with the product and can be picked like any
              action; the composer&apos;s own <strong>Chat</strong> is the
              third way to run without one. None of them can be edited or
              deleted, and none carries triggers:
            </p>
            <ul>
              <li>
                <strong>Create action</strong>: the authoring flow above.
                Takes a description, plus an optional name, repository and
                icon. It is the <strong>New action</strong> button rather than
                a row in the list.
              </li>
              <li>
                <strong>Fix merge conflicts</strong>: takes a pull request,
                checks out its branch in its own worktree, rebases onto the
                base branch, resolves the conflicts, force-pushes, and merges.
                It doesn&apos;t clutter the actions list — you meet it as the
                {` `}
                <strong>Fix conflicts</strong> button that replaces{` `}
                <strong>Merge</strong> on the{` `}
                <a href="/docs/coding/#review-merge">Reviews</a> queue and in
                the session view when a merge fails on conflicts.
              </li>
            </ul>
            <p>
              <strong>Chat</strong> is not an action you pick: it is the{` `}
              <a href="/docs/coding/#watch-steer">Agent page</a> composer
              with nothing chipped, a free prompt with no issue attached and
              no repository required. With a repository picked the run gets
              its own <code>exp/chat-&lt;id&gt;</code> worktree, without one
              it runs in a scratch directory. Either way it steers like any
              other session.
            </p>
            <p>
              Actions and their triggers are scriptable too. See the{` `}
              <a href="/docs/mcp/#tools">
                <code>exponential_actions_*</code> tools
              </a>
              {` `}in the MCP reference.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

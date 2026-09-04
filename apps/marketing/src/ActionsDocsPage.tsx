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
  { id: `automations`, num: `05`, label: `Automations` },
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
              Actions run on demand, or on a schedule or an issue event —
              see <a href="#automations">Automations</a> below.
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
              Actions carry a name, a description, an optional icon and the
              markdown body that becomes the prompt.
            </p>
          </DocsSection>

          {/* ── 03 Inputs ── */}
          <DocsSection id="inputs" num="03" label="Inputs">
            <h2>Inputs</h2>
            <p>
              An action can declare up to <strong>10 typed inputs</strong>,
              each optional or required. Whoever runs it fills them in, and the
              values are appended to the prompt:
            </p>
            <ul>
              <li>
                <code>text</code>: free text, up to 4096 characters.
              </li>
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
              <li>
                <code>textarea</code>: multi-line free text, same 4096
                characters.
              </li>
            </ul>
          </DocsSection>

          {/* ── 04 Running one ── */}
          <DocsSection id="running" num="04" label="Running one">
            <h2>Running one</h2>
            <p>
              On the desktop, actions live in their own rail entry, and the
              {` `}
              <a href="/docs/coding/#start-coding">Start coding</a> dialog has
              an <strong>Actions</strong> tab, with the same agent, model and
              effort pickers as an issue run.
            </p>
            <p>
              From the web (<strong>Actions</strong> in the sidebar) or the{` `}
              <a href="/docs/apps/">mobile apps</a>, <strong>Run</strong>{` `}
              hands the action to one of your online desktops and drops you
              into the live session, the same watch-and-steer view as a coding
              run. All four clients edit actions in full; the writes are still
              owner-only.
            </p>
            <p>
              Desktop web and the IDE give <strong>Actions</strong> and{` `}
              <strong>Automations</strong> their own sidebar entries; the
              phone keeps them as tabs of the Actions tab. Starting from
              scratch? A curated catalog of <strong>suggestions</strong> —
              seeds that prefill the creator run, some carrying an automation
              with them — sits behind the lightbulb next to{` `}
              <strong>New action</strong> (on desktop web it is Getting
              started&apos;s <strong>Suggested actions</strong> tab; on the
              phone it is the third tab).
            </p>
            <DocsCallout kind="note" title="A machine has to be online">
              Actions always execute on one of your own machines. Starting one
              from the web or your phone needs a desktop app, or the{` `}
              <a href="/docs/cli/#daemon">CLI daemon</a>, online. A start to an
              offline machine is refused right away rather than queued.
            </DocsCallout>
          </DocsSection>

          {/* ── 05 Automations ── */}
          <DocsSection id="automations" num="05" label="Automations">
            <h2>Automations</h2>
            <p>
              An automation is the <em>when</em> around an action: it binds one
              action to one <strong>device</strong> and one{` `}
              <strong>trigger</strong>. Automations live on their own{` `}
              <strong>Automations</strong> surface — a sidebar entry on
              desktop web and in the IDE, a tab of the Actions page on the
              phone. Creating, editing, toggling and deleting them is{` `}
              <strong>owner-only</strong>; every member sees the list and the
              runs.
            </p>

            <DocShot
              view="automations-list"
              caption="The Automations tab: each row's trigger, machine, agent, next and last run, and its on/off switch"
            />

            <p>
              <strong>+ New automation</strong> opens the editor. It has four
              parts:
            </p>
            <ul>
              <li>
                <strong>Action</strong>: which of the team&apos;s actions to
                run.
              </li>
              <li>
                <strong>Schedule</strong> or <strong>On event</strong>. A
                schedule runs <strong>Every</strong> Day, Week or Month at a{` `}
                <strong>Time</strong> — always the bound machine&apos;s local
                clock, which is why the row reads &ldquo;(device time)&rdquo;
                — with a weekday or day-of-month picker for the longer
                intervals. An event fires{` `}
                <strong>When an issue is created</strong>, the{` `}
                <strong>status changes</strong>, the{` `}
                <strong>assignee changes</strong>, a{` `}
                <strong>label is added</strong>, the{` `}
                <strong>priority changes</strong>, or a{` `}
                <strong>pull request is opened</strong> or{` `}
                <strong>merged</strong>. Events can be narrowed by board, and
                by label, priority or target status where the event has one.
              </li>
              <li>
                <strong>Runs on</strong>: the machine that executes it. Any
                machine that can take a run is pickable, online or not — a
                schedule that came due while it was off catches up when it
                reconnects.
              </li>
              <li>
                <strong>Agent</strong>, <strong>Model</strong> and{` `}
                <strong>Effort</strong>, exactly as in the start-coding
                dialog. Leave them on <em>CLI default</em> and the run takes
                the device&apos;s own launch defaults.
              </li>
            </ul>

            <DocShot
              view="automations"
              caption="The automation editor: action, trigger, machine, agent"
            />

            <p>
              Each row carries its trigger sentence, its machine, its{` `}
              <strong>next</strong> run and how the <strong>last</strong> one
              ended, plus a switch to disable it without deleting it and a{` `}
              <strong>…</strong> menu to edit or delete. Below the list,{` `}
              <strong>Recent automated runs</strong> is the record of what
              actually fired; each finished run keeps the summary its agent
              wrote.
            </p>
            <DocsCallout kind="note" title="An automation fills in nothing">
              Nobody is there to answer a prompt, so an enabled automation
              needs <em>every input of its action to be optional</em>.
              An action with a required input can still be picked, but the
              switch stays off until you make those inputs optional.
            </DocsCallout>
            <p>
              There is no server-side scheduler: the bound machine reads its
              own enabled automations off the live sync and starts the run
              itself, so nothing fires while every one of your machines is
              off. Withdrawing a shared machine from a team disables the
              automations bound to it.
            </p>
          </DocsSection>

          {/* ── 06 The builtins ── */}
          <DocsSection id="builtins" num="06" label="The builtins">
            <h2>The builtins</h2>
            <p>
              Three actions ship with the product. They can&apos;t be edited
              or deleted, and they can&apos;t be automated:
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
              <li>
                <strong>Chat</strong>: a free prompt on a repository, with no
                issue attached. It is the launcher&apos;s{` `}
                <strong>Chat</strong> tab rather than a list row; the run gets
                its own <code>exp/chat-&lt;id&gt;</code> worktree and steers
                like any other session.
              </li>
            </ul>
            <p>
              Actions and automations are scriptable too. See the{` `}
              <a href="/docs/mcp/#tools">
                <code>exponential_actions_*</code> and{` `}
                <code>exponential_automations_*</code> tools
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

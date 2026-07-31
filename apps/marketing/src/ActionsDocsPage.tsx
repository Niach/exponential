import {
  DocsCallout,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `what`, num: `01`, label: `What an action is` },
  { id: `authoring`, num: `02`, label: `Authoring` },
  { id: `inputs`, num: `03`, label: `Inputs` },
  { id: `running`, num: `04`, label: `Running one` },
  { id: `builtins`, num: `05`, label: `The two builtins` },
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
              it runs on that repo&apos;s trunk clone. Without one it runs in a
              scratch directory. Either way it runs on the member&apos;s own
              device, with their own agent subscription. Nothing executes on
              Exponential&apos;s servers, and no team secrets are involved.
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
              From the web (<strong>Agents</strong> in the sidebar) or the{` `}
              <a href="/docs/apps/">mobile apps</a>, <strong>Run</strong>{` `}
              hands the action to one of your online desktops and drops you
              into the live session, the same watch-and-steer view as a coding
              run. Mobile can view and run actions; editing is web and desktop
              only.
            </p>
            <DocsCallout kind="note" title="A desktop has to be online">
              Actions always execute on a desktop. Starting one from the web or
              your phone needs one of your machines running the desktop app.
            </DocsCallout>
          </DocsSection>

          {/* ── 05 The two builtins ── */}
          <DocsSection id="builtins" num="05" label="The two builtins">
            <h2>The two builtins</h2>
            <p>
              Two actions ship with the product. They are pinned first in every
              team&apos;s list and can&apos;t be edited or deleted:
            </p>
            <ul>
              <li>
                <strong>Create action</strong>: the authoring flow above.
                Takes a description, plus an optional repository and icon.
              </li>
              <li>
                <strong>Fix merge conflicts</strong>: takes a pull request,
                checks out its branch in its own worktree, rebases onto the
                default branch, resolves the conflicts, force-pushes, and
                merges. It&apos;s the button you get on the{` `}
                <a href="/docs/coding/#review-merge">Reviews</a> list when a
                merge fails on conflicts.
              </li>
            </ul>
            <p>
              Actions are scriptable too. See the{` `}
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

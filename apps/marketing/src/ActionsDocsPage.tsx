import {
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { DocShot } from "./components/DocShot"
import { LINKS } from "./lib/links"

/* NEW-2: actions became prompts. The URL stays /docs/actions/ so links keep
   working; the page documents prompts. */
const SECTIONS: DocsSectionType[] = [
  { id: `what`, num: `01`, label: `What a prompt is` },
  { id: `settings`, num: `02`, label: `Settings → Prompts` },
  { id: `composer`, num: `03`, label: `The / menu` },
  { id: `triggers`, num: `04`, label: `Schedules and events` },
]

export function ActionsDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Prompts</h1>
            <p>
              Saved team prompts. Write one once, run it from the{` `}
              <code>/</code> menu.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href={LINKS.downloadPage}>
                Get the desktop app <IcArrow size={12} />
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/actions/">
          {/* ── 01 What a prompt is ── */}
          <DocsSection id="what" num="01" label="What a prompt is">
            <h2>What a prompt is</h2>
            <p>
              A prompt is a <strong>name</strong>, an <strong>icon</strong>{` `}
              and a markdown <strong>body</strong>, owned by the team. That is
              all it holds. The session you start with it picks its device,
              agent and repository like any other{` `}
              <a href="/docs/coding/">coding session</a>.
            </p>
          </DocsSection>

          {/* ── 02 Settings → Prompts ── */}
          <DocsSection id="settings" num="02" label="Settings → Prompts">
            <h2>Settings → Prompts</h2>
            <p>
              <strong>Settings → Prompts</strong> lists the team&apos;s
              prompts. <strong>New prompt</strong> opens the editor: icon and
              name in one row over the body, then{` `}
              <strong>Create prompt</strong>. Opening a row opens the same
              page, saved with <strong>Save changes</strong>; owners also get{` `}
              <strong>Delete prompt</strong> there.
            </p>
            <p>
              Owners create, edit and delete prompts. Every member can read
              and run them.
            </p>

            <DocShot
              view="settings-prompts"
              caption="Settings → Prompts"
            />

            <DocShot
              view="settings-prompt"
              caption="New prompt: the icon and name over the body, then Create prompt"
            />
          </DocsSection>

          {/* ── 03 The / menu ── */}
          <DocsSection id="composer" num="03" label="The / menu">
            <h2>The / menu</h2>
            <p>
              In the <strong>Agent</strong> page&apos;s composer, type{` `}
              <code>/</code> to
              list the team&apos;s prompts. Keep typing to filter, then press
              {` `}
              <strong>Enter</strong> or click one. The prompt becomes the
              session&apos;s subject, and the send starts the session with its
              body plus what you typed.
            </p>

            <DocShot
              view="composer-slash-menu"
              caption="The Agent page composer's / menu"
            />

            <p>
              In a live session, <code>/</code> lists <code>/compact</code>,
              {` `}
              <code>/model</code> and <code>/effort</code>, then the
              prompts. Picking a prompt puts its body in your draft.
            </p>

            <DocShot
              view="session-slash-menu"
              caption="A live session's / menu"
            />
          </DocsSection>

          {/* ── 04 Schedules and events ── */}
          <DocsSection id="triggers" num="04" label="Schedules and events">
            <h2>Schedules and events</h2>
            <p>
              Prompts have no triggers. To run work on a schedule or when an
              event happens, give an <strong>agent</strong> a trigger in{` `}
              <strong>Settings → Agents</strong>.
            </p>
            <p>
              An agent is a definition the owners set up there with{` `}
              <strong>New agent</strong> (or the <strong>+</strong> beside{` `}
              <strong>Agents</strong> in the sidebar): its icon, color and{` `}
              <strong>Name</strong>, the <strong>Device</strong> it runs on,
              the <strong>Account</strong> it uses there (the device&apos;s
              last used login when left alone), its{` `}
              <strong>Triggers</strong> and its markdown{` `}
              <strong>Instructions</strong>. Members can read every agent.
              Each member talks to an agent in a conversation of their own,
              from its row in the sidebar or{` `}
              <strong>Message an agent</strong> in the composer&apos;s{` `}
              <strong>+</strong> menu; see{` `}
              <a href="/docs/coding/#watch-steer">Coding agents</a>.
            </p>
            <p>
              <strong>Add</strong> on the Triggers row opens{` `}
              <strong>New trigger</strong> with two tabs.{` `}
              <strong>Schedule</strong> picks <strong>Repeat</strong> (every
              hour, day, week or month), the <strong>Days</strong> or{` `}
              <strong>Day</strong>, the <strong>Time</strong> and the{` `}
              <strong>Time zone</strong>, and reads back as &ldquo;Every day
              at 09:00 (Europe/Vienna)&rdquo;. <strong>On event</strong>{` `}
              names an event: a built-in one (&ldquo;When an issue is
              created&rdquo;, &ldquo;When an issue&apos;s status
              changes&rdquo;, &ldquo;When a pull request opens&rdquo;,
              &ldquo;When a pull request merges&rdquo;, &ldquo;When a session
              ends&rdquo;) or any name of your own, which a session emits over
              MCP. Each trigger has its own switch; a paused one reads{` `}
              <strong>Paused</strong>. With no trigger the agent{` `}
              <strong>Waits for messages</strong>. A firing arrives as a
              message in the conversation of the agent&apos;s owner; the
              server fires it, so nothing is armed on a device.
            </p>

            <DocShot
              view="settings-team-agent"
              caption="An agent: icon, color and name, Device, Account, two triggers and its instructions"
            />

            <p>
              The <strong>Events</strong> list under the agents on{` `}
              <strong>Settings → Agents</strong> is the team&apos;s stream:
              every event that fired, where it came from and when.
            </p>
            <p>
              Agents read prompts over MCP too: see{` `}
              <a href="/docs/mcp/#tools">
                <code>exponential_team_config</code>
              </a>
              {` `}(<code>prompts_list</code>, <code>prompts_get</code>).
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}

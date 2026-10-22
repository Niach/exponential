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
              name in one row over the body. Opening a row opens it.
            </p>
            <p>
              Owners create, edit and delete prompts. Every member can read
              and run them.
            </p>

            <DocShot
              view="settings-prompts"
              caption="Settings → Prompts"
            />
          </DocsSection>

          {/* ── 03 The / menu ── */}
          <DocsSection id="composer" num="03" label="The / menu">
            <h2>The / menu</h2>
            <p>
              In the <strong>Home</strong> composer, type <code>/</code> to
              list the team&apos;s prompts. Keep typing to filter, then press
              {` `}
              <strong>Enter</strong> or click one. The prompt becomes the
              session&apos;s subject, and the send starts the session with its
              body plus what you typed.
            </p>

            <DocShot
              view="composer-slash-menu"
              caption="The Home composer's / menu"
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

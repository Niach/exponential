/* EXP-1503: exponential.at/agent/, eAgent's one docs page. Facts:
   apps/desktop/crates/eagent (EXP-1426, its README = the seams and the
   extraction order); renamed from exponential-agent-runtime (released as
   agent-runtime-v0.1.0) by EXP-1505, which keeps the old name one release
   as a shim; the local hub and chat view-model EXP-1509..1512: planned
   until they merge. */
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, IcArrow, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `rules`, num: `02`, label: `Your agent, your login` },
  { id: `seams`, num: `03`, label: `Seams` },
  { id: `install`, num: `04`, label: `Install` },
]

const IN_APP = <StatusChip status="in-progress">in the app</StatusChip>
const PLANNED = <StatusChip status="planned">planned</StatusChip>

const SEAMS: [string, string][] = [
  [`LaunchSpec`, `What to spawn and how: program, env, working directory, agent, model, resume, MCP servers`],
  [`EventSink`, `Where the mapped events of a run go`],
  [`RunBackend`, `The side-car a run brings up, its record and its end sequence`],
  [`AccountEnv`, `The config directory and env of the spawn`],
  [`McpProvider`, `The MCP servers a run gets`],
  [`HostTools`, `The host's own tools: how they are shown and previewed`],
]

export default function Agent({ path }: PageProps) {
  return (
    <>
      <PageHero
        eyebrow="Library"
        title="eAgent"
        cta={
          <a className="btn btn-ghost" href={LINKS.source(`apps/desktop/crates/eagent`)}>
            Source <IcArrow />
          </a>
        }
      >
        Runs your own Claude Code or Codex: spawns the unmodified CLI over ACP, streams what it does and wires its MCP servers.
      </PageHero>
      <DocsLayout nav={LIBRARIES_NAV} title="Libraries" sections={SECTIONS} currentPath={path}>
        <DocsSection id="status" num="01" label="Status">
          <h2>Status</h2>
          <p>
            In progress. The core runs every coding session of the Exponential desktop app and the CLI daemon, and is being
            moved out of the app crate by crate. The crate is <code>eagent</code>; the old name,{` `}
            <code>exponential-agent-runtime</code>, stays one more release as a re-export shim.
          </p>
          <DocsTable
            head={[`Part`, `What`, ``]}
            rows={[
              [<strong key="p">The runtime</strong>, `Spawn, ACP, events, login, MCP wiring`, IN_APP],
              [<strong key="p">The library</strong>, <span key="w"><code>eagent</code>, a git dependency, tag <code>eagent-v*</code></span>, <StatusChip key="s" status="in-progress">0.1.0</StatusChip>],
              [<strong key="p">Local orchestration</strong>, `Session trees, wake-ups and reports for one person, without a server`, PLANNED],
              [<strong key="p">Chat view-model</strong>, `One transcript, composer and question model any eUI host renders`, PLANNED],
            ]}
          />
        </DocsSection>
        <DocsSection id="rules" num="02" label="Your agent, your login">
          <h2>Your agent, your login</h2>
          <p>
            eAgent runs the vendor&apos;s own binary on your machine with your own subscription. It never reads, copies,
            moves or refreshes a token: the CLI logs in itself, in a terminal eAgent drives. No server and no Exponential
            app crate: one local account is enough.
          </p>
        </DocsSection>
        <DocsSection id="seams" num="03" label="Seams">
          <h2>Seams</h2>
          <p>Everything host-specific is a trait the host implements; a local host uses the defaults.</p>
          <DocsTable head={[`Trait`, `Answers`]} rows={SEAMS.map(([name, what]) => [<code key="n">{name}</code>, what])} />
        </DocsSection>
        <DocsSection id="install" num="04" label="Install">
          <h2>Install</h2>
          <p>
            A git dependency on a release tag, never from crates.io: the contract is compiled in from the repo&apos;s
            fixtures. Pin the tag, never a branch.
          </p>
          <DocsCode language="toml">{`[dependencies]
eagent = { git = "https://github.com/Niach/exponential", tag = "eagent-v0.1.0" }`}</DocsCode>
        </DocsSection>
      </DocsLayout>
    </>
  )
}

/* EXP-1504: exponential.at/core/, eCore's one docs page. Facts:
   apps/desktop/crates/ecore (its README) and packages/ecore (contract.json
   → Rust, TS, Swift, Kotlin; fixtures/ecore.json ×4). No release tag yet. */
import { DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, IcArrow, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `types`, num: `02`, label: `The types` },
  { id: `rule`, num: `03`, label: `Types only` },
]

const TYPES: [string, string][] = [
  [`Ids`, `SessionId, RunId, SpaceId, WindowId, DeviceId, one id rule`],
  [`Grants`, `A bearer Token and a Grant: session, run, capabilities, expiry`],
  [`Capabilities`, `capture, input:person, space, lend, ui, system`],
  [`Delivery verdict`, `delivered_verified, not_delivered or retry_safe`],
  [`Errors`, `Failure { code, message, retryable }, refusals included`],
  [`Event envelope`, `Envelope { v, id, kind, at, source, sessionId?, data }`],
]

export default function Core({ path }: PageProps) {
  return (
    <>
      <PageHero
        eyebrow="Library"
        title="eCore"
        cta={
          <a className="btn btn-ghost" href={LINKS.source(`apps/desktop/crates/ecore`)}>
            Source <IcArrow />
          </a>
        }
      >
        The plain types every module shares. Modules call each other over MCP through eMCP; what they pass is spelled here.
      </PageHero>
      <DocsLayout nav={LIBRARIES_NAV} title="Libraries" sections={SECTIONS} currentPath={path}>
        <DocsSection id="status" num="01" label="Status">
          <h2>Status</h2>
          <p>
            Built, not released. One contract generates the Rust crate <code>ecore</code> and its TypeScript, Swift and
            Kotlin twins, and one fixture replays on all four. No release tag is cut yet.
          </p>
          <DocsTable
            head={[`Part`, `Where`, ``]}
            rows={[
              [<strong key="p">Rust</strong>, <code key="w">ecore</code>, <StatusChip key="s" status="in-progress">built</StatusChip>],
              [<strong key="p">TypeScript, Swift, Kotlin</strong>, `Generated from packages/ecore/contract.json`, <StatusChip key="s" status="in-progress">built</StatusChip>],
              [<strong key="p">Release</strong>, `A tag and published packages`, <StatusChip key="s" status="planned">planned</StatusChip>],
            ]}
          />
        </DocsSection>
        <DocsSection id="types" num="02" label="The types">
          <h2>The types</h2>
          <DocsTable head={[`Type`, `What`]} rows={TYPES.map(([name, what]) => [<strong key="n">{name}</strong>, what])} />
        </DocsSection>
        <DocsSection id="rule" num="03" label="Types only">
          <h2>Types only</h2>
          <p>
            No I/O, no async runtime, no network, no UI and no dependency on any other module: serde and serde_json only. A
            test in the crate enforces it.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}

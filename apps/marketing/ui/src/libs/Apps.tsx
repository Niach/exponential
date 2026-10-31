/* EXP-1421: exponential.at/apps/, eApps' one docs page: the app package
   format. Facts: packages/exponential-ui/docs/package-format.md (format
   version 2, VAPP-161) and src/host/ (package.ts, records.ts,
   record-store.ts, tools.ts, exposes.ts, signature.ts), in @exponential_app/eui 0.3.0
   (exposes + upstreams, VAPP-180);
   domain-contract fixtures app-records.json and app-export.json (what the
   app ships). Package names: docs/libraries.md. */
import type { ReactNode } from "react"
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `parts`, num: `02`, label: `One package` },
  { id: `example`, num: `03`, label: `A package` },
  { id: `rules`, num: `04`, label: `Data, never code` },
  { id: `exposes`, num: `05`, label: `MCP tools` },
  { id: `today`, num: `06`, label: `Use it today` },
  { id: `packages`, num: `07`, label: `Packages` },
]

const IN_APP = <StatusChip status="in-progress">in the app</StatusChip>
const IN_EUI = <StatusChip status="shipped">in eUI</StatusChip>
const PLANNED = <StatusChip status="planned">planned</StatusChip>

const PARTS: [string, string, ReactNode][] = [
  [`Manifest`, `id, name, version, the eUI catalog it targets, icon, theme, description`, IN_APP],
  [`Templates`, `Screens as A2UI components, one per template id`, IN_APP],
  [`Format version`, `formatVersion (absent = 1); a host refuses a newer one`, IN_EUI],
  [`Bindings`, `A data model path filled live from a source, e.g. records:entry?sort=date`, IN_APP],
  [`Function allowlist`, `The functions the screens may call; everything else is refused`, IN_APP],
  [`Records`, `The app's own data: kinds with a small JSON Schema, records.upsert and records.delete`, IN_APP],
  [`Records source`, `The neutral records:<kind>?query source; the host provides the store (RecordStore)`, IN_APP],
  [`Tools`, `MCP servers or scripts as tools.<id>.<name>; the host shows toolConsent() before the first call`, IN_EUI],
  [`Exposes`, `MCP tools the app offers: declarative bindings over its records, the host's actions or an upstream, run as the calling person`, IN_EUI],
  [`Upstreams`, `The MCP servers its upstream bindings call, with each person's own credential`, IN_EUI],
  [`Signed package file`, `An author and an Ed25519 signature over RFC 8785 canonical JSON (signPackageFile, verifyPackageFile)`, IN_EUI],
  [`Extensions`, `Custom eUI component catalogs with a minimum version, checked against the host's`, IN_EUI],
]

const EXAMPLE = `{
  "formatVersion": 2,
  "id": "hours",
  "name": "Working hours",
  "version": "1.4.0",
  "catalogId": "https://ui.exponential.at/catalogs/core/v1",
  "sources": ["records:entry"],
  "records": {
    "entry": {
      "schema": {
        "type": "object",
        "properties": {
          "date": { "type": "string", "format": "date" },
          "hours": { "type": "number" }
        },
        "required": ["date", "hours"]
      }
    }
  },
  "templates": {
    "month": {
      "components": [{ "id": "root", "component": "Text", "text": "Hours" }],
      "bindings": [{ "path": "/entries", "source": "records:entry?sort=date" }]
    }
  }
}`

const EXPOSES = `"upstreams": ["m5"],
"exposes": [
  { "name": "list_entries", "title": "List entries", "description": "The time entries of a day.",
    "input": { "type": "object", "properties": { "day": { "type": "string" } } },
    "access": "read",
    "run": { "records": { "op": "list", "kind": "entry", "where": { "date": "\${input.day}" } } } },
  { "name": "find_customer", "title": "Find a customer", "description": "Search m5 customers.",
    "input": { "type": "object", "properties": { "q": { "type": "string" } } },
    "access": "read",
    "run": { "upstream": "m5", "tool": "search_customers", "args": { "query": "\${input.q}" } } }
]`

const VALIDATE = `import { toolConsent, validatePackage, verifyPackageFile } from "@exponential_app/eui/host"

const file = JSON.parse(text)
const issues = validatePackage(file.package)
// [] or [{ path: "/templates", message: "templates is a non-empty object" }, …]
const signed = await verifyPackageFile(file) // unsigned | valid (author) | invalid
const consent = toolConsent(file.package)    // one card row per tool, before its first call`

export default function Apps({ path }: PageProps) {
  return (
    <>
      <PageHero eyebrow="Library" title="eApps">
        One package format for apps an agent builds: a manifest, eUI screens, bindings, records and tools. Data, never code.
      </PageHero>
      <DocsLayout nav={LIBRARIES_NAV} title="Libraries" sections={SECTIONS} currentPath={path}>
        <DocsSection id="status" num="01" label="Status">
          <h2>Status</h2>
          <p>
            The format runs today: every <a className="content-link" href="/docs/apps/#team-apps">team app</a> in Exponential is
            one of these packages, painted by eUI on web, desktop, iOS and Android. Format version 2 (records, tools with
            consent, versioned extensions, signed package files, MCP tools) ships in eUI&apos;s host module; the spec is{` `}
            <a className="content-link" href={LINKS.source(`packages/exponential-ui/docs/package-format.md`)}>
              package-format.md
            </a>
            . The standalone library is planned.
          </p>
        </DocsSection>
        <DocsSection id="parts" num="02" label="One package">
          <h2>One package</h2>
          <DocsTable head={[`Part`, `What`, ``]} rows={PARTS.map(([part, what, chip]) => [<strong key="p">{part}</strong>, what, chip])} />
        </DocsSection>
        <DocsSection id="example" num="03" label="A package">
          <h2>A package</h2>
          <p>
            A working-hours app: one record kind, one screen bound to its records. In Exponential, an app shared as a file is
            this package inside a <code>.app.json</code> with the app&apos;s name, icon and colour, and its records if you
            include them.
          </p>
          <DocsCode language="json">{EXAMPLE}</DocsCode>
        </DocsSection>
        <DocsSection id="rules" num="04" label="Data, never code">
          <h2>Data, never code</h2>
          <p>
            A package carries no scripts that run in the app. Screens are A2UI, data comes in through bindings to the sources
            the package lists, and actions call only the functions it allows. A host checks each against its own policy, so
            an app from a stranger can&apos;t do more than its package says, and whatever acts as you needs your consent.
          </p>
          <p>
            Records are the app&apos;s own rows: written without a prompt, validated against their kind&apos;s schema before
            they are stored.
          </p>
        </DocsSection>
        <DocsSection id="exposes" num="05" label="MCP tools">
          <h2>MCP tools</h2>
          <p>
            A package can offer MCP tools to its host: <code>exposes</code> lists them as declarative bindings over its own
            records, a host action or a tool of one of its <code>upstreams</code>. The host lists them as{` `}
            <code>&lt;app&gt;__&lt;name&gt;</code> and runs them as the calling person, so no device has to be online.
            Deletes and other destructive names are never exposed, and a <code>read</code> tool can&apos;t write.
          </p>
          <DocsCode language="json">{EXPOSES}</DocsCode>
          <p>
            In Exponential, a team&apos;s app tools are served at <code>/api/mcp/apps</code>; see{` `}
            <a className="content-link" href="/docs/mcp/">MCP</a> for consent and access.
          </p>
        </DocsSection>
        <DocsSection id="today" num="06" label="Use it today">
          <h2>Use it today</h2>
          <p>
            The package type, its validator, the records store, tool consent and signatures are in eUI&apos;s host module
            (format version 2 since 0.2.0; <code>exposes</code> and <code>upstreams</code> since 0.3.0):
          </p>
          <DocsCode language="shell">npm install @exponential_app/eui</DocsCode>
          <DocsCode language="ts">{VALIDATE}</DocsCode>
        </DocsSection>
        <DocsSection id="packages" num="07" label="Packages">
          <h2>Packages</h2>
          <p>Planned names, one per platform.</p>
          <DocsTable
            head={[`Package`, `Registry`, ``]}
            rows={[
              [<code key="n">exponential-apps</code>, `crates.io`, PLANNED],
              [<code key="n">@exponential_app/apps</code>, `npm`, PLANNED],
              [<code key="n">at.exponential:apps</code>, `Maven Central`, PLANNED],
              [<code key="n">ExponentialApps</code>, `SwiftPM`, PLANNED],
            ]}
          />
        </DocsSection>
      </DocsLayout>
    </>
  )
}

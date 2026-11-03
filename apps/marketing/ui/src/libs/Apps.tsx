/* EXP-1606: exponential.at/apps/, eApps in one screen, the /ui/ home's
   twin: what an app is (data, never code), the three executors, one package
   painted by eUI everywhere, records + triggers + upstreams, example apps,
   building one by asking, then the format for library users. Visual first,
   little text. Source of truth: docs/spec/apps.md (its mocks are copied to
   public/eapps/); what the spec marks 🆕/❌ reads "coming" here. The
   in-product docs are /docs/apps/#team-apps; the format's prose spec is
   packages/exponential-ui/docs/package-format.md. */
import type { ReactNode } from "react"
import {
  Bot,
  Boxes,
  Braces,
  CalendarClock,
  Clock,
  Database,
  Laptop,
  LayoutTemplate,
  ListChecks,
  Monitor,
  Plug,
  Server,
  ShieldCheck,
  Smartphone,
  Sparkles,
  Users,
  Wrench,
} from "lucide-react"
import { DocsCode } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, IcArrow } from "../components/Content"
import { COMPONENT_COUNT } from "../lib/catalog-facts"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const TODAY = <StatusChip status="shipped">today</StatusChip>
const COMING = <StatusChip status="planned">coming</StatusChip>

/* The hero's package: Stunden, trimmed to what reads at a glance. */
const PACKAGE = `{
  "formatVersion": 3,
  "id": "acme.stunden",
  "name": "Stunden",
  "records": {
    "entry": { "schema": { "date": "date",
                           "hours": "number" } }
  },
  "templates": {
    "month": { "components": [ Chart, Table, … ],
      "bindings": [{ "path": "/entries",
        "source": "records:entry?mine=1" }] }
  },
  "functions": ["records.*"],
  "exposes": [{ "name": "log_hours", … }]
}`

const PARTS: { icon: ReactNode; key: string; what: string }[] = [
  { icon: <Braces size={14} />, key: `manifest`, what: `name, icon, version` },
  { icon: <LayoutTemplate size={14} />, key: `templates`, what: `screens in eUI components` },
  { icon: <Plug size={14} />, key: `bindings`, what: `live data into a screen` },
  { icon: <Database size={14} />, key: `records`, what: `the app's own rows` },
  { icon: <ShieldCheck size={14} />, key: `functions`, what: `what a button may call` },
  { icon: <Wrench size={14} />, key: `exposes`, what: `MCP tools it offers` },
  { icon: <Server size={14} />, key: `upstreams`, what: `MCP servers it calls` },
]

const NEVER = [`scripts`, `fetch`, `background code`, `a hosted process`]

const EXECUTORS: { icon: ReactNode; name: string; when: string; runs: string; chip?: ReactNode }[] = [
  {
    icon: <Monitor size={16} />,
    name: `Client host`,
    when: `while someone looks`,
    runs: `bindings, queries, buttons, forms`,
  },
  {
    icon: <Server size={16} />,
    name: `Server broker`,
    when: `always on`,
    runs: `MCP tool calls; the clock, events and webhooks next`,
  },
  {
    icon: <Bot size={16} />,
    name: `Agent`,
    when: `when it needs judgement`,
    runs: `a hand-off with the app's data`,
    chip: COMING,
  },
]

const PLATFORMS: { icon: ReactNode; name: string }[] = [
  { icon: <Monitor size={13} />, name: `Web` },
  { icon: <Laptop size={13} />, name: `Desktop` },
  { icon: <Smartphone size={13} />, name: `iOS` },
  { icon: <Smartphone size={13} />, name: `Android` },
]

const INPUTS: { icon: ReactNode; title: string; note: string; chip: ReactNode }[] = [
  { icon: <Database size={14} />, title: `Records`, note: `kinds with a schema, synced to every device`, chip: TODAY },
  { icon: <Server size={14} />, title: `Upstreams`, note: `Stripe, GitHub, your own MCP server; each member's own account`, chip: TODAY },
  { icon: <CalendarClock size={14} />, title: `Triggers`, note: `a schedule, a record change, an event, a webhook`, chip: COMING },
]

const OUTPUTS: { icon: ReactNode; title: string; note: string; chip: ReactNode }[] = [
  { icon: <Users size={14} />, title: `Your team`, note: `the same screens on four platforms`, chip: TODAY },
  { icon: <Wrench size={14} />, title: `MCP tools`, note: `stunden__log_hours in Claude Code or OpenClaw`, chip: TODAY },
  { icon: <Bot size={14} />, title: `An agent`, note: `"follow up with this customer"`, chip: COMING },
]

const EXAMPLES: { icon: ReactNode; name: string; what: string; parts: string[]; chip: ReactNode }[] = [
  { icon: <Clock size={15} />, name: `Stunden`, what: `Hours per day, totals per month.`, parts: [`records`, `Chart`, `exposes`], chip: TODAY },
  { icon: <ListChecks size={15} />, name: `Release checklist`, what: `Steps per release, ticked by the team.`, parts: [`records`, `exp:issues`], chip: TODAY },
  { icon: <Server size={15} />, name: `Admin console`, what: `One screen over Stripe, GitHub and your API.`, parts: [`upstreams`, `exposes`], chip: TODAY },
  { icon: <Users size={15} />, name: `CRM`, what: `Contacts, companies, deals in stages.`, parts: [`relations`, `screens`], chip: COMING },
  { icon: <Boxes size={15} />, name: `Inventory`, what: `Items, stock moves, a low-stock alert.`, parts: [`sums`, `triggers`, `offline`], chip: COMING },
  { icon: <CalendarClock size={15} />, name: `On-call`, what: `A weekly rotation that flips on Monday.`, parts: [`schedule`, `date maths`], chip: COMING },
]

const STEPS = [
  { num: `01`, title: `New app`, body: `Opens the editor on a draft. No form.` },
  { num: `02`, title: `Describe it`, body: `"Log working hours." The agent names it and writes the package.` },
  { num: `03`, title: `Watch it build`, body: `Every change is a version; the preview repaints as it lands.` },
]

const VALIDATE = `import { toolConsent, validatePackage, verifyPackageFile } from "@exponential_app/eui/host"

const issues = validatePackage(file.package)    // [] or [{ path, message }, …]
const signed = await verifyPackageFile(file)     // unsigned | valid (author) | invalid`

function Node({ icon, title, note, chip }: { icon: ReactNode; title: string; note: string; chip: ReactNode }) {
  return (
    <div className="ea-node glass-card">
      <span className="ea-node-head">
        {icon}
        <b>{title}</b>
        {chip}
      </span>
      <span className="ea-node-note">{note}</span>
    </div>
  )
}

export default function Apps(_: PageProps) {
  return (
    <>
      <section className="home-hero">
        <div className="shell">
          <div className="home-hero-text">
            <div className="section-eyebrow">eApps</div>
            <h1 className="home-title">
              Apps are data,
              <br />
              <em>never code.</em>
            </h1>
            <p className="home-sub">Ask an agent for an app. It writes one package, and eUI paints it on web, desktop, iOS and Android.</p>
            <div className="home-cta">
              <a className="btn btn-primary" href="/docs/apps/#team-apps">
                Read the docs <IcArrow />
              </a>
              <a className="btn btn-ghost" href="#package">
                The package
              </a>
              <a className="btn btn-ghost" href="/ui/">
                eUI
              </a>
            </div>
          </div>
          <div className="ea-hero" role="img" aria-label="A package (records, templates, bindings, functions, exposes) on the left; the same package painted as the Stunden time tracker on a desktop and on a phone on the right">
            <pre className="ea-pkg glass-card">
              <span className="ea-pkg-file">stunden.app.json</span>
              <code>{PACKAGE}</code>
            </pre>
            <span className="ea-arrow" aria-hidden>
              <span>eUI</span>
              <IcArrow size={16} />
            </span>
            <div className="ea-shots">
              <img className="ea-shot ea-shot-desktop" src="/eapps/time-tracker.webp" width={2360} height={1125} alt="" loading="eager" decoding="async" />
              <img className="ea-shot ea-shot-phone" src="/eapps/time-tracker-phone.webp" width={780} height={736} alt="" loading="eager" decoding="async" />
            </div>
          </div>
        </div>
      </section>

      <section className="home-section" id="package">
        <div className="shell">
          <div className="section-eyebrow">What an app is</div>
          <h2 className="section-title">One package. Data, never code.</h2>
          <div className="ea-parts">
            {PARTS.map((p) => (
              <div key={p.key} className="ea-part glass-card">
                <span className="ea-part-icon">{p.icon}</span>
                <code>{p.key}</code>
                <span>{p.what}</span>
              </div>
            ))}
          </div>
          <div className="ea-never" aria-label="A package never carries scripts, fetch, background code or a hosted process">
            <span className="ea-never-label">never in a package</span>
            {NEVER.map((n) => (
              <s key={n}>{n}</s>
            ))}
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Three executors</div>
          <h2 className="section-title">Three things run an app. None of them runs app code.</h2>
          <ol className="home-steps ea-exec">
            {EXECUTORS.map((e) => (
              <li key={e.name} className="home-step glass-card">
                <span className="ea-exec-icon">{e.icon}</span>
                <h3>
                  {e.name} {e.chip}
                </h3>
                <span className="ea-exec-when">{e.when}</span>
                <p>{e.runs}</p>
              </li>
            ))}
          </ol>
          <div className="ea-upstream-note">
            <Server size={14} aria-hidden />
            <span>
              Code that must always run lives behind an <strong>upstream MCP server</strong> your team already runs.
            </span>
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Painted by eUI</div>
          <h2 className="section-title">The same package on every screen.</h2>
          <div className="ea-platforms">
            {PLATFORMS.map((p) => (
              <span key={p.name} className="ea-platform">
                {p.icon} {p.name}
              </span>
            ))}
            <a className="ea-platform ea-platform-link" href="/ui/">
              {`${COMPONENT_COUNT} native components`} <IcArrow size={11} />
            </a>
          </div>
          <figure className="ea-figure">
            <div className="ea-twin">
              <img className="ea-shot" src="/eapps/time-tracker.webp" width={2360} height={1125} alt="Stunden on the web: a month of hours as a bar chart and a table" loading="lazy" decoding="async" />
              <img className="ea-shot" src="/eapps/time-tracker-phone.webp" width={780} height={736} alt="Stunden on a phone: one row per day and a Log hours button" loading="lazy" decoding="async" />
            </div>
            <figcaption>
              <span className="ea-concept">concept</span> Stunden, a time tracker: one package, desktop and phone, drawn with eUI.
            </figcaption>
          </figure>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Records, triggers, upstreams</div>
          <h2 className="section-title">Data in, tools out.</h2>
          <div className="ea-flow" role="img" aria-label="Into the app: its records, upstream MCP servers and, coming, triggers. Out of the app: the team's screens, MCP tools for coding agents and, coming, a hand-off to an agent">
            <div className="ea-flow-col">
              {INPUTS.map((n) => (
                <Node key={n.title} {...n} />
              ))}
            </div>
            <span className="ea-wire" aria-hidden>
              <i />
            </span>
            <div className="ea-core glass-card">
              <Sparkles size={18} aria-hidden />
              <b>your app</b>
              <em>one package · versioned · per team</em>
            </div>
            <span className="ea-wire" aria-hidden>
              <i />
            </span>
            <div className="ea-flow-col">
              {OUTPUTS.map((n) => (
                <Node key={n.title} {...n} />
              ))}
            </div>
          </div>
          <figure className="ea-figure">
            <img className="ea-shot" src="/eapps/m5-admin.webp" width={2560} height={976} alt="An admin console over Stripe, GitHub and Home Assistant, each member connected with their own account" loading="lazy" decoding="async" />
            <figcaption>
              <span className="ea-concept">concept</span> An admin console over three upstreams, each member on their own account.
            </figcaption>
          </figure>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Example apps</div>
          <h2 className="section-title">Team tools, built by asking.</h2>
          <div className="ea-examples">
            {EXAMPLES.map((x) => (
              <div key={x.name} className="ea-example glass-card">
                <span className="ea-example-head">
                  <span className="ea-part-icon">{x.icon}</span>
                  <b>{x.name}</b>
                  {x.chip}
                </span>
                <p>{x.what}</p>
                <span className="ea-tags">
                  {x.parts.map((t) => (
                    <code key={t}>{t}</code>
                  ))}
                </span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Build one</div>
          <h2 className="section-title">Describe it. An agent builds it.</h2>
          <ol className="home-steps">
            {STEPS.map((s) => (
              <li key={s.num} className="home-step glass-card">
                <span className="home-step-num">{s.num}</span>
                <h3>{s.title}</h3>
                <p>{s.body}</p>
              </li>
            ))}
          </ol>
          <div className="home-cta ea-cta">
            <a className="btn btn-primary" href={LINKS.app}>
              Try it free <IcArrow />
            </a>
            <a className="btn btn-ghost" href="/docs/apps/#build">
              How the editor works
            </a>
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell home-packages">
          <div>
            <h2 className="section-title">The format, open</h2>
            <p className="section-sub">
              Format version 2 ships in eUI&apos;s host module: the package type, its validator, the records store and signed package files. Spec:{` `}
              <a className="content-link" href={LINKS.source(`packages/exponential-ui/docs/package-format.md`)}>
                package-format.md
              </a>
              .
            </p>
          </div>
          <div>
            <DocsCode language="shell">npm install @exponential_app/eui</DocsCode>
            <DocsCode language="ts">{VALIDATE}</DocsCode>
            <DocsTable
              head={[`Standalone package`, `Registry`, ``]}
              rows={[
                [<code key="n">eapps</code>, `crates.io`, COMING],
                [<code key="n">@exponential_app/eapps</code>, `npm`, COMING],
                [<code key="n">at.exponential:eapps</code>, `Maven Central`, COMING],
                [<code key="n">ExponentialApps</code>, `SwiftPM`, COMING],
              ]}
            />
          </div>
        </div>
      </section>
    </>
  )
}

import { motion } from "motion/react"
import type { Platform } from "@exp/view-catalog"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { AgentIconRow } from "./components/agent-icons"
import { StatusChip, type Status } from "./components/blog/StatusChip"
import { shotSrc } from "./components/DocShot"
import { HeroDownload } from "./components/HeroDownload"
import { HeroPhone } from "./components/HeroPhone"
import { IcArrow } from "./components/icons"
import { TalkOverlay } from "./components/TalkOverlay"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"
import { LIBRARIES } from "../ui/src/lib/libraries"

/* NEW-15: the home page is the product in four parts (eTalk, agents, apps,
   eUI), each one or two sentences over REAL parts: committed captures
   (shots/) and live @exp/ui islands. The hero is an agent's phone chat
   (HeroPhone, src/ui-demos/AgentChat.tsx); eTalk states only what has
   shipped. EXP-1421: then the library family, one
   docs card per lib with its state. EXP-1606: genUI and apps are their
   own visual sections, linking /ui/ and /apps/. 2026-10-31: plans (#1471), sealed
   sessions + steering over eNet (EXP-1457), editor-first apps (VAPP-176). */

function Shot({ view, platform, className }: { view: string; platform: Platform; className: string }) {
  const { src, frame, title } = shotSrc(view, platform)
  return (
    <img className={className} src={src} width={frame.w} height={frame.h} loading={`lazy`} decoding={`async`} alt={title} />
  )
}

const TALK: { text: string; status: Status; label: string }[] = [
  { text: `Push to talk on your desktop`, status: `in-progress`, label: `beta` },
  { text: `Spoken replies in English and German`, status: `in-progress`, label: `beta` },
  { text: `Call your agent from your phone`, status: `in-progress`, label: `beta` },
  { text: `Wake word`, status: `in-progress`, label: `beta` },
]

const GENUI_JSON = `{"createSurface":{"surfaceId":"main"}}
{"updateComponents":{
 "surfaceId":"main","components":[
  {"id":"root","component":"Card",
   "title":"Deploy 0.18.84?"},
  {"id":"go","component":"Button",
   "label":"Deploy"}]}}`

const APP_PARTS = [`templates`, `bindings`, `records`, `functions`, `exposes`, `upstreams`]

function Feature({
  id,
  title,
  text,
  link,
  children,
}: {
  id: string
  title: string
  text: string
  link?: { href: string; label: string }
  children: React.ReactNode
}) {
  return (
    <section id={id} className={`home-feature`}>
      <div className={`shell aw-grid`}>
        <motion.div className={`aw-copy`} {...sectionReveal}>
          <h2 className={`section-title`}>{title}</h2>
          <p className={`section-sub`}>{text}</p>
          {link && (
            <a className={`btn btn-ghost`} href={link.href}>
              {link.label} <IcArrow size={12} />
            </a>
          )}
        </motion.div>
        <motion.figure className={`aw-shots`} {...sectionReveal}>
          {children}
        </motion.figure>
      </div>
    </section>
  )
}

export function HomePage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className={`home-hero`} id={`product`}>
          <motion.div className={`shell home-hero-grid`} variants={heroStagger} initial={`hidden`} animate={`visible`}>
            <div className={`home-hero-copy`}>
              <motion.h1 className={`hero-title`} variants={heroChild}>
                <span className={`hero-title-line`}>The next generation</span>
                <span className={`hero-title-line`}>dev platform for</span>
                <span className={`hero-title-line`}>teams and agents</span>
              </motion.h1>
              <motion.p className={`hero-sub`} variants={heroChild}>
                Agents plan the work, write the code, build your apps and answer when you talk to them. On web, desktop,
                iOS and Android.
              </motion.p>
              <motion.div className={`hero-cta`} variants={heroChild}>
                <HeroDownload />
              </motion.div>
              <motion.div variants={heroChild}>
                <AgentIconRow />
              </motion.div>
            </div>
            <motion.div variants={heroChild}>
              <HeroPhone />
            </motion.div>
          </motion.div>
        </section>

        <section id={`talk`} className={`home-feature home-talk`}>
          <div className={`shell aw-grid`}>
            <motion.div className={`aw-copy`} {...sectionReveal}>
              <h2 className={`section-title`}>Talk to it</h2>
              <p className={`section-sub`}>
                Hold a key and speak. Your agent gets it as a message and reads its reply aloud.
              </p>
              <ul className={`home-talk-list`}>
                {TALK.map((v) => (
                  <li key={v.text}>
                    <span>{v.text}</span>
                    <StatusChip status={v.status}>{v.label}</StatusChip>
                  </li>
                ))}
              </ul>
              <a className={`btn btn-ghost`} href={`/talk/`}>
                How eTalk works <IcArrow size={12} />
              </a>
            </motion.div>
            <motion.figure className={`home-talk-stage`} {...sectionReveal}>
              <TalkOverlay />
            </motion.figure>
          </div>
        </section>

        <Feature
          id={`agents`}
          title={`Agents work your boards`}
          text={`They run Claude Code or Codex on your own machine and subscription. Give one a job, message it like a teammate, review its pull request. Reports from the feedback widget on your site land on a board, and an agent can pick them up.`}
          link={{ href: LINKS.downloadPage, label: `Get the apps` }}
        >
          <Shot view={`agent-channel`} platform={`desktop`} className={`aw-shot aw-shot-desktop`} />
          <Shot view={`agent-channel`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
        </Feature>

        <Feature
          id={`plans`}
          title={`Plans you can read`}
          text={`Before big work, an agent writes its plan as a page: an outline, pictures and the questions it needs you to answer. Then it shows what it built as stories.`}
          link={{ href: `/docs/coding/`, label: `How coding works` }}
        >
          <Shot view={`plan-page`} platform={`desktop`} className={`aw-shot aw-shot-desktop`} />
          <Shot view={`plan-page`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
        </Feature>

        <Feature
          id={`sealed`}
          title={`Sealed end to end`}
          text={`Session content is sealed on your devices; our server stores only ciphertext. Watch and steer a running session from your phone over eNet, straight to the machine it runs on.`}
          link={{ href: `/net/`, label: `How eNet works` }}
        >
          <Shot view={`session`} platform={`desktop`} className={`aw-shot aw-shot-desktop`} />
          <Shot view={`steering`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
        </Feature>

        <section id={`genui`} className={`home-feature`}>
          <div className={`shell aw-grid`}>
            <motion.div className={`aw-copy`} {...sectionReveal}>
              <h2 className={`section-title`}>Generative UI</h2>
              <p className={`section-sub`}>
                Agents answer with an interface, not a wall of text. One A2UI message, painted natively on every platform by
                eUI, our open framework.
              </p>
              <a className={`btn btn-ghost`} href={`/ui/`}>
                eUI <IcArrow size={12} />
              </a>
            </motion.div>
            <motion.figure className={`aw-shots hx-stage`} {...sectionReveal}>
              <Shot view={`exponential-ui-demo`} platform={`desktop`} className={`aw-shot aw-shot-desktop hx-shot-desktop`} />
              <Shot view={`exponential-ui-demo`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
              <pre className={`hx-code glass-card`} aria-hidden>
                <span className={`hx-file`}>a2ui · jsonl</span>
                <code>{GENUI_JSON}</code>
              </pre>
            </motion.figure>
          </div>
        </section>

        <section id={`apps`} className={`home-feature`}>
          <div className={`shell aw-grid`}>
            <motion.div className={`aw-copy`} {...sectionReveal}>
              <h2 className={`section-title`}>Apps, built by asking</h2>
              <p className={`section-sub`}>
                Describe a tool and an agent writes it: one package, data, never code. Your team opens it on every
                platform, and its tools reach your agents over MCP.
              </p>
              <ul className={`hx-parts`} aria-label={`What an app package holds`}>
                {APP_PARTS.map((p) => (
                  <li key={p}>
                    <code>{p}</code>
                  </li>
                ))}
              </ul>
              <span className={`hx-links`}>
                <a className={`btn btn-ghost`} href={`/apps/`}>
                  eApps <IcArrow size={12} />
                </a>
                <a className={`btn btn-ghost`} href={`/docs/apps/#team-apps`}>
                  Docs
                </a>
              </span>
            </motion.div>
            <motion.figure className={`aw-shots`} {...sectionReveal}>
              <img
                className={`aw-shot aw-shot-desktop`}
                src={`/eapps/time-tracker.webp`}
                width={2360}
                height={1232}
                loading={`lazy`}
                decoding={`async`}
                alt={`Hours, a time tracker app: a month of hours as a chart and a table`}
              />
              <img
                className={`aw-shot aw-shot-phone hx-phone-wide`}
                src={`/eapps/time-tracker-phone.webp`}
                width={780}
                height={824}
                loading={`lazy`}
                decoding={`async`}
                alt={`The same app on a phone`}
              />
            </motion.figure>
          </div>
        </section>

        <section id={`libraries`} className={`home-feature home-libs`}>
          <motion.div className={`shell`} {...sectionReveal}>
            <h2 className={`section-title`}>Open libraries</h2>
            <p className={`section-sub`}>
              The parts Exponential is built from, open source. eUI, eTalk, ePilot, eNet and eMCP have releases; eApps, eAgent and
              eCore are on their way. Next: <a href={`/blog/exponentialos/`}>ExponentialOS</a> and eDE, its
              desktop, in progress.
            </p>
            <div className={`docs-cards`}>
              {LIBRARIES.map((lib) => (
                <a key={lib.href} className={`docs-card`} href={lib.href}>
                  <span className={`docs-card-title`}>
                    {lib.name}
                    <StatusChip status={lib.status}>{lib.label}</StatusChip>
                  </span>
                  <span className={`docs-card-desc`}>{lib.desc}</span>
                </a>
              ))}
            </div>
          </motion.div>
        </section>

        <section id={`pricing`} className={`home-price-line`}>
          <motion.div className={`shell`} {...sectionReveal}>
            <h2 className={`section-title`}>Free for teams of three</h2>
            <p className={`section-sub`}>€12 per seat a month after that, billed yearly. Free to self-host.</p>
            <div className={`home-pricing-links`}>
              <a className={`btn btn-primary`} href={LINKS.app.login}>
                Get started free <IcArrow size={12} />
              </a>
              <a className={`btn btn-ghost`} href={`/pricing/`}>
                Pricing
              </a>
            </div>
          </motion.div>
        </section>
      </main>
      <SiteFooter />
    </>
  )
}

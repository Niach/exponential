/* ─── The frontpage (MKT-9): "The issue tracker your agents work in." ───
   One idea per section: the hero, three real prompts drawn as the tool
   calls they trigger, workflows, what a run can do, the proof from this
   repo's own history, pricing and the FAQ. No film: the feature tour lives
   on /features/. */
import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader, UneedBadge } from "./components/SiteShell"
import { AgentIconRow } from "./components/agent-icons"
import { BuildProof } from "./components/BuildProof"
import { HeroDownload } from "./components/HeroDownload"
import { HomeFaq } from "./components/HomeFaq"
import { HomePricing } from "./components/HomePricing"
import { IcArrow } from "./components/icons"
import { PromptLoops } from "./components/PromptLoops"
import { WorkflowShowcase } from "./components/WorkflowShowcase"
import {
  heroChild,
  heroStagger,
  heroTitleStagger,
  heroWord,
  sectionReveal,
} from "./lib/animations"
import { MCP_TOOL_COUNT } from "./lib/product-facts"

/* The two authored H1 lines; every word is its own animated span. */
const TITLE_LINES = [
  [`The`, `issue`, `tracker`],
  [`your`, `agents`, `work`, `in.`],
]

/* ── What a run can do (the MCP surface, in plain words) ── */
const ABILITIES = [
  {
    title: `Reads its issue`,
    text: `The run starts from the issue: description, comments and the images inside them, which the agent receives as pictures it can see.`,
  },
  {
    title: `Files follow-ups`,
    text: `Work that is out of scope becomes a new issue on the right board, linked to the one it came from.`,
  },
  {
    title: `Opens and merges its PR`,
    text: `Commits land on the issue's branch, the pull request opens linked to it and statuses move on their own. It merges when you ask.`,
  },
  {
    title: `Starts runs on your other machines`,
    text: `A run can start child runs on another device or agent account, and hears back when they finish.`,
  },
  {
    title: `Publishes screenshots`,
    text: `Results show up on every client, grouped by screen, so web, iOS and Android shots sit side by side.`,
  },
  {
    title: `Asks you, only when it has to`,
    text: `When a decision is yours, it asks: a push to your phone, a row in your inbox, and the run waits for the answer.`,
  },
]

function RunAbilities() {
  return (
    <section className={`abilities-section`} id={`mcp`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <div className={`section-eyebrow`}>What a run can do</div>
          <h2 className={`section-title`}>
            The tracker is the agent's toolbox.
          </h2>
          <p className={`section-sub`}>
            Every run gets the tracker as its MCP server, with the same
            permissions you have. No glue code, no webhooks to wire up.
          </p>
        </motion.div>
        <motion.ul className={`abilities-grid`} {...sectionReveal}>
          {ABILITIES.map((a) => (
            <li key={a.title} className={`ability`}>
              <h3>{a.title}</h3>
              <p>{a.text}</p>
            </li>
          ))}
        </motion.ul>
        <a className={`abilities-link`} href={`/docs/mcp/`}>
          All {MCP_TOOL_COUNT} MCP tools in the docs <IcArrow size={12} />
        </a>
      </div>
    </section>
  )
}

export function HomePage() {
  return (
    <>
      <SiteHeader />

      <main>
        {/* ── Hero (text only) ─────────────────── */}
        <section className={`hero home-hero`} id={`product`}>
          <motion.div
            className={`shell hero-content`}
            variants={heroStagger}
            initial={`hidden`}
            animate={`visible`}
          >
            <motion.p className={`home-hero-eyebrow`} variants={heroChild}>
              Open-source issue tracker for Claude Code and Codex
            </motion.p>
            {/* Words are individually animated spans; the real space text
                nodes between them keep copy/screen-reader output intact.
                Two authored nowrap lines (block + nowrap), so the H1 height
                is constant at every viewport (EXP-176: no page jump). */}
            <motion.h1 className={`hero-title`} variants={heroTitleStagger}>
              {TITLE_LINES.map((line, n) => (
                <span key={line[0]} className={`hero-title-line`}>
                  {/* A real space between the lines, for textContent. */}
                  {n > 0 ? ` ` : ``}
                  {line.map((word, i) => (
                    <span key={word}>
                      {i > 0 ? ` ` : ``}
                      <motion.span className={`hero-word`} variants={heroWord}>
                        {word}
                      </motion.span>
                    </span>
                  ))}
                </span>
              ))}
            </motion.h1>
            <motion.p className={`hero-sub`} variants={heroChild}>
              Every Claude Code or Codex run gets Exponential as its MCP
              server: it reads the issue, files follow-ups, opens and merges
              its PR, and asks you only when it has to. On your machines, on
              your subscription.
            </motion.p>
            <motion.div className={`hero-cta`} variants={heroChild}>
              <HeroDownload />
            </motion.div>
            <motion.div variants={heroChild}>
              <AgentIconRow />
            </motion.div>
            <motion.div className={`home-hero-proof`} variants={heroChild}>
              <UneedBadge />
            </motion.div>
          </motion.div>
        </section>

        <PromptLoops />
        <WorkflowShowcase />
        <RunAbilities />
        <BuildProof />
        <HomePricing />
        <HomeFaq />

        <FooterCTA />
      </main>
      <SiteFooter />
    </>
  )
}

/* ─── PromptLoops — "One prompt. The whole loop." (MKT-9) ───
   Every run Exponential starts gets Exponential as its MCP server, so one
   prompt can drive the tracker, other machines and PRs. Three real prompts,
   each with the TRACE a run leaves: the device, the MCP tool it calls (real
   names, see apps/web/src/lib/mcp/tools.ts) and a plain caption. Steps that
   are the user's own scripts wear a distinct "your script" style.

   SSR: all three panels render (inactive ones carry `hidden`), so every
   step's text is in the prerendered HTML. Steps reveal with a whileInView
   stagger; reduced motion is handled in CSS (prompt-loops.css forces the
   steps visible), which stays correct even before hydration. */
import { motion, type Variants } from "motion/react"
import {
  House,
  Laptop,
  Server,
  Smartphone,
  Terminal,
  type LucideIcon,
} from "lucide-react"
import { useRef, useState, type KeyboardEvent, type ReactNode } from "react"
import { EASE_EXPO, eyebrowDraw, sectionReveal } from "../lib/animations"

type Device = `phone` | `macbook` | `staging` | `home`

const DEVICES: Record<Device, { label: string; icon: LucideIcon }> = {
  phone: { label: `Phone`, icon: Smartphone },
  macbook: { label: `MacBook`, icon: Laptop },
  staging: { label: `Staging server`, icon: Server },
  home: { label: `Home server`, icon: House },
}

/* One line of a trace. `tool` = an Exponential MCP call, `script` = the
   user's own script or command, `you` = a human step, `divider` = a time
   break ("on every merge"). `device` = where it happens / what it targets. */
type Step =
  | { kind: `tool` | `script` | `you`; name: string; caption: string; device?: Device }
  | { kind: `divider`; name: string }

interface Loop {
  id: string
  label: string
  prompt: ReactNode
  /* The line under the prompt: where it was sent from, where it runs. */
  meta: string
  steps: Step[]
}

const LOOPS: Loop[] = [
  {
    id: `feedback`,
    label: `Feedback to fix`,
    meta: `Sent from your phone. Runs on your home server.`,
    prompt: (
      <>
        See which bugs users reported through the widget since yesterday and
        deduplicate them. Start a session on the staging server to reproduce
        each one and cancel the ones that aren't real. Fix the rest in
        parallel and verify them against the staging database. Then send me a
        push with a summary and wait for my OK. Once I give it, merge and
        deploy to production with our deploy script, and build a new iOS
        version on my MacBook for review.
      </>
    ),
    steps: [
      {
        kind: `you`,
        name: `Remote start`,
        caption: `You send the prompt from your phone. The run starts on your home server.`,
        device: `phone`,
      },
      {
        kind: `tool`,
        name: `exponential_issues_list`,
        caption: `Widget reports on the feedback board since yesterday.`,
      },
      {
        kind: `tool`,
        name: `exponential_attachments_get`,
        caption: `Reads the screenshot each report came with.`,
      },
      {
        kind: `tool`,
        name: `exponential_issue_relations_add`,
        caption: `Links the duplicates to the report they repeat.`,
      },
      {
        kind: `tool`,
        name: `exponential_sessions_start`,
        caption: `A child run per bug tries to reproduce it on staging.`,
        device: `staging`,
      },
      {
        kind: `tool`,
        name: `exponential_issues_update`,
        caption: `Cancels the reports that did not reproduce.`,
      },
      {
        kind: `tool`,
        name: `exponential_sessions_start`,
        caption: `Fix runs in parallel, one per real bug.`,
        device: `staging`,
      },
      {
        kind: `tool`,
        name: `exponential_pr_open`,
        caption: `Each fix is checked against the staging database, then opens its PR.`,
      },
      {
        kind: `tool`,
        name: `exponential_sessions_ask_parent`,
        caption: `Pushes the summary to your phone and waits for your OK.`,
        device: `phone`,
      },
      {
        kind: `you`,
        name: `You reply "OK"`,
        caption: `From the notification, on your phone.`,
        device: `phone`,
      },
      {
        kind: `tool`,
        name: `exponential_pr_merge`,
        caption: `Merges the fixes.`,
      },
      {
        kind: `script`,
        name: `./deploy.sh`,
        caption: `Your deploy script ships production.`,
        device: `home`,
      },
      {
        kind: `tool`,
        name: `exponential_sessions_start`,
        caption: `A run on your MacBook builds the iOS app with your release script and submits it for review.`,
        device: `macbook`,
      },
    ],
  },
  {
    id: `automation`,
    label: `Automation on merge`,
    meta: `Set up once. Runs on your home server after every merge.`,
    prompt: (
      <>
        Create an automation that runs whenever a PR gets merged: take
        screenshots of every view of the app on every platform and publish
        them to our styleguide at{` `}
        <a
          href={`https://styleguide.exponential.at`}
          target={`_blank`}
          rel={`noopener`}
        >
          styleguide.exponential.at
        </a>
        . Afterwards check whether the docs are still up to date and file an
        issue for anything that drifted.
      </>
    ),
    steps: [
      {
        kind: `tool`,
        name: `exponential_actions_create`,
        caption: `Saves the job as a team action you can also run by hand.`,
      },
      {
        kind: `tool`,
        name: `exponential_automations_create`,
        caption: `Trigger: the pr_merged event. Runs on your home server.`,
        device: `home`,
      },
      { kind: `divider`, name: `Then, on every merge` },
      {
        kind: `script`,
        name: `./capture-views.sh`,
        caption: `Your capture script shoots every view on web, iOS, Android and desktop.`,
        device: `home`,
      },
      {
        kind: `tool`,
        name: `exponential_sessions_results`,
        caption: `Publishes the screenshots on the run, grouped by view.`,
      },
      {
        kind: `script`,
        name: `./publish-styleguide.sh`,
        caption: `Your script uploads them to styleguide.exponential.at.`,
        device: `home`,
      },
      {
        kind: `tool`,
        name: `exponential_issues_create`,
        caption: `Files an issue for every doc that drifted.`,
      },
    ],
  },
  {
    id: `review`,
    label: `Overnight review`,
    meta: `Started before bed. Runs on your MacBook.`,
    prompt: (
      <>
        Do a code review and file every finding as an issue on a new board,
        labelled and prioritised. Then start the iOS-specific ones on my
        MacBook and the rest on my server, and have them ready for review in
        the morning.
      </>
    ),
    steps: [
      {
        kind: `tool`,
        name: `exponential_boards_create`,
        caption: `A new board for the findings.`,
      },
      {
        kind: `tool`,
        name: `exponential_issues_create`,
        caption: `One issue per finding, with labels and a priority.`,
      },
      {
        kind: `tool`,
        name: `exponential_issue_relations_add`,
        caption: `Blocks relations: what has to land first.`,
      },
      {
        kind: `tool`,
        name: `exponential_workflows_create`,
        caption: `Two workflows: the iOS findings, and the rest.`,
      },
      {
        kind: `tool`,
        name: `exponential_workflows_update`,
        caption: `The iOS workflow runs on your MacBook.`,
        device: `macbook`,
      },
      {
        kind: `tool`,
        name: `exponential_workflows_update`,
        caption: `The rest runs on your home server.`,
        device: `home`,
      },
      {
        kind: `tool`,
        name: `exponential_workflows_start`,
        caption: `Both start. Every finding becomes its own agent run.`,
      },
      { kind: `divider`, name: `By morning` },
      {
        kind: `you`,
        name: `Reviews`,
        caption: `Two final PRs wait in Reviews on your phone.`,
        device: `phone`,
      },
    ],
  },
]

/* The trace reveal: steps land one after another, once, on scroll. */
const traceStagger: Variants = {
  hidden: {},
  visible: { transition: { staggerChildren: 0.07, delayChildren: 0.1 } },
}

const stepReveal: Variants = {
  hidden: { opacity: 0, y: 10 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.5, ease: EASE_EXPO },
  },
}

function DeviceChip({ device }: { device: Device }) {
  const { label, icon: Icon } = DEVICES[device]
  return (
    <span className={`pl-device`}>
      <Icon size={12} strokeWidth={1.8} aria-hidden={true} />
      {label}
    </span>
  )
}

function TraceStep({ step }: { step: Step }) {
  if (step.kind === `divider`) {
    return (
      <motion.li className={`pl-step pl-step--divider`} variants={stepReveal}>
        <span className={`pl-divider-label`}>{step.name}</span>
      </motion.li>
    )
  }
  return (
    <motion.li
      className={`pl-step pl-step--${step.kind}`}
      variants={stepReveal}
    >
      <span className={`pl-node`} aria-hidden={true}>
        {step.kind === `script` && <Terminal size={10} strokeWidth={2} />}
      </span>
      <div className={`pl-step-body`}>
        <div className={`pl-step-head`}>
          {step.kind === `you` ? (
            <span className={`pl-you`}>{step.name}</span>
          ) : (
            <code className={`pl-tool`}>{step.name}</code>
          )}
          {step.kind === `script` && (
            <span className={`pl-script-tag`}>Your script</span>
          )}
          {step.device && <DeviceChip device={step.device} />}
        </div>
        <p className={`pl-caption`}>{step.caption}</p>
      </div>
    </motion.li>
  )
}

export function PromptLoops() {
  const [active, setActive] = useState(0)
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([])

  /* WAI-ARIA tabs: arrows move and activate, Home/End jump. */
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const last = LOOPS.length - 1
    let next: number | null = null
    if (e.key === `ArrowRight` || e.key === `ArrowDown`) {
      next = active === last ? 0 : active + 1
    } else if (e.key === `ArrowLeft` || e.key === `ArrowUp`) {
      next = active === 0 ? last : active - 1
    } else if (e.key === `Home`) next = 0
    else if (e.key === `End`) next = last
    if (next === null) return
    e.preventDefault()
    setActive(next)
    tabRefs.current[next]?.focus()
  }

  return (
    <section className={`pl-section`} id={`loop`}>
      <div className={`shell`}>
        <motion.div className={`pl-head`} {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            MCP in every run
          </motion.span>
          <h2 className={`section-title`}>One prompt. The whole loop.</h2>
          <p className={`section-sub`}>
            Exponential is the MCP server of every run it starts. So a single
            prompt can read the tracker, start runs on your other machines,
            open and merge PRs, and ping you when it needs a decision.
          </p>
        </motion.div>

        <div
          className={`pl-tabs`}
          role={`tablist`}
          aria-label={`Example prompts`}
          onKeyDown={onKeyDown}
        >
          {LOOPS.map((loop, i) => (
            <button
              key={loop.id}
              ref={(el) => {
                tabRefs.current[i] = el
              }}
              type={`button`}
              role={`tab`}
              id={`pl-tab-${loop.id}`}
              aria-selected={i === active}
              aria-controls={`pl-panel-${loop.id}`}
              tabIndex={i === active ? 0 : -1}
              className={`pl-tab${i === active ? ` is-active` : ``}`}
              onClick={() => setActive(i)}
            >
              {loop.label}
            </button>
          ))}
        </div>

        {LOOPS.map((loop, i) => (
          <div
            key={loop.id}
            className={`pl-panel`}
            role={`tabpanel`}
            id={`pl-panel-${loop.id}`}
            aria-labelledby={`pl-tab-${loop.id}`}
            hidden={i !== active}
          >
            {/* The prompt, styled like the app's Agent composer. */}
            <figure className={`pl-prompt`}>
              <figcaption className={`pl-prompt-label`}>
                <span className={`pl-avatar`} aria-hidden={true}>
                  You
                </span>
                Prompt
              </figcaption>
              <blockquote className={`pl-prompt-text`}>
                <p>{loop.prompt}</p>
              </blockquote>
              <p className={`pl-prompt-meta`}>{loop.meta}</p>
            </figure>

            <div className={`pl-trace-wrap`}>
              <p className={`pl-trace-title`}>What the run does</p>
              <motion.ol
                className={`pl-trace`}
                variants={traceStagger}
                initial={`hidden`}
                whileInView={`visible`}
                viewport={{ once: true, amount: 0.1 }}
              >
                {loop.steps.map((step, j) => (
                  <TraceStep key={`${loop.id}-${j}`} step={step} />
                ))}
              </motion.ol>
            </div>
          </div>
        ))}

        <p className={`pl-footnote`}>
          Deploys, store uploads and screenshot capture are your own scripts,
          on your own machines. Every step runs on your devices, with your
          agent subscription.
        </p>
      </div>
    </section>
  )
}

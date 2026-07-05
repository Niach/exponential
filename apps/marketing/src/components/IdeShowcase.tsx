import { motion } from "motion/react"
import {
  FolderGit2,
  GitPullRequest,
  Play,
  SquareTerminal,
  GitCompare,
} from "lucide-react"
import { sectionReveal } from "../lib/animations"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"

const points = [
  {
    icon: <Play size={14} strokeWidth={2.2} />,
    title: `Start coding, one click.`,
    body: `Open any issue in the desktop app and hit Start coding. It clones the repo, cuts a dedicated branch in a worktree, and launches Claude — no gh, no setup.`,
  },
  {
    icon: <SquareTerminal size={14} strokeWidth={2.2} />,
    title: `A real embedded terminal.`,
    body: `The coding session runs in a genuine terminal inside the IDE, not a log view. Watch Claude think, scroll back, or take over the keyboard any time.`,
  },
  {
    icon: <FolderGit2 size={14} strokeWidth={2.2} />,
    title: `Source control, built in.`,
    body: `A files rail and a source-control panel sit right there — see every changed file and its diff as the session works, like the git IDE you already know.`,
  },
  {
    icon: <GitCompare size={14} strokeWidth={2.2} />,
    title: `The Changes tab on every issue.`,
    body: `Review the branch diff on the issue itself — web, desktop, or phone. When Claude opens the PR, it links straight back to the issue.`,
  },
]

export function IdeShowcase() {
  return (
    <section id="ide">
      <div className="shell">
        <div className="agents-grid">
          <motion.div className="agents-copy" {...sectionReveal}>
            <span className="section-eyebrow">Desktop IDE</span>
            <h2 className="section-title">
              Your issues become code, in one window.
            </h2>
            <p className="section-sub">
              The desktop app is a full git IDE built in Rust and gpui. Point
              Claude at an issue and it codes locally — on your machine, your
              subscription, in a terminal you can watch.
            </p>
            <ul className="mobile-bullets">
              {points.map((p) => (
                <li key={p.title}>
                  <span className="mobile-bullet-icon">{p.icon}</span>
                  <div>
                    <strong>{p.title}</strong>
                    <p>{p.body}</p>
                  </div>
                </li>
              ))}
            </ul>
            <div className="mobile-cta">
              <a className="btn btn-primary" href={LINKS.downloads.macos}>
                Download for desktop <IcArrow size={12} />
              </a>
            </div>
          </motion.div>

          <motion.div className="agents-stage" {...sectionReveal}>
            <IdeMockup />
          </motion.div>
        </div>
      </div>
    </section>
  )
}

function IdeMockup() {
  return (
    <div className="ide-wrap">
      <div className="window ide-window">
        <div className="window-bar ide-bar">
          <div className="window-dots">
            <span />
            <span />
            <span />
          </div>
          <div className="ide-titlebar">
            <span className="ide-repo">niach/exponential</span>
            <span className="ide-branch">exp/EXP-214</span>
          </div>
          <span className="ide-startcoding">
            <Play size={10} strokeWidth={2.6} /> Start coding
          </span>
        </div>

        <div className="ide-body">
          <aside className="ide-rail">
            <div className="ide-rail-tabs">
              <span className="is-active">Source Control</span>
              <span>Files</span>
            </div>
            <div className="ide-rail-label">Changes &middot; 3</div>
            <ul className="ide-changes">
              <li>
                <span className="ide-file">webhooks.ts</span>
                <span className="ide-badge is-add">A</span>
              </li>
              <li>
                <span className="ide-file">trpc/issues.ts</span>
                <span className="ide-badge is-mod">M</span>
              </li>
              <li>
                <span className="ide-file">db/schema.ts</span>
                <span className="ide-badge is-mod">M</span>
              </li>
            </ul>
          </aside>

          <div className="ide-term">
            <div className="term-line tl-dim">$ claude &middot; EXP-214</div>
            <div className="term-line tl-cmd">
              &#9656; Add webhook events for issue mutations
            </div>
            <div className="term-line tl-out">Reading db/schema.ts&hellip;</div>
            <div className="term-line tl-tool">
              &#10023; Write integrations/webhooks.ts
            </div>
            <div className="term-line tl-tool">&#10023; Edit trpc/issues.ts</div>
            <div className="term-line tl-ok">
              &#10003; committed &middot; pushed exp/EXP-214
            </div>
            <div className="term-line tl-cmd">
              &#9656; opening pull request&hellip;
              <span className="caret" />
            </div>
          </div>
        </div>

        <div className="ide-changesbar">
          <span className="ide-tab is-active">
            <GitCompare size={11} strokeWidth={2} /> Changes
          </span>
          <span className="ide-tab">Comments</span>
          <span className="ide-diffstat">
            <i className="add">+48</i> <i className="del">&minus;6</i>
            <span className="ide-diffstat-files">3 files</span>
          </span>
          <span className="ide-pr">
            <GitPullRequest size={11} strokeWidth={2.2} /> PR #214
          </span>
        </div>
      </div>
    </div>
  )
}

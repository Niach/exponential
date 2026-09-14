/* ─── The Run and diff faces (agent-session.tsx / session-diff-face.tsx) ───
   EXP-877: an issue-bound run renders UNDER the same work header as its
   issue. The Run face is the transcript in the shared 896px column —
   narration rows led by the sparkles glyph, collapsed tool groups wearing
   the toolGroupSummary caption, settled edit rows, the user's own turns as
   neutral glass bubbles, and the answerable question card — over the ONE-ROW
   steer composer: a field with the round send glyph inline (Stop while the
   agent works and nothing is typed), and the footer under the card with the
   attach `+`, the model picker and the context ring. The diff face is the
   run's changes: the file list over the patches, in place of the
   transcript. */
import { useState } from "react"
import { useWeb } from "./state"
import { ContextRing } from "./bits"
import { RUN_DIFF, RUN_FEED, sessionFor, type RunRow } from "./data"
import {
  ICON_3,
  ICON_35,
  ICON_4,
  IcChevDown,
  IcChevRight,
  IcHelp,
  IcPlus,
  IcSparkles,
  IcStopSquare,
  IcSubmit,
  IcWrench,
} from "./icons"

function FeedRow({
  row,
  answered,
  onAnswer,
}: {
  row: RunRow
  answered: number | null
  onAnswer: (index: number) => void
}) {
  const { interactive } = useWeb()
  switch (row.kind) {
    case `narration`:
      return (
        <div className="web-feed-narr">
          <IcSparkles size={ICON_35} className="web-feed-lead" />
          <span>{row.text}</span>
        </div>
      )
    case `group`:
      return (
        <div className="web-feed-tool">
          <IcChevRight size={ICON_3} className="web-feed-chev" />
          <IcWrench size={ICON_3} />
          <span>{row.caption}</span>
        </div>
      )
    case `edit`:
      return (
        <div className="web-feed-edit">
          <IcWrench size={ICON_3} />
          <b>Edit</b>
          <code>{row.path}</code>
          <code className="is-muted">{`· ${row.detail}`}</code>
        </div>
      )
    case `user`:
      return (
        <div className="web-feed-user">
          <div className="web-feed-bubble">{row.text}</div>
        </div>
      )
    case `question`:
      return (
        <div className="web-feed-ask">
          <div className="web-feed-ask-q">
            <IcHelp size={ICON_4} className="web-feed-ask-icon" />
            <span>{row.text}</span>
          </div>
          <div className="web-feed-ask-opts">
            {row.options.map((opt, i) => (
              <button
                key={opt.title}
                type="button"
                disabled={answered !== null}
                className={`web-feed-opt${answered === i ? ` is-picked` : ``}${answered !== null && answered !== i ? ` is-dim` : ``}${interactive && answered === null ? ` is-click` : ``}`}
                onClick={interactive && answered === null ? () => onAnswer(i) : undefined}
              >
                <span className="web-feed-opt-main">
                  <span className="web-feed-opt-title">{opt.title}</span>
                  <span className="web-feed-opt-sub">{opt.sub}</span>
                </span>
                <span className="web-feed-opt-key">{i + 1}</span>
              </button>
            ))}
          </div>
        </div>
      )
  }
}

export function WebRunFace({ issueId }: { issueId: string }) {
  const { interactive } = useWeb()
  const session = sessionFor(issueId)
  const [draft, setDraft] = useState(``)
  const [sent, setSent] = useState<RunRow[]>([])
  const [answered, setAnswered] = useState<number | null>(null)
  if (!session) return null

  const feed = RUN_FEED[issueId] ?? []
  const working = session.state === `working` && answered === null
  const send = () => {
    const text = draft.trim()
    if (!text) return
    setSent((prev) => [...prev, { kind: `user`, text }])
    setDraft(``)
  }
  const answer = (index: number) => {
    setAnswered(index)
    const q = feed.find((r) => r.kind === `question`)
    if (q?.kind === `question`) {
      setSent((prev) => [...prev, { kind: `user`, text: q.options[index].title }])
    }
  }

  return (
    <div className="web-runface">
      <div className="web-feed-scroll">
        <div className="web-workcol web-feed">
          {feed.map((row, i) => (
            <FeedRow key={i} row={row} answered={answered} onAnswer={answer} />
          ))}
          {sent.map((row, i) => (
            <FeedRow key={`s${i}`} row={row} answered={answered} onAnswer={answer} />
          ))}
        </div>
      </div>
      <div className="web-steer">
        <div className="web-workcol">
          <div className="web-steer-card">
            <textarea
              className="web-steer-input"
              placeholder="Type / for commands"
              rows={1}
              value={draft}
              readOnly={!interactive}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={
                interactive
                  ? (e) => {
                      if (e.key === `Enter` && !e.shiftKey) {
                        e.preventDefault()
                        send()
                      }
                    }
                  : undefined
              }
            />
            {/* EXP-790: nothing typed while the agent works = Stop. */}
            {working && !draft.trim() ? (
              <span className="web-steer-stop" title="Stop">
                <IcStopSquare size={ICON_3} />
              </span>
            ) : (
              <button
                type="button"
                className={`web-send is-inline${interactive && draft.trim() ? ` is-click` : ``}`}
                disabled={!draft.trim()}
                onClick={interactive ? send : undefined}
                aria-label="Send"
              >
                <IcSubmit size={ICON_4 * 1.35} />
              </button>
            )}
          </div>
          <div className="web-steer-foot">
            <span className="web-editrail-btn is-sm" title="Attach image">
              <IcPlus size={ICON_35} />
            </span>
            <span className="web-steer-footright">
              <span className="web-steer-model">
                {session.model}
                {session.agent === `claude` && <IcChevDown size={ICON_3} />}
              </span>
              <ContextRing percent={session.contextPercent} />
            </span>
          </div>
        </div>
      </div>
    </div>
  )
}

export function WebDiffFace({ issueId }: { issueId: string }) {
  const { interactive } = useWeb()
  const [open, setOpen] = useState<Set<string>>(new Set([RUN_DIFF[0].path]))
  if (!sessionFor(issueId)) return null
  const toggle = (path: string) =>
    setOpen((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  return (
    <div className="web-detail-scroll web-diffface">
      <div className="web-workcol web-diff-col">
        {RUN_DIFF.map((file) => {
          const expanded = open.has(file.path) && !!file.lines
          const slash = file.path.lastIndexOf(`/`)
          return (
            <div className="web-diff-file" key={file.path}>
              <button
                type="button"
                className={`web-diff-head${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => toggle(file.path) : undefined}
              >
                <span className="web-diff-letter">M</span>
                <span className="web-diff-path">
                  {file.path.slice(0, slash + 1)}
                  <b>{file.path.slice(slash + 1)}</b>
                </span>
                <span className="web-difflabel">
                  <span className="is-add">+{file.additions}</span>{` `}
                  <span className="is-del">-{file.deletions}</span>
                </span>
                <IcChevDown size={ICON_35} className={`web-diff-chev${expanded ? ` is-open` : ``}`} />
              </button>
              {expanded && (
                <div className="web-diff-body">
                  <div className="web-diff-hunk">{file.hunk}</div>
                  {file.lines!.map((line, i) => (
                    <div className={`web-diff-line is-${line.kind}`} key={i}>
                      <span className="web-diff-n">{line.old ?? ``}</span>
                      <span className="web-diff-n">{line.new ?? ``}</span>
                      <span className="web-diff-sign">
                        {line.kind === `add` ? `+` : line.kind === `del` ? `-` : ``}
                      </span>
                      <span className="web-diff-text">{line.text}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

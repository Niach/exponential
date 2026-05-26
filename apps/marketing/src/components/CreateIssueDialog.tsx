import { useEffect, useState } from "react"
import { motion } from "motion/react"
import {
  CircleDashed,
  Minus,
  Tag,
  User,
  CalendarDays,
  ChevronRight,
  X,
} from "lucide-react"

export function CreateIssueDialog({
  open,
  onClose,
  onCreate,
  projectPrefix,
  projectColor,
}: {
  open: boolean
  onClose: () => void
  onCreate: (title: string) => void
  projectPrefix: string
  projectColor: string
}) {
  const [title, setTitle] = useState(``)
  const [createMore, setCreateMore] = useState(false)

  useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === `Escape`) onClose()
    }
    window.addEventListener(`keydown`, handler)
    return () => window.removeEventListener(`keydown`, handler)
  }, [open, onClose])

  const handleSubmit = () => {
    if (!title.trim()) return
    onCreate(title.trim())
    if (createMore) {
      setTitle(``)
    } else {
      setTitle(``)
      onClose()
    }
  }

  return (
    <motion.div
      className="ex-dialog-overlay"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      onClick={onClose}
    >
      <motion.div
        className="ex-dialog"
        initial={{ opacity: 0, scale: 0.96 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.96 }}
        transition={{ duration: 0.15 }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="ex-dialog-header">
          <span className="ex-dialog-project-pill">
            <span
              className="ex-dialog-project-dot"
              style={{ background: projectColor }}
            />
            {projectPrefix}
          </span>
          <ChevronRight size={13} strokeWidth={1.6} />
          <span>New issue</span>
          <span style={{ marginLeft: `auto`, cursor: `pointer` }} onClick={onClose}>
            <X size={14} strokeWidth={1.6} />
          </span>
        </div>

        <div className="ex-dialog-body">
          <input
            className="ex-dialog-title-input"
            placeholder="Issue title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === `Enter`) handleSubmit()
            }}
            autoFocus
          />
          <div className="ex-dialog-desc">Add a description...</div>
        </div>

        <div className="ex-dialog-chips">
          <span className="ex-dialog-chip">
            <CircleDashed size={13} strokeWidth={1.7} /> Backlog
          </span>
          <span className="ex-dialog-chip">
            <Minus size={13} strokeWidth={2} /> No priority
          </span>
          <span className="ex-dialog-chip">
            <Tag size={13} strokeWidth={1.7} />
          </span>
          <span className="ex-dialog-chip">
            <User size={13} strokeWidth={1.7} />
          </span>
          <span className="ex-dialog-chip">
            <CalendarDays size={13} strokeWidth={1.7} />
          </span>
        </div>

        <div className="ex-dialog-footer">
          <div className="ex-dialog-toggle-row">
            <div
              className={`ex-dialog-toggle ${createMore ? `is-on` : ``}`}
              onClick={() => setCreateMore(!createMore)}
            >
              <div className="ex-dialog-toggle-knob" />
            </div>
            <span>Create more</span>
          </div>
          <button
            className="ex-dialog-submit"
            disabled={!title.trim()}
            onClick={handleSubmit}
          >
            Create issue
          </button>
        </div>
      </motion.div>
    </motion.div>
  )
}

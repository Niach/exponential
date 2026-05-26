import { useEffect, useRef, useState } from "react"
import { motion } from "motion/react"
import { CircleDashed, Minus } from "lucide-react"

export function MobileCreateSheet({
  onClose,
  onCreate,
}: {
  onClose: () => void
  onCreate: (title: string) => void
}) {
  const [title, setTitle] = useState(``)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const handleCreate = () => {
    if (!title.trim()) return
    onCreate(title.trim())
    setTitle(``)
    onClose()
  }

  return (
    <>
      <motion.div
        className="m-create-overlay"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        onClick={onClose}
      />
      <motion.div
        className="m-create-sheet"
        initial={{ y: `100%` }}
        animate={{ y: 0 }}
        exit={{ y: `100%` }}
        transition={{ duration: 0.3, ease: [0.2, 0.8, 0.2, 1] }}
      >
        <div className="m-create-handle" />
        <div className="m-create-header">New issue</div>
        <input
          ref={inputRef}
          className="m-create-title"
          type="text"
          placeholder="Issue title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === `Enter`) handleCreate()
          }}
        />
        <div className="m-create-chips">
          <span className="m-create-chip">
            <CircleDashed size={12} strokeWidth={1.8} />
            Backlog
          </span>
          <span className="m-create-chip">
            <Minus size={12} strokeWidth={2} />
            No priority
          </span>
        </div>
        <button
          className="m-create-submit"
          disabled={!title.trim()}
          onClick={handleCreate}
        >
          Create issue
        </button>
      </motion.div>
    </>
  )
}

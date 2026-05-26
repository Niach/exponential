import { useEffect, useRef } from "react"
import { motion } from "motion/react"
import { Search, X } from "lucide-react"

export function MobileSearch({
  query,
  onChange,
  onClose,
}: {
  query: string
  onChange: (q: string) => void
  onClose: () => void
}) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  return (
    <motion.div
      className="m-search-bar"
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.2, ease: `easeOut` }}
    >
      <Search size={13} strokeWidth={2} style={{ color: `rgba(255,255,255,0.5)`, flexShrink: 0 }} />
      <input
        ref={inputRef}
        className="m-search-input"
        type="text"
        placeholder="Search issues..."
        value={query}
        onChange={(e) => onChange(e.target.value)}
      />
      <button className="m-search-close" onClick={onClose}>
        <X size={13} strokeWidth={2} />
      </button>
    </motion.div>
  )
}

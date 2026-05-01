import { useEffect, useState } from "react"
import { Input } from "@/components/ui/input"

interface TimeInputProps {
  value: string | null
  onChange: (value: string | null) => void
  disabled?: boolean
  className?: string
  ariaLabel?: string
}

/**
 * Compact HH:MM input. Lets the user type freely — "1337", "13:37",
 * "9:5" all become "13:37" / "09:05" on commit. Native <input type="time">
 * forces a segmented cursor and an OS-level picker that fights typed entry,
 * so we use a plain text field with a digit-only pattern and parse on
 * blur / Enter.
 */
export function TimeInput({
  value,
  onChange,
  disabled,
  className,
  ariaLabel,
}: TimeInputProps) {
  const initial = value ? value.slice(0, 5) : ``
  const [draft, setDraft] = useState(initial)

  useEffect(() => {
    setDraft(value ? value.slice(0, 5) : ``)
  }, [value])

  const commit = (raw: string) => {
    const trimmed = raw.trim()
    if (!trimmed) {
      setDraft(``)
      onChange(null)
      return
    }

    const digits = trimmed.replace(/\D/g, ``)
    if (digits.length < 3 || digits.length > 4) {
      setDraft(value ? value.slice(0, 5) : ``)
      return
    }

    const hh = digits.length === 3 ? digits.slice(0, 1) : digits.slice(0, 2)
    const mm = digits.length === 3 ? digits.slice(1) : digits.slice(2)
    const h = Number(hh)
    const m = Number(mm)
    if (h > 23 || m > 59) {
      setDraft(value ? value.slice(0, 5) : ``)
      return
    }

    const formatted = `${String(h).padStart(2, `0`)}:${String(m).padStart(2, `0`)}`
    setDraft(formatted)
    onChange(formatted)
  }

  return (
    <Input
      type="text"
      inputMode="numeric"
      placeholder="HH:MM"
      maxLength={5}
      value={draft}
      onChange={(e) => {
        // accept digits and a single colon, auto-insert colon after HH
        const raw = e.target.value.replace(/[^\d:]/g, ``)
        if (raw.length === 2 && draft.length === 1 && !raw.includes(`:`)) {
          setDraft(`${raw}:`)
          return
        }
        if (raw.length > 5) return
        setDraft(raw)
      }}
      onBlur={(e) => commit(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === `Enter`) {
          e.preventDefault()
          commit(e.currentTarget.value)
        }
      }}
      disabled={disabled}
      className={className}
      aria-label={ariaLabel}
    />
  )
}

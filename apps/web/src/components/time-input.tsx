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
 * "9:5" all become "13:37" / "09:05", and a bare "9" becomes "09:00" on
 * commit. Native <input type="time"> forces a segmented cursor and an
 * OS-level picker that fights typed entry, so we use a plain text field
 * with a digit-only pattern and parse on blur / Enter.
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

    const revert = () => setDraft(value ? value.slice(0, 5) : ``)

    let hh: string
    let mm: string
    if (trimmed.includes(`:`)) {
      // "9:5" → 09:05, "9:" → 09:00 — hours and minutes parse independently
      // (1-2 digits each), so we never need three digits before the colon.
      const [rawHours, rawMinutes = ``] = trimmed.split(`:`)
      hh = rawHours.replace(/\D/g, ``)
      const minutes = rawMinutes.replace(/\D/g, ``)
      if (hh.length < 1 || hh.length > 2 || minutes.length > 2) {
        revert()
        return
      }
      mm = minutes || `0`
    } else {
      // Colon-less entry: "1337" → 13:37, bare "9" → 09:00.
      const digits = trimmed.replace(/\D/g, ``)
      if (digits.length < 1 || digits.length > 4) {
        revert()
        return
      }
      if (digits.length <= 2) {
        hh = digits
        mm = `0`
      } else {
        hh = digits.length === 3 ? digits.slice(0, 1) : digits.slice(0, 2)
        mm = digits.slice(digits.length === 3 ? 1 : 2)
      }
    }

    const h = Number(hh)
    const m = Number(mm)
    if (h > 23 || m > 59) {
      revert()
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

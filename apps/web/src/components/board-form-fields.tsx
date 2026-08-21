import type { BoardIcon } from "@exp/db-schema/domain"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { IconPicker } from "@/components/ui/icon-picker"

// Shared, fully controlled field blocks for the create-board dialog and the
// per-board settings dialog (EXP-159/160). Presentation only — no tRPC in
// here: create saves everything on submit, edit mutates per change, so each
// surface owns its own persistence through these props.

// EXP-584: the icon picker sits LEFT of the name input (one row under the
// "Name" label) on every board form — create, edit and the onboarding
// wizard — mirrored on desktop/iOS/Android.
export function BoardNameField({
  value,
  onChange,
  onBlur,
  autoFocus,
  icon,
  onIconChange,
  color,
}: {
  value: string
  onChange: (value: string) => void
  onBlur?: () => void
  autoFocus?: boolean
  icon: BoardIcon
  onIconChange: (icon: BoardIcon) => void
  color: string
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor="board-name">Name</Label>
      <div className="flex items-center gap-2">
        <IconPicker
          value={icon}
          onChange={(next) => onIconChange(next as BoardIcon)}
          color={color}
        />
        <Input
          id="board-name"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={onBlur}
          placeholder="e.g. Backend API"
          autoFocus={autoFocus}
        />
      </div>
    </div>
  )
}

export function BoardPrefixField({
  value,
  onChange,
}: {
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor="board-prefix">Prefix</Label>
      <Input
        id="board-prefix"
        value={value}
        // Alphanumeric only — the server floor rejects symbol prefixes
        // (EXP-46).
        onChange={(e) =>
          onChange(e.target.value.replace(/[^A-Za-z0-9]/g, ``).toUpperCase())
        }
        placeholder="e.g. API"
        maxLength={4}
      />
    </div>
  )
}

export function BoardColorField({
  color,
  onColorChange,
}: {
  color: string
  onColorChange: (color: string) => void
}) {
  return (
    <div className="space-y-2">
      <Label>Color</Label>
      <ColorSwatchGrid value={color} onChange={onColorChange} />
    </div>
  )
}

import type { BoardIcon } from "@exp/db-schema/domain"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"

// Shared, fully controlled field blocks for the create-board dialog and the
// per-board settings dialog (EXP-159/160). Presentation only — no tRPC in
// here: create saves everything on submit, edit mutates per change, so each
// surface owns its own persistence through these props.

export function BoardNameField({
  value,
  onChange,
  onBlur,
  autoFocus,
}: {
  value: string
  onChange: (value: string) => void
  onBlur?: () => void
  autoFocus?: boolean
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor="board-name">Name</Label>
      <Input
        id="board-name"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        placeholder="e.g. Backend API"
        autoFocus={autoFocus}
      />
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
        maxLength={10}
      />
    </div>
  )
}

export function BoardIconColorFields({
  icon,
  onIconChange,
  color,
  onColorChange,
}: {
  icon: BoardIcon
  onIconChange: (icon: BoardIcon) => void
  color: string
  onColorChange: (color: string) => void
}) {
  return (
    <>
      <div className="space-y-2">
        <Label>Icon</Label>
        <IconSwatchGrid value={icon} onChange={onIconChange} color={color} />
      </div>
      <div className="space-y-2">
        <Label>Color</Label>
        <ColorSwatchGrid value={color} onChange={onColorChange} />
      </div>
    </>
  )
}

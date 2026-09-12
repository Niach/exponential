import type { BoardIcon } from "@exp/db-schema/domain"
import { Input } from "@/components/ui/input"
import { Pill } from "@/components/ui/pill"
import { ColorPicker } from "@/components/ui/color-picker"
import { IconPicker } from "@/components/ui/icon-picker"
import { GlassInputRow } from "@/components/ui/glass-rows"

// Shared, fully controlled field blocks for the create-board dialog, the
// per-board settings page and the onboarding wizard (EXP-159/160).
// Presentation only — no tRPC in here: create saves everything on submit,
// the settings page mutates per change, so each surface owns its own
// persistence through these props.
//
// EXP-862: every block is a ROW of the form's glass group — the label lives
// inside the row, never as a caption above it — and identity is ONE row:
// icon picker, colour picker, name, the two pickers being the same rounded
// square at the name field's height (×4 with desktop/iOS/Android).

// A field that draws no chrome of its own: the glass group around it IS the
// field (`action-editor-dialog`'s GROUPED_FIELD, restated here so the board
// form doesn't import a dialog for a class string).
const ROW_FIELD = `h-auto min-w-0 flex-1 rounded-none border-0 bg-transparent p-0 text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`

export function BoardIdentityRow({
  name,
  onNameChange,
  onNameBlur,
  autoFocus,
  icon,
  onIconChange,
  color,
  onColorChange,
  disabled,
}: {
  name: string
  onNameChange: (value: string) => void
  onNameBlur?: () => void
  autoFocus?: boolean
  icon: BoardIcon
  onIconChange: (icon: BoardIcon) => void
  color: string
  onColorChange: (color: string) => void
  disabled?: boolean
}) {
  return (
    <div className="flex items-center gap-2 px-4 py-3">
      <IconPicker
        id="board-icon"
        value={icon}
        onChange={(next) => onIconChange(next as BoardIcon)}
        color={color}
        disabled={disabled}
      />
      <ColorPicker
        id="board-color"
        value={color}
        onChange={onColorChange}
        disabled={disabled}
      />
      <Input
        id="board-name"
        aria-label="Name"
        value={name}
        onChange={(e) => onNameChange(e.target.value)}
        onBlur={onNameBlur}
        placeholder="Name"
        className={ROW_FIELD}
        autoFocus={autoFocus}
        disabled={disabled}
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
    <GlassInputRow
      id="board-prefix"
      label="Prefix"
      value={value}
      // Alphanumeric only — the server floor rejects symbol prefixes
      // (EXP-46).
      onChange={(e) =>
        onChange(e.target.value.replace(/[^A-Za-z0-9]/g, ``).toUpperCase())
      }
      placeholder="API"
      maxLength={4}
      inputClassName="font-mono uppercase"
    />
  )
}

// The minted prefix, read-only: identifiers are derived from it, so it can't
// change after creation.
export function BoardPrefixRow({ prefix }: { prefix: string }) {
  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <span className="shrink-0 text-sm text-foreground">Prefix</span>
      <span className="ml-auto">
        <Pill className="font-mono">{prefix}</Pill>
      </span>
    </div>
  )
}

import { Globe } from "lucide-react"
import type { ProjectIcon } from "@exp/db-schema/domain"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"

// The disable-with-explanation hint for the owner-only public option
// (projects.create/update reject isPublic from non-owners — EXP-133). Lives
// with the shared fields so both the create dialog and the settings edit
// dialog (EXP-159) speak the same copy.
export const OWNER_ONLY_PUBLIC_HINT = `Only team owners can create public boards.`

// Shared, fully controlled field blocks for the create-project dialog and the
// per-project settings dialog (EXP-159/160). Presentation only — no tRPC in
// here: create saves everything on submit, edit mutates per change, so each
// surface owns its own persistence through these props.

export function ProjectNameField({
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
      <Label htmlFor="project-name">Name</Label>
      <Input
        id="project-name"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        placeholder="e.g. Backend API"
        autoFocus={autoFocus}
      />
    </div>
  )
}

export function ProjectPrefixField({
  value,
  onChange,
}: {
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor="project-prefix">Prefix</Label>
      <Input
        id="project-prefix"
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

export function ProjectIconColorFields({
  icon,
  onIconChange,
  color,
  onColorChange,
}: {
  icon: ProjectIcon
  onIconChange: (icon: ProjectIcon) => void
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

export function ProjectPublicSection({
  checked,
  onCheckedChange,
  disabled,
  hint,
  showWarning,
}: {
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  disabled?: boolean
  // Explains a disabled switch (owner-only / protected project).
  hint?: string
  // The "readable by anyone" disclaimer; shown while the board is public.
  showWarning?: boolean
}) {
  return (
    <>
      <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
        <div className="min-w-0">
          <Label
            htmlFor="project-public"
            className="flex items-center gap-1.5 text-sm"
          >
            <Globe className="h-3.5 w-3.5 text-muted-foreground" />
            Public board
          </Label>
          <p className="text-xs text-muted-foreground">
            {hint ?? `Anyone with the link can read it.`}
          </p>
        </div>
        <Switch
          id="project-public"
          checked={checked}
          onCheckedChange={onCheckedChange}
          disabled={disabled}
        />
      </div>

      {showWarning && (
        <p className="rounded-md border border-border bg-accent/30 px-3 py-2 text-xs text-muted-foreground">
          Public boards are readable by anyone: issues, comments and @mentions
          in them are visible to anyone with the link. The workspace name is
          shown on the board.
        </p>
      )}
    </>
  )
}

import { shortcutLabel, VOICE_SHORTCUT_GRAMMAR } from "@exp/etalk/voice"

import { Kbd, KbdGroup } from "@exponential_app/eui-react/primitives/kbd"
import { cn } from "./cn"

// The keycap (the Exponential UI Kbd primitive) and S-3's push-to-talk
// binding drawn as keycaps (voice.json `shortcutLabel`, one cap per part;
// empty = `copy.shortcutNone` as muted words). The primitive paints the
// catalog's `accent` fill; the app's `--accent` is the opaque `#262626`, so
// the caps wear the glass active wash (`oklch(1 0 0 / 11%)` = the
// exponential theme's dark `accent` `#ffffff1c`, the IDE's `fill_active`).
export { Kbd, KbdGroup }

const KEYCAP_FILL = `bg-glass-active`

export function ShortcutKeys({ value, className }: { value: string; className?: string }) {
  const label = shortcutLabel(value)
  if (value === ``) return <span className={cn(`text-sm text-muted-foreground`, className)}>{label}</span>
  return (
    <KbdGroup aria-label={label} className={className}>
      {label.split(VOICE_SHORTCUT_GRAMMAR.separator).map((part, index) => (
        <Kbd key={`${index}-${part}`} className={KEYCAP_FILL}>
          {part}
        </Kbd>
      ))}
    </KbdGroup>
  )
}

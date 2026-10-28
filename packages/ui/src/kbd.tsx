import { shortcutLabel, VOICE_SHORTCUT_GRAMMAR } from "@exp/domain-contract/voice"

import { Kbd, KbdGroup } from "@exponential_app/ui-react/primitives/kbd"
import { cn } from "./cn"

// The keycap (the Exponential UI Kbd primitive) and S-3's push-to-talk
// binding drawn as keycaps (voice.json `shortcutLabel`, one cap per part;
// empty = `copy.shortcutNone` as muted words).
export { Kbd, KbdGroup }

export function ShortcutKeys({ value, className }: { value: string; className?: string }) {
  const label = shortcutLabel(value)
  if (value === ``) return <span className={cn(`text-sm text-muted-foreground`, className)}>{label}</span>
  return (
    <KbdGroup aria-label={label} className={className}>
      {label.split(VOICE_SHORTCUT_GRAMMAR.separator).map((part, index) => (
        <Kbd key={`${index}-${part}`}>{part}</Kbd>
      ))}
    </KbdGroup>
  )
}

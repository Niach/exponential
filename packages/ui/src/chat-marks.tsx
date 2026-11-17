import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS, type ChatMark } from "@exp/domain-contract/chats"
import { cn } from "./cn"
import { IconTooltip } from "./icon-tooltip"
import { conceptIcon } from "./icons.generated"

// #EDE-98 (chats.json `chatMarks`): the glyphs right after a chat's title,
// in the chat header and on every chat list row: the team icon when the
// chat is shared, the open lock when its tree has no key. Private and
// encrypted chats draw nothing.
const MARKS = {
  "ui-team": { Icon: conceptIcon(`ui-team`), label: CHATS_COPY.marks.team, testId: CHATS_TEST_IDS.chatMarkTeam },
  "ui-unencrypted": {
    Icon: conceptIcon(`ui-unencrypted`),
    label: CHATS_COPY.marks.unencrypted,
    testId: CHATS_TEST_IDS.chatMarkUnencrypted,
  },
} as const

export function ChatMarks({ marks, className }: { marks: readonly ChatMark[]; className?: string }) {
  if (marks.length === 0) return null
  return (
    <span className={cn(`flex shrink-0 items-center text-muted-foreground`, className)} style={{ gap: CHATS_SHELL.markGap }}>
      {marks.map((mark) => {
        const { Icon, label, testId } = MARKS[mark]
        return (
          <IconTooltip key={mark} label={label}>
            <Icon
              role="img"
              aria-label={label}
              data-testid={testId}
              style={{ width: CHATS_SHELL.markGlyph, height: CHATS_SHELL.markGlyph }}
            />
          </IconTooltip>
        )
      })}
    </span>
  )
}

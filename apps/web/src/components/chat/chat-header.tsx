// Chat 58cb32df (chats.json, the chat screen's header) / #EXP-1732
// (session-screen.json HEADER): a chat's lead avatar = a 40px DISC (the
// chat's lead: the run mark with the amber badge / the working mark / the
// PR glyph), drawn by THE session screen's header (`session-screen-frame.tsx`)
// beside the title over ONE subtitle line.

/** The disc's size on the header (the list's is `shell.leadDisc`). */
export const HEADER_DISC_CLASS = `!size-10 [&>svg]:size-5! [&>[data-slot=run-mark-badge]]:right-1.5! [&>[data-slot=run-mark-badge]]:top-1.5!`

/** The subtitle's tones: `needs input` amber, `working…` foreground, else
 *  muted (#EXP-1729: the app editor's `editorHeader` tones alike). */
export const CHAT_HEADER_TONE_CLASS = {
  attention: `text-yellow-400`,
  active: `text-foreground`,
  muted: `text-muted-foreground`,
} as const

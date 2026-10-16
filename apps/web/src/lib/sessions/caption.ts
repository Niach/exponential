// EXP-1331/EXP-1330: the pure text rules of the live status line
// (`sessions.caption`, varchar(80)) and a `/btw` aside's title.

export const CAPTION_MAX = 80
export const ASIDE_TITLE_MAX = 120

function capped(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text
}

/** One line, trimmed, capped; blank = cleared (null). */
export function normalizeCaption(caption: string | null): string | null {
  const line = caption?.replace(/\s+/g, ` `).trim() ?? ``
  return line ? capped(line, CAPTION_MAX) : null
}

/** An aside's title = the question's first line, capped. */
export function asideTitle(question: string): string {
  return capped(question.trim().split(`\n`)[0]!.trim(), ASIDE_TITLE_MAX)
}

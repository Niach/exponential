// Brand marks for the MCP setup tabs (EXP-141) and the agent pickers. The
// OpenAI/Cursor marks are simplified geometric approximations drawn as single
// currentColor fill paths on a 24×24 grid so they size and tint exactly like
// lucide icons (className="size-4") — a recognizable silhouette next to the
// client's name is all the tabs need.
//
// Claude is the exception (EXP-877): its REAL mark, in its own brand orange,
// everywhere it appears — the same path the desktop IDE, iOS and Android
// bundle (`apps/desktop/assets/icons/claude.svg`,
// `Assets.xcassets/agent-claude`, `drawable/ic_agent_claude.xml`), tinted
// `CLAUDE_FILL` on all four so "claude" looks the same on every client. The
// old 12-ray sunburst approximation read as a sun, not as Claude.

/** Anthropic's brand orange (#D97757). Never the text colour. */
export const CLAUDE_FILL = `hsl(14.8, 63.1%, 59.6%)`

export function ClaudeIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 100 100" fill={CLAUDE_FILL} aria-hidden="true" {...props}>
      <path d="m19.6 66.5 19.7-11 .3-1-.3-.5h-1l-3.3-.2-11.2-.3L14 53l-9.5-.5-2.4-.5L0 49l.2-1.5 2-1.3 2.9.2 6.3.5 9.5.6 6.9.4L38 49.1h1.6l.2-.7-.5-.4-.4-.4L29 41l-10.6-7-5.6-4.1-3-2-1.5-2-.6-4.2 2.7-3 3.7.3.9.2 3.7 2.9 8 6.1L37 36l1.5 1.2.6-.4.1-.3-.7-1.1L33 25l-6-10.4-2.7-4.3-.7-2.6c-.3-1-.4-2-.4-3l3-4.2L28 0l4.2.6L33.8 2l2.6 6 4.1 9.3L47 29.9l2 3.8 1 3.4.3 1h.7v-.5l.5-7.2 1-8.7 1-11.2.3-3.2 1.6-3.8 3-2L61 2.6l2 2.9-.3 1.8-1.1 7.7L59 27.1l-1.5 8.2h.9l1-1.1 4.1-5.4 6.9-8.6 3-3.5L77 13l2.3-1.8h4.3l3.1 4.7-1.4 4.9-4.4 5.6-3.7 4.7-5.3 7.1-3.2 5.7.3.4h.7l12-2.6 6.4-1.1 7.6-1.3 3.5 1.6.4 1.6-1.4 3.4-8.2 2-9.6 2-14.3 3.3-.2.1.2.3 6.4.6 2.8.2h6.8l12.6 1 3.3 2 1.9 2.7-.3 2-5.1 2.6-6.8-1.6-16-3.8-5.4-1.3h-.8v.4l4.6 4.5 8.3 7.5L89 80.1l.5 2.4-1.3 2-1.4-.2-9.2-7-3.6-3-8-6.8h-.5v.7l1.8 2.7 9.8 14.7.5 4.5-.7 1.4-2.6 1-2.7-.6-5.8-8-6-9-4.7-8.2-.5.4-2.9 30.2-1.3 1.5-3 1.2-2.5-2-1.4-3 1.4-6.2 1.6-8 1.3-6.4 1.2-7.9.7-2.6v-.2H49L43 72l-9 12.3-7.2 7.6-1.7.7-3-1.5.3-2.8L24 86l10-12.8 6-7.9 4-4.6-.1-.5h-.3L17.2 77.4l-4.7.6-2-2 .2-3 1-1 8-5.5Z" />
    </svg>
  )
}

export function OpenAiIcon(props: React.SVGProps<SVGSVGElement>) {
  // The hexagonal knot: six interlocking thick "L" legs, 60° apart.
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M13.1 9.1L13.1 4.41L19.68 8.2L20.78 6.3L10.9 0.59L10.9 9.1ZM15.06 11.5L19.13 9.16L19.13 16.75L21.33 16.75L21.33 5.34L13.96 9.6ZM13.96 14.4L18.03 16.75L11.45 20.55L12.55 22.45L22.43 16.75L15.06 12.5ZM10.9 14.9L10.9 19.59L4.32 15.8L3.22 17.7L13.1 23.41L13.1 14.9ZM8.94 12.5L4.87 14.84L4.87 7.25L2.67 7.25L2.67 18.66L10.04 14.4ZM10.04 9.6L5.97 7.25L12.55 3.45L11.45 1.55L1.57 7.25L8.94 11.5Z" />
    </svg>
  )
}

export function CodexIcon(props: React.SVGProps<SVGSVGElement>) {
  // OpenAI Codex CLI mark (same path the desktop bundles as codex.svg) —
  // agent tab strips (EXP-213).
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z" />
    </svg>
  )
}

export function CursorIcon(props: React.SVGProps<SVGSVGElement>) {
  // The angular 3D cube: an isometric hexagon split into three faces.
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M12 3.55L19.25 7.5L12 11.45L4.75 7.5ZM4.48 7.98L11.52 12.27L11.72 20.52L4.68 16.23ZM12.48 12.27L19.52 7.98L19.32 16.23L12.28 20.52Z" />
    </svg>
  )
}

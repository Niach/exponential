// Monochrome brand marks for the MCP setup tabs (EXP-141). Simplified
// geometric approximations of the real logos, drawn as single currentColor
// fill paths on a 24×24 grid so they size and tint exactly like lucide icons
// (className="size-4"). Deliberately NOT the trademarked vector art — a
// recognizable silhouette next to the client's name is all the tabs need.

export function ClaudeIcon(props: React.SVGProps<SVGSVGElement>) {
  // Anthropic's radiating-starburst mark, simplified to 12 uniform rays.
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M15 13.05L23 12.55L23 11.45L15 10.95ZM14.07 14.41L21.25 17.98L21.8 17.02L15.12 12.59ZM12.59 15.12L17.02 21.8L17.98 21.25L14.41 14.07ZM10.95 15L11.45 23L12.55 23L13.05 15ZM9.59 14.07L6.02 21.25L6.98 21.8L11.41 15.12ZM8.88 12.59L2.2 17.02L2.75 17.98L9.93 14.41ZM9 10.95L1 11.45L1 12.55L9 13.05ZM9.93 9.59L2.75 6.02L2.2 6.98L8.88 11.41ZM11.41 8.88L6.98 2.2L6.02 2.75L9.59 9.93ZM13.05 9L12.55 1L11.45 1L10.95 9ZM14.41 9.93L17.98 2.75L17.02 2.2L12.59 8.88ZM15.12 11.41L21.8 6.98L21.25 6.02L14.07 9.59Z" />
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

export function PiIcon(props: React.SVGProps<SVGSVGElement>) {
  // pi.dev's blocky "P + i-dot" mark (the desktop's pi.svg, currentColor) —
  // agent tab strips (EXP-213).
  return (
    <svg
      viewBox="0 0 800 800"
      fill="currentColor"
      aria-hidden="true"
      {...props}
    >
      <path
        fillRule="evenodd"
        d="M165.29 165.29 H517.36 V400 H400 V517.36 H282.65 V634.72 H165.29 Z M282.65 282.65 V400 H400 V282.65 Z"
      />
      <path d="M517.36 400 H634.72 V634.72 H517.36 Z" />
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

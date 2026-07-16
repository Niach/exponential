// Monochrome brand marks for the MCP setup tabs (EXP-141). Simplified
// geometric approximations of the real logos, drawn as single currentColor
// fill paths on a 24×24 grid so they size and tint exactly like lucide icons
// (className="size-4"). Deliberately NOT the trademarked vector art — a
// recognizable silhouette next to the client's name is all the tabs need.

export function ClaudeIcon(props: React.SVGProps<SVGSVGElement>) {
  // Anthropic's radiating-starburst mark, simplified to 12 uniform rays.
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      {...props}
    >
      <path d="M15 13.05L23 12.55L23 11.45L15 10.95ZM14.07 14.41L21.25 17.98L21.8 17.02L15.12 12.59ZM12.59 15.12L17.02 21.8L17.98 21.25L14.41 14.07ZM10.95 15L11.45 23L12.55 23L13.05 15ZM9.59 14.07L6.02 21.25L6.98 21.8L11.41 15.12ZM8.88 12.59L2.2 17.02L2.75 17.98L9.93 14.41ZM9 10.95L1 11.45L1 12.55L9 13.05ZM9.93 9.59L2.75 6.02L2.2 6.98L8.88 11.41ZM11.41 8.88L6.98 2.2L6.02 2.75L9.59 9.93ZM13.05 9L12.55 1L11.45 1L10.95 9ZM14.41 9.93L17.98 2.75L17.02 2.2L12.59 8.88ZM15.12 11.41L21.8 6.98L21.25 6.02L14.07 9.59Z" />
    </svg>
  )
}

export function OpenAiIcon(props: React.SVGProps<SVGSVGElement>) {
  // The hexagonal knot: six interlocking thick "L" legs, 60° apart.
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      {...props}
    >
      <path d="M13.1 9.1L13.1 4.41L19.68 8.2L20.78 6.3L10.9 0.59L10.9 9.1ZM15.06 11.5L19.13 9.16L19.13 16.75L21.33 16.75L21.33 5.34L13.96 9.6ZM13.96 14.4L18.03 16.75L11.45 20.55L12.55 22.45L22.43 16.75L15.06 12.5ZM10.9 14.9L10.9 19.59L4.32 15.8L3.22 17.7L13.1 23.41L13.1 14.9ZM8.94 12.5L4.87 14.84L4.87 7.25L2.67 7.25L2.67 18.66L10.04 14.4ZM10.04 9.6L5.97 7.25L12.55 3.45L11.45 1.55L1.57 7.25L8.94 11.5Z" />
    </svg>
  )
}

export function CursorIcon(props: React.SVGProps<SVGSVGElement>) {
  // The angular 3D cube: an isometric hexagon split into three faces.
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      {...props}
    >
      <path d="M12 3.55L19.25 7.5L12 11.45L4.75 7.5ZM4.48 7.98L11.52 12.27L11.72 20.52L4.68 16.23ZM12.48 12.27L19.52 7.98L19.32 16.23L12.28 20.52Z" />
    </svg>
  )
}

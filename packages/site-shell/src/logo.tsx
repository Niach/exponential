/* The Exponential brand mark (moved from apps/marketing so every public site
   wears the same one). `size` drives the mask ids, so two sizes on one page
   never collide. */
import type { CSSProperties } from "react"

export const ExpLogo = ({
  size = 22,
  color,
  style,
}: {
  size?: number
  color?: string
  style?: CSSProperties
}) => {
  const id = `exp-${size}`
  return (
    <svg
      viewBox="0 0 100 100"
      width={size}
      height={size}
      style={{ display: `block`, ...style }}
      aria-hidden
    >
      <defs>
        <clipPath id={`${id}-c`}>
          <circle cx="50" cy="50" r="50" />
        </clipPath>
        <mask id={`${id}-m`}>
          <rect width="100" height="100" fill="white" />
          <g clipPath={`url(#${id}-c)`}>
            <path
              d="M -5.87 62.01 C 39.09 65.44 48.72 28.71 49.03 -6.21"
              stroke="black"
              strokeWidth="6"
              fill="none"
            />
            <path
              d="M -5.07 86.00 C 53.78 84.42 71.13 37.29 73.00 -5.09"
              stroke="black"
              strokeWidth="6"
              fill="none"
            />
            <path
              d="M -4.27 109.99 C 68.46 103.40 93.55 45.86 96.98 -3.98"
              stroke="black"
              strokeWidth="6"
              fill="none"
            />
          </g>
        </mask>
      </defs>
      <circle
        cx="50"
        cy="50"
        r="50"
        fill={color || `currentColor`}
        mask={`url(#${id}-m)`}
      />
    </svg>
  )
}

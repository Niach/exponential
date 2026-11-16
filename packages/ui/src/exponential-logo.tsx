import { useId } from "react"

interface ExponentialLogoProps {
  variant?: `dark` | `light`
  size?: number
  className?: string
  /** #EXP-1876: the disc wears the glassy sheen (`--sheen-from` →
   *  `--sheen-to`, top → bottom), THE mark button's look (`.fab-mark`). */
  sheen?: boolean
}

export function ExponentialLogo({
  variant = `dark`,
  size = 24,
  className,
  sheen = false,
}: ExponentialLogoProps) {
  const id = useId()
  const clipId = `clip-${id}`
  const maskId = `mask-${id}`
  const sheenId = `sheen-${id}`
  const fill = sheen ? `url(#${sheenId})` : variant === `dark` ? `#222326` : `currentColor`

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
    >
      <defs>
        {sheen && (
          <linearGradient id={sheenId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" style={{ stopColor: `var(--sheen-from)` }} />
            <stop offset="1" style={{ stopColor: `var(--sheen-to)` }} />
          </linearGradient>
        )}
        <clipPath id={clipId}>
          <circle cx="50" cy="50" r="50" />
        </clipPath>
        <mask id={maskId}>
          <rect width="100" height="100" fill="white" />
          <g clipPath={`url(#${clipId})`}>
            <path
              d="M -5.87 62.01 C 39.09 65.44 48.72 28.71 49.03 -6.21"
              stroke="black"
              strokeWidth="3.5"
              fill="none"
            />
            <path
              d="M -5.07 86.00 C 53.78 84.42 71.13 37.29 73.00 -5.09"
              stroke="black"
              strokeWidth="3.5"
              fill="none"
            />
            <path
              d="M -4.27 109.99 C 68.46 103.40 93.55 45.86 96.98 -3.98"
              stroke="black"
              strokeWidth="3.5"
              fill="none"
            />
          </g>
        </mask>
      </defs>
      <circle cx="50" cy="50" r="50" fill={fill} mask={`url(#${maskId})`} />
    </svg>
  )
}

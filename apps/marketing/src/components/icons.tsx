import type { CSSProperties } from "react"
import {
  ArrowRight,
  Calendar as CalendarIcon,
  ChevronDown,
  ChevronRight,
  ChevronsUpDown,
  Container,
  Copy,
  Filter,
  Github,
  PanelLeft,
  Paperclip,
  Plus,
  Server,
  Shield,
  Tag,
  User as UserIcon,
  Zap,
  type LucideProps,
} from "lucide-react"

export type IconProps = {
  size?: number
  stroke?: number
  className?: string
  style?: CSSProperties
}

const wrap = (Cmp: React.ComponentType<LucideProps>) =>
  function WrappedIcon({ size = 16, stroke = 1.6, className, style }: IconProps) {
    return (
      <Cmp
        size={size}
        strokeWidth={stroke}
        className={className}
        style={style}
      />
    )
  }

export const IcGithub = wrap(Github)
export const IcDocker = wrap(Container)
export const IcArrow = wrap(ArrowRight)
export const IcCopy = wrap(Copy)
export const IcShield = wrap(Shield)
export const IcZap = wrap(Zap)
export const IcServer = wrap(Server)
export const IcFilter = wrap(Filter)
export const IcPlus = wrap(Plus)
export const IcCal = wrap(CalendarIcon)
export const IcUser = wrap(UserIcon)
export const IcTag = wrap(Tag)
export const IcAttach = wrap(Paperclip)
export const IcChev = wrap(ChevronRight)
export const IcChevDown = wrap(ChevronDown)
export const IcChevSwap = wrap(ChevronsUpDown)
export const IcSidebar = wrap(PanelLeft)

const Custom = ({
  size = 16,
  stroke = 1.6,
  className,
  style,
  children,
}: IconProps & { children: React.ReactNode }) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={stroke}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    style={style}
  >
    {children}
  </svg>
)

export const IcCircle = (p: IconProps) => (
  <Custom {...p}>
    <circle cx="12" cy="12" r="9" />
  </Custom>
)
export const IcCircleDashed = (p: IconProps) => (
  <Custom {...p}>
    <circle cx="12" cy="12" r="9" strokeDasharray="2 3" />
  </Custom>
)
export const IcHalf = (p: IconProps) => (
  <Custom {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 3 a9 9 0 0 1 0 18 z" fill="currentColor" />
  </Custom>
)
export const IcCheck = (p: IconProps) => (
  <Custom {...p}>
    <circle cx="12" cy="12" r="9" fill="currentColor" stroke="none" />
    <polyline
      points="8 12 11 15 16 9"
      stroke="var(--bg-card)"
      strokeWidth={2}
      fill="none"
    />
  </Custom>
)
export const IcX = (p: IconProps) => (
  <Custom {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M9 9 L15 15 M15 9 L9 15" />
  </Custom>
)
export const IcViewsEmpty = (p: IconProps) => (
  <Custom {...p}>
    <rect x="3" y="4" width="18" height="4" rx="1" />
    <rect x="3" y="11" width="11" height="3" rx="1" />
    <rect x="3" y="17" width="14" height="3" rx="1" />
  </Custom>
)

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

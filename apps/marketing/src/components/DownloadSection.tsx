import type { ReactNode } from "react"
import { LINKS } from "../lib/links"

/* ─── Platform logos (inline SVG, currentColor) ───────────────────────── */

export function AppleLogo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      <path d="M16.365 1.43c0 1.14-.417 2.2-1.11 2.98-.744.83-1.964 1.47-3.02 1.39-.13-1.1.42-2.26 1.06-2.99.72-.82 1.99-1.44 3.07-1.38zM20.5 17.02c-.55 1.28-.82 1.85-1.53 2.98-.99 1.58-2.39 3.55-4.12 3.56-1.54.02-1.94-1.01-4.03-1-2.09.01-2.53 1.02-4.07 1-1.73-.02-3.05-1.8-4.04-3.38C.02 16.72-.28 11.4 1.42 8.58c1.2-2 3.1-3.17 4.88-3.17 1.82 0 2.96 1.01 4.46 1.01 1.46 0 2.35-1.01 4.46-1.01 1.6 0 3.29.87 4.5 2.38-3.95 2.17-3.31 7.82.28 9.23z" />
    </svg>
  )
}

export function LinuxLogo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      {/* Tux-ish penguin silhouette */}
      <path d="M12 2c-2.1 0-3.4 1.7-3.4 3.9 0 1 .1 1.9.1 2.7 0 .7-.4 1.3-1 2.2-.9 1.3-2.1 2.9-2.9 4.6-.4.8-.6 1.5-.4 2.1.1.4.4.7.8.8-.1.4-.1.8 0 1.2.2.6.8 1 1.6 1.1.5.7 1.4 1.1 2.5 1.2h4.6c1.1-.1 2-.5 2.5-1.2.8-.1 1.4-.5 1.6-1.1.1-.4.1-.8 0-1.2.4-.1.7-.4.8-.8.2-.6 0-1.3-.4-2.1-.8-1.7-2-3.3-2.9-4.6-.6-.9-1-1.5-1-2.2 0-.8.1-1.7.1-2.7C15.4 3.7 14.1 2 12 2zm-1.5 4.1c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9zm3 0c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9zm-1.5 2.6c.7 0 1.6.5 1.6 1 0 .3-.4.5-.8.7-.3.2-.6.4-.8.4s-.5-.2-.8-.4c-.4-.2-.8-.4-.8-.7 0-.5.9-1 1.6-1z" />
    </svg>
  )
}

export function WindowsLogo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      {/* Four-pane Windows flag */}
      <path d="M3 5.55 10.62 4.5v6.98H3V5.55zm0 12.9 7.62 1.05v-6.9H3v5.85zM11.46 19.62 21.5 21V12.6H11.46v7.02zm0-15.24v7.1H21.5V3L11.46 4.38z" />
    </svg>
  )
}

export function AppStoreLogo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      <path d="M12 2c5.52 0 10 4.48 10 10s-4.48 10-10 10S2 17.52 2 12 6.48 2 12 2zm-1.35 4.6-.6 1.04-.6-1.04a.5.5 0 0 0-.87.5l.83 1.44-2.02 3.5H5.6a.5.5 0 0 0 0 1h7.02l-.58-1H8.37l3.15-5.44.58 1 .58-1 .3.52.58-1-.31-.52a.5.5 0 0 0-.86 0zm4.32 5.9-.58-1-.58 1 1.6 2.77h-1.53l.58 1h1.53l.42.72a.5.5 0 1 0 .87-.5l-.42-.72h.87a.5.5 0 0 0 0-1h-1.45l-1.3-2.27zM7.6 15.17l-.4.7a.5.5 0 0 0 .86.5l.98-1.7h-1.15l-.29.5z" />
    </svg>
  )
}

export function GlobeLogo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M3.2 12h17.6" />
      <path d="M12 3a13.4 13.4 0 0 1 0 18 13.4 13.4 0 0 1 0-18Z" />
    </svg>
  )
}

export function AndroidLogo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      <path d="M17.6 9.48l1.84-3.18a.4.4 0 0 0-.7-.4l-1.87 3.23a11.4 11.4 0 0 0-9.74 0L5.26 5.9a.4.4 0 1 0-.7.4L6.4 9.48A10.8 10.8 0 0 0 1 18.2h22a10.8 10.8 0 0 0-5.4-8.72zM7 15.25a1 1 0 1 1 0-2 1 1 0 0 1 0 2zm10 0a1 1 0 1 1 0-2 1 1 0 0 1 0 2z" />
    </svg>
  )
}

/* ─── Platform data (reused by DownloadPage + the hero icon row) ───────── */

export type Platform = {
  id: string
  name: string
  logo: (p: { size?: number }) => ReactNode
  requirement: string
  cta: string
  href: string
  soon?: boolean
}

export const PLATFORMS: Platform[] = [
  {
    id: `macos`,
    name: `macOS`,
    logo: AppleLogo,
    requirement: `Apple Silicon · macOS 13+`,
    cta: `Download for Mac`,
    href: LINKS.downloads.macos,
  },
  {
    id: `windows`,
    name: `Windows`,
    logo: WindowsLogo,
    requirement: `x86_64 · portable .exe`,
    cta: `Download for Windows`,
    href: LINKS.downloads.windows,
  },
  {
    id: `linux`,
    name: `Linux`,
    logo: LinuxLogo,
    requirement: `x86_64 AppImage`,
    cta: `Download AppImage`,
    href: LINKS.downloads.linux,
  },
  {
    id: `ios`,
    name: `iOS`,
    logo: AppStoreLogo,
    requirement: `Public beta · TestFlight`,
    cta: `Join the TestFlight beta`,
    href: LINKS.downloads.ios,
  },
  {
    id: `android`,
    name: `Android`,
    logo: AndroidLogo,
    requirement: `Open beta · Google Play`,
    cta: `Join the Play Store beta`,
    href: LINKS.downloads.android,
  },
]

/* ─── Compact hero icon row ────────────────────────────────────────────── */

export function DownloadIconRow() {
  return (
    <div className="dl-iconrow">
      {/* The web app leads the row (EXP-217) — no download, straight to the
          merged login/signup page. */}
      <a
        className={`dl-iconbtn`}
        href={LINKS.app.login}
        title={`Open the web app`}
      >
        <GlobeLogo size={16} />
        <span>Web</span>
      </a>
      {PLATFORMS.map((p) => {
        const Logo = p.logo
        // Desktop links go straight to the asset; mobile routes to /download/.
        const href = p.soon ? LINKS.downloadPage : p.href
        return (
          <a
            key={p.id}
            className={`dl-iconbtn${p.soon ? ` is-soon` : ``}`}
            href={href}
            title={p.soon ? p.cta : p.requirement}
          >
            <Logo size={16} />
            <span>{p.name}</span>
          </a>
        )
      })}
    </div>
  )
}

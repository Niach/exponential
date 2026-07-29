/* The hero CTA row (EXP-337, orca-style downloads): an OS-detected accent
   download button with a platform-switcher popover, a ghost "Get started
   free", and "View on GitHub" with the star badge.

   SSR contract: detection runs in useEffect only — the prerendered markup
   is the neutral "Download the app" → /download/ state and the popover is
   closed, so the first client render matches byte-for-byte. */
import { useEffect, useRef, useState } from "react"
import { LINKS } from "../lib/links"
import { GlobeLogo, PLATFORMS, type Platform } from "./DownloadSection"
import { GitHubStarsButton } from "./GitHubStarsButton"
import { IcChevDown, IcDownload } from "./icons"

type PlatformId = `macos` | `windows` | `linux` | `ios` | `android`

const detectPlatform = (): PlatformId | null => {
  const nav = navigator as Navigator & {
    userAgentData?: { platform?: string }
  }
  const p = (nav.userAgentData?.platform ?? nav.platform ?? ``).toLowerCase()
  const ua = navigator.userAgent.toLowerCase()
  if (/iphone|ipad|ipod/.test(p) || /iphone|ipad|ipod/.test(ua)) return `ios`
  if (/android/.test(p) || /android/.test(ua)) return `android`
  // iPadOS reports MacIntel with touch — treat touch Macs as iOS.
  if (/mac/.test(p)) return navigator.maxTouchPoints > 1 ? `ios` : `macos`
  if (/win/.test(p)) return `windows`
  if (/linux/.test(p)) return `linux`
  return null
}

export function HeroDownload() {
  const [platform, setPlatform] = useState<Platform | null>(null)
  const [open, setOpen] = useState(false)
  const wrapRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const id = detectPlatform()
    if (!id) return
    const match = PLATFORMS.find((p) => p.id === id)
    if (match) setPlatform(match)
  }, [])

  /* Close the switcher on outside click / Escape. */
  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === `Escape`) setOpen(false)
    }
    document.addEventListener(`mousedown`, onDown)
    document.addEventListener(`keydown`, onKey)
    return () => {
      document.removeEventListener(`mousedown`, onDown)
      document.removeEventListener(`keydown`, onKey)
    }
  }, [open])

  const Logo = platform?.logo

  return (
    <div className={`hero-dl`} ref={wrapRef}>
      <span className={`hero-dl-split`}>
        <a
          className={`btn btn-accent hero-dl-main`}
          href={platform ? platform.href : LINKS.downloadPage}
        >
          {Logo ? <Logo size={15} /> : <IcDownload size={15} stroke={2} />}
          {platform ? platform.cta : `Download the app`}
        </a>
        <button
          type={`button`}
          className={`btn btn-accent hero-dl-more`}
          aria-label={`Other platforms`}
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          <IcChevDown size={14} stroke={2.2} />
        </button>
      </span>

      <a className={`btn btn-ghost`} href={LINKS.app.login}>
        Get started free
      </a>

      <GitHubStarsButton variant={`hero`} />

      {open ? (
        <div className={`glass-panel hero-dl-menu`}>
          <a className={`hero-dl-item`} href={LINKS.app.login}>
            <GlobeLogo size={16} />
            <span className={`hero-dl-item-name`}>Web</span>
            <span className={`hero-dl-item-req`}>app.exponential.at</span>
          </a>
          {PLATFORMS.map((p) => {
            const ItemLogo = p.logo
            return (
              <a key={p.id} className={`hero-dl-item`} href={p.href}>
                <ItemLogo size={16} />
                <span className={`hero-dl-item-name`}>{p.name}</span>
                <span className={`hero-dl-item-req`}>{p.requirement}</span>
              </a>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}

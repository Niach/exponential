/* GitHub star count for the repo badge (EXP-337). SSR-safe by construction:
   the hook's initial state is null (SSR markup === first client render, no
   hydration mismatch) and the fetch runs in useEffect only. Unauthenticated
   GitHub API is 60 req/h/IP, so the count is cached per tab (sessionStorage,
   1h TTL) and deduped across mounts via a module-level promise. Any failure
   just leaves the badge count-less. */
import { useEffect, useState } from "react"
import { LINKS } from "./links"

const CACHE_KEY = `exp:gh-stars`
const TTL_MS = 60 * 60 * 1000

let pending: Promise<number | null> | null = null

const readCache = (): number | null => {
  try {
    const raw = sessionStorage.getItem(CACHE_KEY)
    if (!raw) return null
    const { value, at } = JSON.parse(raw) as { value: number; at: number }
    if (typeof value !== `number` || Date.now() - at > TTL_MS) return null
    return value
  } catch {
    return null
  }
}

const fetchStars = (): Promise<number | null> => {
  if (pending) return pending
  pending = (async () => {
    const cached = readCache()
    if (cached !== null) return cached
    try {
      const repoPath = new URL(LINKS.github.repo).pathname.replace(/^\//, ``)
      const res = await fetch(`https://api.github.com/repos/${repoPath}`)
      if (!res.ok) return null
      const data = (await res.json()) as { stargazers_count?: number }
      const count = data.stargazers_count
      if (typeof count !== `number`) return null
      try {
        sessionStorage.setItem(
          CACHE_KEY,
          JSON.stringify({ value: count, at: Date.now() })
        )
      } catch {
        /* private mode etc. — cache is best-effort */
      }
      return count
    } catch {
      return null
    }
  })()
  return pending
}

export function useGitHubStars(): number | null {
  const [stars, setStars] = useState<number | null>(null)
  useEffect(() => {
    let alive = true
    void fetchStars().then((count) => {
      if (alive && count !== null) setStars(count)
    })
    return () => {
      alive = false
    }
  }, [])
  return stars
}

/* Deterministic short format ("1.2k") — no Intl, so the Bun prerender and
   every browser agree byte-for-byte (not that SSR ever renders a count). */
export const formatStars = (n: number): string =>
  n >= 1000 ? `${(Math.floor(n / 100) / 10).toFixed(1)}k` : String(n)

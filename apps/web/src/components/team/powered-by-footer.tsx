// Small branding footer shown on the public reporter-facing support-thread
// view (routes/support/$token) — the one surface strangers still see, so it
// doubles as a signpost back to the product. The imprint link is a legal
// requirement (§5 TMG/DDG): the page is publicly reachable, so the imprint
// must be reachable from it (EXP-40).
export function PoweredByFooter() {
  return (
    <footer className="border-t border-border/60 py-4 text-center">
      <a
        href="https://exponential.at/?utm_source=powered-by"
        target="_blank"
        rel="noreferrer"
        className="text-xs text-muted-foreground hover:text-foreground transition-colors"
      >
        Powered by <span className="font-semibold">Exponential</span> — issue
        tracking with local AI agents
      </a>
      <span className="mx-2 text-xs text-muted-foreground/60">·</span>
      <a
        href="https://exponential.at/imprint/"
        target="_blank"
        rel="noreferrer"
        className="text-xs text-muted-foreground hover:text-foreground transition-colors"
      >
        Imprint
      </a>
    </footer>
  )
}

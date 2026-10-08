/* Small building blocks of the content pages (home, concepts, guides,
   conformance), on the shared docs styles of @exp/site-shell. */
import type { ReactNode } from "react"
import { ArrowRight, ChevronRight } from "lucide-react"

export const IcArrow = ({ size = 12 }: { size?: number }) => <ArrowRight size={size} strokeWidth={1.8} aria-hidden />
export const IcChev = ({ size = 13 }: { size?: number }) => <ChevronRight size={size} strokeWidth={1.8} aria-hidden />

/** The hero above a docs-style page. */
export function PageHero({ eyebrow, title, children, cta }: { eyebrow?: string; title: string; children: ReactNode; cta?: ReactNode }) {
  return (
    <section className="docs-hero content-hero">
      <div className="shell docs-hero-content">
        {eyebrow && <div className="section-eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        <p>{children}</p>
        {cta && <div className="docs-hero-cta">{cta}</div>}
      </div>
    </section>
  )
}

/** A grid of linked cards (the docs hub look). */
export function CardGrid({ items }: { items: { href: string; title: string; desc: string }[] }) {
  return (
    <div className="docs-cards">
      {items.map((item) => (
        <a key={item.href} className="docs-card" href={item.href}>
          <span className="docs-card-title">
            {item.title} <IcChev />
          </span>
          <span className="docs-card-desc">{item.desc}</span>
        </a>
      ))}
    </div>
  )
}

/** A plain data table in the docs column. */
export function DocsTable({ head, rows, className }: { head: ReactNode[]; rows: ReactNode[][]; className?: string }) {
  return (
    <div className={`content-table-wrap${className ? ` ${className}` : ``}`}>
      <table className="content-table">
        <thead>
          <tr>
            {head.map((h, i) => (
              <th key={i}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, r) => (
            <tr key={r}>
              {row.map((cell, c) => (
                <td key={c}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

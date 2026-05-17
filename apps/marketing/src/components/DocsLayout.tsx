import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { IcCheck, IcCopy } from "./icons"

export type DocsSection = {
  id: string
  num: string
  label: string
}

export function DocsLayout({
  sections,
  children,
}: {
  sections: DocsSection[]
  children: ReactNode
}) {
  const [activeId, setActiveId] = useState<string>(sections[0]?.id ?? ``)

  useEffect(() => {
    if (typeof window === `undefined`) return
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((e) => e.isIntersecting)
        if (visible.length === 0) return
        const top = visible.sort(
          (a, b) => a.boundingClientRect.top - b.boundingClientRect.top
        )[0]
        const id = top.target.getAttribute(`data-docs-section`)
        if (id) setActiveId(id)
      },
      { rootMargin: `-90px 0px -55% 0px`, threshold: 0 }
    )
    document
      .querySelectorAll(`[data-docs-section]`)
      .forEach((el) => observer.observe(el))
    return () => observer.disconnect()
  }, [])

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLAnchorElement>, id: string) => {
      e.preventDefault()
      const el = document.getElementById(id)
      if (!el) return
      el.scrollIntoView({ behavior: `smooth`, block: `start` })
      history.replaceState(null, ``, `#${id}`)
      setActiveId(id)
    },
    []
  )

  return (
    <div className="shell docs-layout">
      <aside className="docs-sidebar">
        <nav className="docs-nav" aria-label="Docs sections">
          <span className="docs-nav-title">On this page</span>
          {sections.map((s) => (
            <a
              key={s.id}
              href={`#${s.id}`}
              className={activeId === s.id ? `is-active` : ``}
              onClick={(e) => handleClick(e, s.id)}
            >
              <span className="docs-nav-num">{s.num}</span>
              <span>{s.label}</span>
            </a>
          ))}
        </nav>
      </aside>
      <article className="docs-content">{children}</article>
    </div>
  )
}

export function DocsSection({
  id,
  num,
  label,
  children,
}: {
  id: string
  num: string
  label: string
  children: ReactNode
}) {
  return (
    <section id={id} data-docs-section={id}>
      <div className="docs-section-tag">
        <span className="num">{num}</span>
        <span className="line" />
        <span className="num docs-section-label">{label}</span>
        <span className="line" />
      </div>
      {children}
    </section>
  )
}

export function DocsCode({
  language,
  children,
}: {
  language?: string
  children: string
}) {
  const code = children.trim()
  const [copied, setCopied] = useState(false)
  const timeout = useRef<ReturnType<typeof setTimeout> | null>(null)
  const onCopy = () => {
    if (typeof navigator === `undefined` || !navigator.clipboard) return
    navigator.clipboard.writeText(code)
    setCopied(true)
    if (timeout.current) clearTimeout(timeout.current)
    timeout.current = setTimeout(() => setCopied(false), 1400)
  }
  return (
    <div className="docs-code">
      <div className="docs-code-header">
        <span>{language ?? `shell`}</span>
        <button
          type="button"
          className="docs-code-copy"
          onClick={onCopy}
          aria-label="Copy to clipboard"
        >
          {copied ? <IcCheck size={11} /> : <IcCopy size={11} />}
          {copied ? `Copied` : `Copy`}
        </button>
      </div>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  )
}

export function DocsCallout({
  kind = `note`,
  title,
  children,
}: {
  kind?: `tip` | `warn` | `note`
  title?: string
  children: ReactNode
}) {
  return (
    <div className={`docs-callout docs-callout--${kind}`}>
      <span className="docs-callout-dot" aria-hidden />
      <div className="docs-callout-body">
        {title && <strong>{title}</strong>}
        {children}
      </div>
    </div>
  )
}

export function EnvVar({
  name,
  required,
  children,
}: {
  name: string
  required?: boolean
  children: ReactNode
}) {
  return (
    <>
      <dt>
        {name}
        {required && <span className="docs-env-req">required</span>}
      </dt>
      <dd>{children}</dd>
    </>
  )
}

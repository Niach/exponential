// Round 1 (contract §3 CodeBlock, a11y.json CodeBlock): the code block on
// the catalog's ONE tokenizer (`tokenizeCode`, catalog/code.json — the same
// tokens on every platform), painted per token kind through the
// `CodeBlock/token` recipe (`when: {kind}`). An optional header (title +
// the copy button: `builtinIcons` copy/copied, `copied` announced), a
// line-number gutter, emphasised `highlight` lines (the `selected` state),
// `wrap`, and `maxLines` before the block scrolls.

import { useMemo, useRef, useState } from "react"
import { tokenizeCode } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { arr, bool, BuiltinIcon, num, str, useParts } from "./shared"

export function CodeBlockNative({ node, props, rootProps, domId }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const code = str(props.code)
  const language = str(props.language, `plain`)
  const lines = useMemo(() => tokenizeCode(code, language), [code, language])
  const highlight = new Set(arr<unknown>(props.highlight).map((v) => num(v)))
  const numbers = bool(props.lineNumbers)
  const copyable = props.copyable !== false
  const title = str(props.title)
  // Round 2 (§7 CodeBlock): an untitled block names its language in the
  // header (`ts`; none for `plain`); the region's name stays `codeBlock`.
  const heading = title || (language === `plain` ? `` : language)
  const maxLines = props.maxLines === undefined ? 0 : num(props.maxLines)
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const copy = async () => {
    // Acknowledge only a copy that happened: no clipboard (an insecure
    // origin) or a refusal leaves the button as it was and says nothing.
    const clipboard = typeof navigator === `undefined` ? undefined : navigator.clipboard
    if (!clipboard?.writeText) return
    try {
      await clipboard.writeText(code)
    } catch {
      return
    }
    setCopied(true)
    ctx.announce(ctx.t(`copied`))
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => setCopied(false), 1500)
  }
  const gutterWidth = `${String(lines.length).length + 1}ch`
  return (
    <div {...(rootProps as Record<string, unknown>)} role="region" aria-label={title || ctx.t(`codeBlock`)} data-language={language}>
      {heading || copyable ? (
        <div {...(part(`header`) as Record<string, string>)}>
          {heading ? <span {...(part(`title`) as Record<string, string>)}>{heading}</span> : <span className="xui-codeblock-spacer" />}
          {copyable ? (
            <button type="button" {...(part(`copy`, copied && `checked`) as Record<string, string>)} aria-label={copied ? ctx.t(`copied`) : ctx.t(`copy`)} onClick={() => void copy()}>
              <BuiltinIcon slot={copied ? `CodeBlock.copied` : `CodeBlock.copy`} />
            </button>
          ) : null}
        </div>
      ) : null}
      <div {...(part(`body`) as Record<string, string>)} className={`${(part(`body`) as { className: string }).className} xui-codeblock-scroll`} data-wrap={bool(props.wrap) ? `true` : undefined} style={maxLines > 0 ? ({ "--xui-code-lines": maxLines } as React.CSSProperties) : undefined} data-max-lines={maxLines > 0 ? maxLines : undefined} tabIndex={0} aria-labelledby={title ? undefined : undefined}>
        <pre className="xui-codeblock-pre" id={`${domId}.code`}>
          <code>
            {lines.map((tokens, i) => (
              <span key={i} {...(part.at(`line`, i, highlight.has(i + 1) && `selected`) as Record<string, string>)} data-line={i + 1}>
                {numbers ? (
                  <span {...(part(`gutter`) as Record<string, string>)} style={{ minWidth: gutterWidth }} aria-hidden="true">
                    <span {...(part.at(`lineNumber`, i) as Record<string, string>)}>{i + 1}</span>
                  </span>
                ) : null}
                <span {...(part.at(`code`, i) as Record<string, string>)} className={`${(part(`code`) as { className: string }).className} xui-codeblock-text`}>
                  {tokens.length === 0 ? `​` : null}
                  {tokens.map((t, j) => (
                    <span key={j} {...(part.with(`token`, { kind: t.kind }) as Record<string, string>)}>
                      {t.text}
                    </span>
                  ))}
                </span>
                {`\n`}
              </span>
            ))}
          </code>
        </pre>
      </div>
    </div>
  )
}

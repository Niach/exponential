/* ─── Tiny regex-based syntax tinting for the diff + code tabs ─── */
import type { ReactNode } from "react"

type TokClass = `ide-tok-c` | `ide-tok-s` | `ide-tok-k` | `ide-tok-n`

const tokenize = (
  text: string,
  re: RegExp,
  classify: (m: RegExpExecArray) => TokClass,
): ReactNode[] => {
  const out: ReactNode[] = []
  let last = 0
  let key = 0
  re.lastIndex = 0
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > last) out.push(text.slice(last, m.index))
    out.push(
      <span key={key} className={classify(m)}>
        {m[0]}
      </span>,
    )
    key += 1
    last = m.index + m[0].length
    if (m[0].length === 0) re.lastIndex += 1
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

const TS_RE =
  /(\/\/[^\n]*)|(`(?:[^`\\]|\\.)*`|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\b(const|let|var|function|return|new|typeof|import|export|from|if|else|for|while|null|undefined|true|false|async|await)\b|\b(\d[\d_]*(?:\.\d+)?)\b/g

export const tintTs = (text: string): ReactNode[] =>
  tokenize(text, TS_RE, (m) => {
    if (m[1]) return `ide-tok-c`
    if (m[2]) return `ide-tok-s`
    if (m[3]) return `ide-tok-k`
    return `ide-tok-n`
  })

const JSON_RE =
  /("(?:[^"\\]|\\.)*")(?=\s*:)|("(?:[^"\\]|\\.)*")|\b(true|false|null)\b|(-?\d+(?:\.\d+)?)/g

export const tintJson = (text: string): ReactNode[] =>
  tokenize(text, JSON_RE, (m) => {
    if (m[1]) return `ide-tok-k`
    if (m[2]) return `ide-tok-s`
    return `ide-tok-n`
  })

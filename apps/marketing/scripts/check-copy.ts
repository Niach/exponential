/* Site copy lint: bans the "X, never Y" / "X, not Y" contrast construction and
   its close variants ("not X but Y", "X — not Y", "not just", "No X. Just Y.",
   "It's not X. It's Y.", "instead of", "rather than") from the prose of
   exponential.at. Say what the thing does; don't define it by what it isn't.

   Only PROSE is scanned. TS/TSX files are parsed with the TypeScript compiler,
   so comments never count; JSX text is flattened per element tree (inline tags
   such as <strong> join the sentence, block tags break it) and string or
   template literals are checked one by one. Code never counts: <DocsCode>,
   <CodeBlock>, <pre> and <code> subtrees, literals that look like source
   (multi-line with braces, semicolons, arrows or tags), import paths, console
   and Error messages, className-like attributes. Markdown skips ``` fences and
   `inline code`; HTML skips <script>, <style>, <pre> and <code>.

   `bun run scripts/check-copy.ts` exits 1 listing file:line; the bun test
   ui/src/lib/copy.test.ts runs the same check in CI. */
import { readdirSync, readFileSync, statSync, existsSync } from "node:fs"
import { join, relative, resolve } from "node:path"
import ts from "typescript"

export const ROOT = resolve(import.meta.dirname, `..`)

/** Directories and files whose text ships as site copy. */
const SCOPE = [
  `src`,
  `ui/src`,
  `docs`,
  `blog`,
  `pricing`,
  `download`,
  `contact`,
  `imprint`,
  `privacy`,
  `terms`,
  `index.html`,
  `ui/index.html`,
  `scripts/generate-social.tsx`,
  `scripts/generate-og.tsx`,
  `scripts/prerender.tsx`,
  `scripts/store`,
]
const SKIP_DIRS = new Set([`node_modules`, `generated`, `dist`])
const EXT = /\.(tsx?|md|html)$/

export interface Pattern {
  id: string
  re: RegExp
}

/* `¶` marks a block boundary (a paragraph, list item or cell edge), so a
   pattern never reads across two paragraphs. */
export const PATTERNS: Pattern[] = [
  { id: `X, never Y`, re: /,\s+never\b/i },
  { id: `X, not Y`, re: /,\s+not\b/i },
  { id: `X — not Y`, re: /(?:\s[—–-]+\s*|[—–])(?:not|never)\b/i },
  { id: `not just / not only`, re: /\bnot\s+(?:only|just|merely|simply)\b/i },
  { id: `not X but Y`, re: /\bnot\b[^.!?;:¶]{1,80}?\bbut\b/i },
  { id: `instead of`, re: /\binstead\s+of\b/i },
  { id: `rather than`, re: /\brather\s+than\b/i },
  { id: `No X. Just Y.`, re: /\bno\s+[^.!?¶]{1,60}[.!?]\s*(?:¶\s*)?Just\b/i },
  { id: `Not a X. (fragment)`, re: /(?:^|[.!?]\s+|¶\s*)Not\s+(?:a|an|the|just|only)\s/ },
  {
    id: `It's not X. It's Y.`,
    re: /\b(?:it|this|that)(?:['’]s\s+not|\s+is\s+not|\s+isn['’]t)\b[^.!?¶]{0,80}[.!?]\s+(?:it|this|that)(?:['’]s|\s+is)\b/i,
  },
]

export interface Finding {
  file: string
  line: number
  pattern: string
  text: string
}

interface Segment {
  text: string
  pos: number
}

/** A run of prose: segments concatenated, each remembering its source offset. */
class Run {
  segments: Segment[] = []
  push(text: string, pos: number): void {
    this.segments.push({ text, pos })
  }
  get text(): string {
    return this.segments.map((s) => s.text).join(``)
  }
  /** Source offset of a character index in `text`. */
  offset(index: number): number {
    let at = 0
    for (const s of this.segments) {
      if (index < at + s.text.length) return s.pos + (index - at)
      at += s.text.length
    }
    return this.segments.at(-1)?.pos ?? 0
  }
}

const CODE_TAGS = new Set([`DocsCode`, `CodeBlock`, `pre`, `code`, `kbd`, `samp`, `script`, `style`])
const INLINE_TAGS = new Set([`strong`, `em`, `b`, `i`, `a`, `span`, `abbr`, `sup`, `sub`, `small`, `mark`, `u`, `s`, `Link`, `br`])
const CODE_ATTRS = new Set([`className`, `class`, `style`, `href`, `src`, `id`, `key`, `language`, `lang`, `to`, `rel`, `target`, `type`, `name`, `value`, `d`, `viewBox`])

function tagName(node: ts.JsxElement | ts.JsxSelfClosingElement | ts.JsxFragment): string {
  if (ts.isJsxFragment(node)) return ``
  const tag = ts.isJsxElement(node) ? node.openingElement.tagName : node.tagName
  return tag.getText()
}

/** A literal that is source code (a sample, a snippet, a shell line), not prose. */
function looksLikeCode(text: string): boolean {
  if (/\n/.test(text) && /[{};]|=>|<\/?[A-Za-z]|^\s*(?:import|export|const|let|fn|func|use|curl|bun|npm)\b/m.test(text)) return true
  if (/^\s*[{[<]/.test(text) && /[}\]>]\s*$/.test(text)) return true
  return false
}

const ENTITIES: Record<string, string> = { apos: `'`, rsquo: `’`, lsquo: `‘`, ldquo: `“`, rdquo: `”`, quot: `"`, amp: `&`, nbsp: ` `, mdash: `—`, ndash: `–`, hellip: `…`, lt: `<`, gt: `>` }
/** JSX text keeps HTML entities raw; `&rdquo;` must not read as a semicolon. */
const decodeEntities = (text: string): string => text.replace(/&(#?\w+);/g, (_, name: string) => ENTITIES[name] ?? `x`)

function literalText(node: ts.Node): string | undefined {
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text
  return undefined
}

function scanTs(file: string, source: string): Run[] {
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, file.endsWith(`.tsx`) ? ts.ScriptKind.TSX : ts.ScriptKind.TS)
  const runs: Run[] = []

  const flatten = (node: ts.Node, run: Run): void => {
    if (ts.isJsxText(node)) {
      run.push(decodeEntities(node.text), node.pos)
      return
    }
    if (ts.isJsxExpression(node)) {
      const lit = node.expression ? literalText(node.expression) : undefined
      if (lit !== undefined) run.push(lit, node.expression!.getStart(sf) + 1)
      else run.push(` x `, node.getStart(sf))
      return
    }
    if (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node) || ts.isJsxFragment(node)) {
      const name = tagName(node)
      const inline = name === `` || INLINE_TAGS.has(name)
      if (CODE_TAGS.has(name)) {
        run.push(name === `code` || name === `kbd` || name === `samp` ? ` \`x\` ` : `¶`, node.getStart(sf))
        return
      }
      if (!inline) run.push(`¶`, node.getStart(sf))
      if (!ts.isJsxSelfClosingElement(node)) for (const child of node.children) flatten(child, run)
      if (!inline) run.push(`¶`, node.getEnd())
    }
  }

  const isJsxChild = (node: ts.Node): boolean => {
    const p = node.parent
    return !!p && (ts.isJsxElement(p) || ts.isJsxFragment(p))
  }

  const skipLiteral = (node: ts.Node): boolean => {
    for (let p: ts.Node | undefined = node.parent; p; p = p.parent) {
      if (ts.isImportDeclaration(p) || ts.isExportDeclaration(p) || ts.isExternalModuleReference(p)) return true
      if (ts.isJsxAttribute(p) && CODE_ATTRS.has(p.name.getText(sf))) return true
      if (ts.isCallExpression(p) && /^(?:console\.\w+|require|import)$/.test(p.expression.getText(sf))) return true
      if (ts.isNewExpression(p) && /Error$/.test(p.expression.getText(sf))) return true
      if (ts.isJsxElement(p) && CODE_TAGS.has(tagName(p))) return true
      if (ts.isPropertyAssignment(p) && p.name === node) return true
      if (ts.isElementAccessExpression(p) || ts.isTypeNode(p)) return true
      if (ts.isStatement(p) || ts.isJsxElement(p)) break
    }
    return false
  }

  const visit = (node: ts.Node): void => {
    if ((ts.isJsxElement(node) || ts.isJsxFragment(node) || ts.isJsxSelfClosingElement(node)) && !isJsxChild(node)) {
      const run = new Run()
      flatten(node, run)
      runs.push(run)
    }
    // A JSX child literal ({`...`}) was already read by flatten().
    const inJsxChildren = node.parent && ts.isJsxExpression(node.parent) && isJsxChild(node.parent)
    if (!inJsxChildren) {
      const lit = literalText(node)
      if (lit !== undefined && !skipLiteral(node) && !looksLikeCode(lit)) {
        const run = new Run()
        run.push(lit, node.getStart(sf) + 1)
        runs.push(run)
      }
      if (ts.isTemplateExpression(node) && !skipLiteral(node)) {
        const run = new Run()
        run.push(node.head.text, node.head.getStart(sf) + 1)
        for (const span of node.templateSpans) run.push(` x ${span.literal.text}`, span.literal.getStart(sf) + 1)
        if (!looksLikeCode(run.text)) runs.push(run)
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(sf)
  return runs
}

/** Blank out a regex match, keeping offsets (and newlines) intact. */
const blank = (re: RegExp, text: string): string => text.replace(re, (m) => m.replace(/[^\n]/g, ` `))

function scanMarkdown(source: string): Run[] {
  let text = blank(/```[\s\S]*?```/g, source)
  text = blank(/`[^`\n]*`/g, text)
  const run = new Run()
  run.push(text.replace(/\n\s*\n/g, (m) => `¶`.padEnd(m.length, `\n`)), 0)
  return [run]
}

function scanHtml(source: string): Run[] {
  let text = blank(/<(script|style|pre|code)\b[\s\S]*?<\/\1>/gi, source)
  const runs: Run[] = []
  // Attribute copy (meta descriptions, titles, alt text).
  for (const m of text.matchAll(/\b(?:content|title|alt|aria-label)="([^"]*)"/g)) {
    const run = new Run()
    run.push(m[1]!, m.index! + m[0].indexOf(`"`) + 1)
    runs.push(run)
  }
  text = text.replace(/<[^>]*>/g, (m) => `¶`.padEnd(m.length, ` `))
  const run = new Run()
  run.push(text, 0)
  runs.push(run)
  return runs
}

const lineOf = (source: string, offset: number): number => source.slice(0, offset).split(`\n`).length

export function checkSource(file: string, source: string): Finding[] {
  const runs = file.endsWith(`.md`) ? scanMarkdown(source) : file.endsWith(`.html`) ? scanHtml(source) : scanTs(file, source)
  const findings: Finding[] = []
  for (const run of runs) {
    const text = run.text
    for (const { id, re } of PATTERNS) {
      const global = new RegExp(re.source, re.flags.includes(`g`) ? re.flags : `${re.flags}g`)
      for (const m of text.matchAll(global)) {
        const start = Math.max(0, m.index! - 50)
        const snippet = text.slice(start, m.index! + m[0].length + 30).replace(/¶/g, ` `).replace(/\s+/g, ` `).trim()
        findings.push({ file, line: lineOf(source, run.offset(m.index!)), pattern: id, text: snippet })
      }
    }
  }
  return findings.sort((a, b) => a.line - b.line)
}

function walk(path: string, out: string[]): void {
  if (!existsSync(path)) return
  if (statSync(path).isDirectory()) {
    for (const name of readdirSync(path).sort()) {
      if (SKIP_DIRS.has(name) || name.startsWith(`.`)) continue
      walk(join(path, name), out)
    }
  } else if (EXT.test(path) && !/\.test\.tsx?$/.test(path)) out.push(path)
}

export function copyFiles(): string[] {
  const files: string[] = []
  for (const entry of SCOPE) walk(join(ROOT, entry), files)
  return files
}

export function checkCopy(files = copyFiles()): Finding[] {
  return files.flatMap((f) => checkSource(relative(ROOT, f), readFileSync(f, `utf8`)))
}

if (import.meta.main) {
  const findings = checkCopy()
  for (const f of findings) console.log(`${f.file}:${f.line}  [${f.pattern}]  ${f.text}`)
  if (findings.length > 0) {
    console.error(`\ncheck-copy: ${findings.length} contrast construction(s) in site copy. Say what the thing does.`)
    process.exit(1)
  }
  console.log(`check-copy: clean`)
}

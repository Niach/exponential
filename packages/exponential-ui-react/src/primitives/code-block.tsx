import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Check, Copy } from "lucide-react"
import { tokenizeCode } from "@exponential-at/ui"

import { Button } from "./button"
import { cn } from "./cn"

// EXP-1324 — THE CodeBlock: shadcn's code-block (radix-nova) on our Button,
// coloured by the catalog's ONE tokenizer (`tokenizeCode`, catalog/code.json
// — the same tokens every renderer paints; `ansi` = command output, the
// Terminal pattern). Size `sm` (fixtures/trace.json `geometry.codeBlock`) is
// tool output inside a Trace line; `destructive` is a failed call's.

const codeBlockVariants = cva(
  `group/code-block relative flex w-full min-w-0 flex-col overflow-hidden rounded-lg text-sm has-[>[data-slot=code-block-content]:focus-visible]:ring-3 has-[>[data-slot=code-block-content]:focus-visible]:ring-ring/50 data-[size=sm]:text-xs`,
  {
    variants: {
      variant: {
        default: `border border-input text-foreground has-[>[data-slot=code-block-content]:focus-visible]:border-ring dark:bg-input/30`,
        destructive: `border border-destructive/30 text-destructive dark:border-destructive/40`,
      },
    },
    defaultVariants: {
      variant: `default`,
    },
  }
)

function CodeBlock({
  className,
  variant = `default`,
  size = `default`,
  ...props
}: React.ComponentProps<`div`> &
  VariantProps<typeof codeBlockVariants> & { size?: `default` | `sm` }) {
  return (
    <div
      data-slot="code-block"
      data-variant={variant}
      data-size={size}
      className={cn(codeBlockVariants({ variant }), className)}
      {...props}
    />
  )
}

function CodeBlockHeader({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="code-block-header"
      className={cn(
        `flex min-w-0 shrink-0 items-center gap-2 px-3 pt-3 text-muted-foreground group-data-[size=sm]/code-block:px-2.5 group-data-[size=sm]/code-block:pt-2.5 group-data-[variant=destructive]/code-block:text-destructive *:has-[>[data-slot=code-block-title]]:min-w-0`,
        className
      )}
      {...props}
    />
  )
}

function CodeBlockTitle({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="code-block-title"
      className={cn(
        `min-w-0 truncate font-sans text-sm font-medium text-foreground group-data-[size=sm]/code-block:text-xs group-data-[variant=destructive]/code-block:text-destructive [&_svg]:pointer-events-none [&_svg]:inline-block [&_svg]:align-middle [&_svg:not([class*='size-'])]:size-3.5 *:[svg]:me-1.5`,
        className
      )}
      {...props}
    />
  )
}

function CodeBlockActions({
  className,
  ...props
}: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="code-block-actions"
      className={cn(
        `-my-1 ms-auto -me-1 flex shrink-0 items-center gap-1 [[data-slot=code-block]>&]:absolute [[data-slot=code-block]>&]:end-2 [[data-slot=code-block]>&]:top-2 [[data-slot=code-block]>&]:z-10 [[data-slot=code-block]>&]:m-0 [[data-slot=code-block]>&]:opacity-0 [[data-slot=code-block]>&]:transition-opacity [[data-slot=code-block]>&]:group-focus-within/code-block:opacity-100 [[data-slot=code-block]>&]:group-hover/code-block:opacity-100 group-data-[size=sm]/code-block:[[data-slot=code-block]>&]:end-1.5 group-data-[size=sm]/code-block:[[data-slot=code-block]>&]:top-1.5 [[data-slot=code-block]>&]:pointer-coarse:opacity-100 [[data-slot=code-block]>&]:*:[button]:bg-background [[data-slot=code-block]>&]:*:[button]:ring-1 [[data-slot=code-block]>&]:*:[button]:ring-foreground/10`,
        className
      )}
      {...props}
    />
  )
}

/** The copy action: ui-copy → ui-check for 2s, announced politely. A copy
 *  that did not happen (no clipboard, a refusal) changes nothing. */
function CodeBlockCopyButton({
  value,
  variant = `ghost`,
  size = `icon-xs`,
  className,
  onClick,
  labels = { copy: `Copy`, copied: `Copied` },
  ...props
}: React.ComponentProps<typeof Button> & {
  value: string
  labels?: { copy: string; copied: string }
}) {
  const [copied, setCopied] = React.useState(false)

  React.useEffect(() => {
    if (!copied) return
    const timeout = setTimeout(() => setCopied(false), 2000)
    return () => clearTimeout(timeout)
  }, [copied])

  return (
    <>
      <Button
        data-slot="code-block-copy-button"
        data-copied={copied || undefined}
        variant={variant}
        size={size}
        aria-label={copied ? labels.copied : labels.copy}
        className={cn(className)}
        onClick={(event) => {
          onClick?.(event)
          const clipboard =
            typeof navigator === `undefined` ? undefined : navigator.clipboard
          if (!clipboard?.writeText) return
          clipboard
            .writeText(value)
            .then(() => setCopied(true))
            .catch(() => {})
        }}
        {...props}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
      <span aria-live="polite" className="sr-only">
        {copied ? labels.copied : ``}
      </span>
    </>
  )
}

/** The catalog's `codeToken` kinds, painted with the app's code palette
 *  (`--hljs-*`, the diff view's and the editor's); a destructive block drops
 *  the colours for its own. */
const CODE_TOKEN_CLASS: Record<string, string> = {
  keyword: `text-(--hljs-keyword)`,
  string: `text-(--hljs-string)`,
  number: `text-(--hljs-number)`,
  comment: `text-muted-foreground`,
  type: `text-(--hljs-title)`,
  function: `text-(--hljs-title)`,
  property: `text-(--hljs-variable)`,
  attribute: `text-(--hljs-variable)`,
  tag: `text-(--hljs-tag)`,
  operator: `text-muted-foreground`,
  punctuation: `text-muted-foreground`,
  success: `text-(--hljs-addition)`,
  warning: `text-(--hljs-number)`,
  error: `text-destructive`,
}

function CodeBlockContent({
  className,
  children,
  style,
  wrap = false,
  lineNumbers = false,
  startLine = 1,
  language,
  maxLines = 16,
  ...props
}: React.ComponentProps<`pre`> & {
  wrap?: boolean
  lineNumbers?: boolean
  startLine?: number
  /** A catalog language (`ts`, `json`, `ansi`…): string children are
   *  tokenized and coloured per token kind. */
  language?: string
  /** Lines shown before the block scrolls; 0 = unbounded. */
  maxLines?: number
}) {
  const source = typeof children === `string` ? children : null
  const lines = React.useMemo(
    () =>
      source !== null && language
        ? tokenizeCode(source.replace(/\n$/, ``), language)
        : null,
    [source, language]
  )
  return (
    <pre
      data-slot="code-block-content"
      data-wrap={wrap || undefined}
      data-numbered={lineNumbers || undefined}
      data-language={language}
      dir="ltr"
      tabIndex={0}
      style={{
        counterReset: `line ${startLine - 1}`,
        ...(maxLines > 0
          ? { maxHeight: `calc(${maxLines}lh + var(--code-block-pad, 1.5rem))` }
          : null),
        ...style,
      }}
      className={cn(
        `min-h-0 min-w-0 scroll-fade-x overflow-auto overscroll-contain px-3 py-3 text-start font-mono leading-relaxed outline-none [--code-block-pad:calc(var(--spacing)*6)] group-has-data-[slot=code-block-footer]/code-block:pb-2 group-has-data-[slot=code-block-header]/code-block:pt-2 group-has-data-[slot=code-block-header]/code-block:[--code-block-pad:calc(var(--spacing)*5)] group-data-[size=sm]/code-block:px-2.5 group-data-[size=sm]/code-block:py-2.5 group-data-[size=sm]/code-block:[--code-block-pad:calc(var(--spacing)*5)] group-data-[size=sm]/code-block:group-has-data-[slot=code-block-header]/code-block:pt-1.5 group-data-[size=sm]/code-block:group-has-data-[slot=code-block-header]/code-block:[--code-block-pad:calc(var(--spacing)*4)] data-wrap:scroll-fade-y data-wrap:wrap-break-word data-wrap:whitespace-pre-wrap`,
        className
      )}
      {...props}
    >
      <code
        data-slot="code-block-code"
        className="grid min-h-lh min-w-max content-start in-data-numbered:grid-cols-[auto_1fr] in-data-wrap:min-w-0"
      >
        {lines !== null
          ? lines.map((tokens, index) => (
              <CodeBlockLine key={index}>
                {tokens.length === 0
                  ? null
                  : tokens.map((token, j) => (
                      <span
                        key={j}
                        data-token={token.kind}
                        className={cn(
                          CODE_TOKEN_CLASS[token.kind],
                          `group-data-[variant=destructive]/code-block:text-current`
                        )}
                      >
                        {token.text}
                      </span>
                    ))}
              </CodeBlockLine>
            ))
          : lineNumbers && source !== null
            ? source
                .replace(/\n$/, ``)
                .split(`\n`)
                .map((line, index) => (
                  <CodeBlockLine key={index}>{line}</CodeBlockLine>
                ))
            : children}
      </code>
    </pre>
  )
}

const codeBlockLineVariants = cva(
  `relative -mx-3 block min-h-lh px-3 group-data-[size=sm]/code-block:-mx-2.5 group-data-[size=sm]/code-block:px-2.5 before:min-w-6 before:text-end before:text-muted-foreground after:absolute after:start-1 after:top-0 after:text-muted-foreground after:select-none in-data-numbered:col-span-2 in-data-numbered:grid in-data-numbered:grid-cols-subgrid in-data-numbered:gap-x-3 in-data-numbered:[counter-increment:line] in-data-numbered:before:content-[counter(line)] in-data-numbered:before:select-none in-data-numbered:supports-[content:counter(line)_/_'']:before:content-[counter(line)_/_'']`,
  {
    variants: {
      variant: {
        default: ``,
        added: `bg-muted after:content-['+']`,
        removed: `bg-destructive/10 after:text-destructive after:content-['−']`,
        highlighted: `bg-muted`,
      },
    },
    defaultVariants: {
      variant: `default`,
    },
  }
)

function CodeBlockLine({
  className,
  variant = `default`,
  children,
  ...props
}: React.ComponentProps<`span`> & VariantProps<typeof codeBlockLineVariants>) {
  // An empty line holds a newline so copying a selection keeps blank lines.
  const empty =
    children === undefined ||
    children === null ||
    children === `` ||
    (Array.isArray(children) && children.length === 0)

  return (
    <span
      data-slot="code-block-line"
      data-variant={variant}
      className={cn(codeBlockLineVariants({ variant }), className)}
      {...props}
    >
      <span data-slot="code-block-line-content" className="min-w-0">
        {empty ? `\n` : children}
      </span>
    </span>
  )
}

function CodeBlockFooter({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="code-block-footer"
      className={cn(
        `flex min-w-0 shrink-0 items-center gap-2 px-3 pb-3 text-muted-foreground group-data-[size=sm]/code-block:px-2.5 group-data-[size=sm]/code-block:pb-2.5`,
        className
      )}
      {...props}
    />
  )
}

export {
  CodeBlock,
  CodeBlockHeader,
  CodeBlockTitle,
  CodeBlockActions,
  CodeBlockCopyButton,
  CodeBlockContent,
  CodeBlockLine,
  CodeBlockFooter,
  CODE_TOKEN_CLASS,
}

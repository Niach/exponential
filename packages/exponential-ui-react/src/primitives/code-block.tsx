import * as React from "react"
import type { VariantProps } from "class-variance-authority"
import { Check, Copy } from "lucide-react"
import { tokenizeCode } from "@exponential_app/ui"
import { CODE_BLOCK_CLASSES, CODE_TOKEN_CLASS, codeBlockLineVariants, codeBlockVariants } from "@exponential_app/ui/web/primitives"

import { Button } from "./button"
import { cn } from "./cn"

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
        CODE_BLOCK_CLASSES.header,
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
        CODE_BLOCK_CLASSES.title,
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
        CODE_BLOCK_CLASSES.actions,
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
      <span aria-live="polite" className={CODE_BLOCK_CLASSES.srOnly}>
        {copied ? labels.copied : ``}
      </span>
    </>
  )
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
        CODE_BLOCK_CLASSES.content,
        className
      )}
      {...props}
    >
      <code
        data-slot="code-block-code"
        className={CODE_BLOCK_CLASSES.code}
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
                          CODE_BLOCK_CLASSES.token
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
      <span data-slot="code-block-line-content" className={CODE_BLOCK_CLASSES.lineContent}>
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
        CODE_BLOCK_CLASSES.footer,
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

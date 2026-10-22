import { cva, type VariantProps } from "class-variance-authority"

// EXP-1324 — THE CodeBlock: shadcn's code-block (radix-nova) on our Button,
// coloured by the catalog's ONE tokenizer (`tokenizeCode`, catalog/code.json
// — the same tokens every renderer paints; `ansi` = command output, the
// Terminal pattern). Size `sm` (fixtures/trace.json `geometry.codeBlock`) is
// tool output inside a Trace line; `destructive` is a failed call's.

export const codeBlockVariants = cva(
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

export type CodeBlockVariantProps = VariantProps<typeof codeBlockVariants>

/** The catalog's `codeToken` kinds, painted with the app's code palette
 *  (`--hljs-*`, the diff view's and the editor's); a destructive block drops
 *  the colours for its own. */
export const CODE_TOKEN_CLASS: Record<string, string> = {
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

export const codeBlockLineVariants = cva(
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

export type CodeBlockLineVariantProps = VariantProps<typeof codeBlockLineVariants>

export const CODE_BLOCK_CLASSES = {
  header: `flex min-w-0 shrink-0 items-center gap-2 px-3 pt-3 text-muted-foreground group-data-[size=sm]/code-block:px-2.5 group-data-[size=sm]/code-block:pt-2.5 group-data-[variant=destructive]/code-block:text-destructive *:has-[>[data-slot=code-block-title]]:min-w-0`,
  title: `min-w-0 truncate font-sans text-sm font-medium text-foreground group-data-[size=sm]/code-block:text-xs group-data-[variant=destructive]/code-block:text-destructive [&_svg]:pointer-events-none [&_svg]:inline-block [&_svg]:align-middle [&_svg:not([class*='size-'])]:size-3.5 *:[svg]:me-1.5`,
  actions: `-my-1 ms-auto -me-1 flex shrink-0 items-center gap-1 [[data-slot=code-block]>&]:absolute [[data-slot=code-block]>&]:end-2 [[data-slot=code-block]>&]:top-2 [[data-slot=code-block]>&]:z-10 [[data-slot=code-block]>&]:m-0 [[data-slot=code-block]>&]:opacity-0 [[data-slot=code-block]>&]:transition-opacity [[data-slot=code-block]>&]:group-focus-within/code-block:opacity-100 [[data-slot=code-block]>&]:group-hover/code-block:opacity-100 group-data-[size=sm]/code-block:[[data-slot=code-block]>&]:end-1.5 group-data-[size=sm]/code-block:[[data-slot=code-block]>&]:top-1.5 [[data-slot=code-block]>&]:pointer-coarse:opacity-100 [[data-slot=code-block]>&]:*:[button]:bg-background [[data-slot=code-block]>&]:*:[button]:ring-1 [[data-slot=code-block]>&]:*:[button]:ring-foreground/10`,
  srOnly: `sr-only`,
  content: `min-h-0 min-w-0 scroll-fade-x overflow-auto overscroll-contain px-3 py-3 text-start font-mono leading-relaxed outline-none [--code-block-pad:calc(var(--spacing)*6)] group-has-data-[slot=code-block-footer]/code-block:pb-2 group-has-data-[slot=code-block-header]/code-block:pt-2 group-has-data-[slot=code-block-header]/code-block:[--code-block-pad:calc(var(--spacing)*5)] group-data-[size=sm]/code-block:px-2.5 group-data-[size=sm]/code-block:py-2.5 group-data-[size=sm]/code-block:[--code-block-pad:calc(var(--spacing)*5)] group-data-[size=sm]/code-block:group-has-data-[slot=code-block-header]/code-block:pt-1.5 group-data-[size=sm]/code-block:group-has-data-[slot=code-block-header]/code-block:[--code-block-pad:calc(var(--spacing)*4)] data-wrap:scroll-fade-y data-wrap:wrap-break-word data-wrap:whitespace-pre-wrap`,
  code: `grid min-h-lh min-w-max content-start in-data-numbered:grid-cols-[auto_1fr] in-data-wrap:min-w-0`,
  token: `group-data-[variant=destructive]/code-block:text-current`,
  lineContent: `min-w-0`,
  footer: `flex min-w-0 shrink-0 items-center gap-2 px-3 pb-3 text-muted-foreground group-data-[size=sm]/code-block:px-2.5 group-data-[size=sm]/code-block:pb-2.5`,
} as const

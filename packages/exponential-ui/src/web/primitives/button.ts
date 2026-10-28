import { cva, type VariantProps } from "class-variance-authority"

export const buttonVariants = cva(
  `inline-flex shrink-0 items-center justify-center gap-2 rounded-md text-sm font-medium whitespace-nowrap transition-all duration-fast ease-standard outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4`,
  {
    variants: {
      variant: {
        default: `bg-primary text-primary-foreground hover:bg-primary/90`,
        destructive: `bg-destructive text-white hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:bg-destructive/60 dark:focus-visible:ring-destructive/40`,
        outline: `border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:border-input dark:bg-input/30 dark:hover:bg-input/50`,
        glass: `border border-glass-stroke-card bg-glass-card text-foreground/70 hover:bg-glass-active hover:text-foreground`,
        secondary: `bg-secondary text-secondary-foreground hover:bg-secondary/80`,
        // EXP-960: a ghost TOGGLE says so with `aria-pressed`, and the pressed
        // state is the hover wash held down (the editor's formatting rail).
        // The `hover` arm names dark's half wash too: a pressed button under
        // the pointer must not fade to it.
        ghost: `hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50 aria-pressed:bg-accent aria-pressed:text-accent-foreground dark:aria-pressed:hover:bg-accent`,
        link: `text-primary underline-offset-4 hover:underline`,
        // EXP-962: the TEXT button — muted words that brighten under the
        // pointer and never underline: a fold's "Show more"/"Show less", a
        // step's "Back to the current step". `link` goes SOMEWHERE (it
        // underlines like a link); `text` toggles something in place.
        text: `text-muted-foreground hover:text-foreground`,
      },
      // EXP-1176: a TEXT button is a rounded rectangle at the ladder's MD
      // step (10px, the row corner), the shape desktop `web_md`, iOS
      // `GlassSubmitButton` and Android `GlassSubmitButton` already wear. The
      // base `rounded-md` covers every boxed size; EXP-616's capsule is gone
      // from buttons. The capsule is the `Pill` (chips, header actions, the
      // picker trigger), the tabs, the badge and the phone bars, never a
      // Button with words in it. Icon sizes stay circles (the primary-action
      // rule, EXP-771); the ghost icon button alone sits on the MD corner.
      size: {
        default: `h-9 px-4 py-2 has-[>svg]:px-3`,
        xs: `h-6 gap-1 px-2 text-xs has-[>svg]:px-1.5 [&_svg:not([class*='size-'])]:size-3`,
        sm: `h-8 gap-1.5 px-3 has-[>svg]:px-2.5`,
        lg: `h-10 px-6 has-[>svg]:px-4`,
        icon: `size-9 rounded-full`,
        // EXP-698 — the ONE trailing-action size: every row/header action
        // (play, merge, resume, copy link, the "…" menus) is a 32px chromed
        // glass circle.
        "icon-sm": `size-8 rounded-full [&_svg:not([class*='size-'])]:size-4`,
        "icon-xs": `size-6 rounded-full [&_svg:not([class*='size-'])]:size-3`,
        // S-3 (voice-call.json): the phone call's round buttons (styleguide
        // `call-controls`): a 56px circle with a 24px glyph.
        "icon-lg": `size-14 rounded-full [&_svg:not([class*='size-'])]:size-6`,
        // EXP-962: INLINE — no box at all: the words sit in the run of text
        // around them at 12px, only the colour (and, for `link`, the
        // underline) says they are a button. For `link` and `text`; the
        // steer feed's "Show more", the session bands' "Continues in a
        // newer run", the edited-files card's footer.
        inline: `h-auto gap-1 rounded-sm p-0 text-xs has-[>svg]:px-0 [&_svg:not([class*='size-'])]:size-3`,
      },
    },
    // EXP-862: a GHOST icon button is the secondary control (no circle, no
    // border, the row wash on hover), so its hover paints under the MD corner
    // like desktop `controls::ghost_icon_button` and the styleguide's ghost
    // entry, not as a disc.
    compoundVariants: [
      {
        variant: `ghost`,
        size: [`icon`, `icon-sm`, `icon-xs`, `icon-lg`],
        className: `rounded-md`,
      },
      // EXP-1243: a boxed button presses to `scale.press` (motion-press);
      // the ghost icon button (a row's or toolbar's quiet control) and the
      // words-only link/text buttons do not.
      {
        variant: [`default`, `destructive`, `outline`, `glass`, `secondary`, `ghost`],
        size: [`default`, `xs`, `sm`, `lg`],
        className: `motion-press`,
      },
      {
        variant: [`default`, `destructive`, `outline`, `glass`, `secondary`],
        size: [`icon`, `icon-sm`, `icon-xs`, `icon-lg`],
        className: `motion-press`,
      },
    ],
    defaultVariants: {
      variant: `default`,
      size: `default`,
    },
  }
)

export type ButtonVariantProps = VariantProps<typeof buttonVariants>

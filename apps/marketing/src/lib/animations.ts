import type { Variants } from "motion/react"

/* Expo-out — fast start, long settle. The house ease for reveals. */
export const EASE_EXPO: [number, number, number, number] = [0.16, 1, 0.3, 1]

export const fadeInUp: Variants = {
  hidden: { opacity: 0, y: 20 },
  visible: { opacity: 1, y: 0 },
}

export const staggerContainer: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.12 },
  },
}

export const cardReveal: Variants = {
  hidden: { opacity: 0, y: 24 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.6, ease: EASE_EXPO },
  },
}

export const heroStagger: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.1, delayChildren: 0.1 },
  },
}

export const heroChild: Variants = {
  hidden: { opacity: 0, y: 16 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.6, ease: EASE_EXPO },
  },
}

/* Hero H1 — per-word stagger (~40ms). The h1 is a variants container nested
   under heroStagger; each word is a heroWord span. */
export const heroTitleStagger: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.04 },
  },
}

export const heroWord: Variants = {
  hidden: { opacity: 0, y: `0.35em` },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.55, ease: EASE_EXPO },
  },
}

export const sectionReveal = {
  initial: { opacity: 0, y: 20 },
  whileInView: { opacity: 1, y: 0 },
  viewport: { once: true, amount: 0.15 },
  transition: { duration: 0.65, ease: EASE_EXPO },
} as const

/* .section-eyebrow line-draw: drives the ::before scaleX via a CSS variable
   (see site.css). Default var value is 1, so eyebrows outside motion spans
   render fully drawn. */
export const eyebrowDraw = {
  initial: { [`--eyebrow-draw`]: 0 },
  whileInView: { [`--eyebrow-draw`]: 1 },
  viewport: { once: true, amount: 0.5 },
  transition: { duration: 0.9, ease: EASE_EXPO },
} as const

export const viewportOnce = { once: true, amount: 0.2 } as const

/* Terminal lines appear — they never float. */
export const termLine: Variants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1, transition: { duration: 0.12 } },
}

export const glowIn: Variants = {
  hidden: { opacity: 0, filter: `brightness(1.6)` },
  visible: {
    opacity: 1,
    filter: `brightness(1)`,
    transition: { duration: 0.5, ease: `easeOut` },
  },
}

export const slideReveal: Variants = {
  hidden: { clipPath: `inset(0 100% 0 0)` },
  visible: {
    clipPath: `inset(0 0% 0 0)`,
    transition: { duration: 0.7, ease: `easeOut` },
  },
}

export const timelineStep: Variants = {
  hidden: { opacity: 0, y: 16, scale: 0.97 },
  visible: {
    opacity: 1,
    y: 0,
    scale: 1,
    transition: { duration: 0.4, ease: `easeOut` },
  },
}

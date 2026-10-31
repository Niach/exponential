/* The one live A2UI surface of "UI as data, down to the compositor"
   (EXP-1369): the expense form the desktop draft shows being built, written
   as a tree of the core catalog and painted in the page by the Exponential
   UI React renderer (components/blog/LiveSurface). scripts/blog-stills.ts
   photographs the live render into public/blog/<id>.webp for the prerender
   and feed readers. The three screen drafts of the post are not surfaces:
   they are drawn in HTML and CSS (components/blog/concepts). */
import type { Nested } from "./a2ui"

/** The expense form: nine components, the whole window of the desktop draft. */
export const EXPENSE_SURFACE: Nested = {
  id: `root`,
  component: `Card`,
  props: { title: `Log an expense`, description: `a form to log an expense, with a place to drop the receipt` },
  children: [
    {
      id: `form`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm`, align: `stretch` },
      children: [
        { id: `amount`, component: `NumberField`, props: { label: `Amount`, value: 42, precision: 2, unit: `€` } },
        {
          id: `category`,
          component: `Select`,
          props: {
            label: `Category`,
            value: `travel`,
            options: [
              { value: `travel`, label: `Travel` },
              { value: `meals`, label: `Meals` },
              { value: `office`, label: `Office` },
            ],
          },
        },
        { id: `date`, component: `DatePicker`, props: { label: `Date`, value: `2026-10-24` } },
        { id: `receipt`, component: `FileUpload`, props: { label: `Receipt`, hint: `Drop the receipt here`, accept: `image/*,application/pdf` } },
        {
          id: `actions`,
          component: `Stack`,
          props: { direction: `horizontal`, gap: `xs`, align: `center`, justify: `end` },
          children: [
            { id: `cancel`, component: `Button`, props: { label: `Cancel`, variant: `ghost`, size: `sm` } },
            { id: `save`, component: `Button`, props: { label: `Save`, variant: `default`, size: `sm` } },
          ],
        },
      ],
    },
  ],
}

/** "Agents get their own seat": the desktop's agent chat (Super+Space), one
 *  request done in the background, written as a core-catalog tree. */
export const DESKTOP_CHAT_SURFACE: Nested = {
  id: `root`,
  component: `Card`,
  props: { title: `Your agent`, description: `local · Claude Code` },
  children: [
    {
      id: `feed`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm`, align: `stretch` },
      children: [
        { id: `ask`, component: `MessageBubble`, props: { side: `out`, text: `Download my September payouts from the bank and put them in my tax folder.`, time: `14:02` } },
        {
          id: `trace`,
          component: `Trace`,
          props: { size: `sm` },
          children: [
            { id: `t1`, component: `TraceItem`, props: { title: `Borrowed your Brave window into agent space 1`, size: `sm`, status: `done` } },
            { id: `t2`, component: `TraceItem`, props: { title: `Clicked Export CSV on the payouts page`, description: `delivered, verified`, size: `sm`, status: `done` } },
            { id: `t3`, component: `TraceItem`, props: { title: `Moved payouts-2026-09.csv to ~/Steuer/2026-09`, size: `sm`, status: `done` } },
            { id: `t4`, component: `TraceItem`, props: { title: `Gave Brave back to your screen`, size: `sm`, status: `running` } },
          ],
        },
        { id: `reply`, component: `MessageBubble`, props: { side: `in`, author: `agent`, text: `Done, without touching your mouse. The CSV is in **~/Steuer/2026-09**.`, time: `14:03` } },
        { id: `composer`, component: `Composer`, props: { placeholder: `Ask, or hold F13 to talk` } },
      ],
    },
  ],
}

/* The playground's starter examples: one per input form it accepts. */
import demo from "@exponential-at/ui/fixtures/demo-surface.json"
import { CORE_CATALOG, createSurface, flatten, replayChunks, type Nested } from "./a2ui"

const BASIC_CATALOG = `https://a2ui.org/specification/v0_9/basic_catalog.json`

export interface Example {
  id: string
  label: string
  blurb: string
  source: string
}

const jsonl = (messages: unknown[]) => messages.map((m) => JSON.stringify(m)).join(`\n`)

/** The home demo as the stream an agent sends: JSONL, one message a line. */
const demoReplay = jsonl(replayChunks(`release`, flatten(demo as unknown as Nested), 4))

/** A form in the NESTED authoring form (what the fixtures use). */
const form: Nested = {
  id: `signup`,
  component: `Card`,
  props: { title: `Join the beta`, description: `We will email you when your seat is ready.`, padded: true },
  children: [
    { id: `name`, component: `Input`, props: { label: `Name`, name: `name`, placeholder: `Ada Lovelace`, value: { path: `/form/name` } } },
    { id: `email`, component: `Input`, props: { label: `Email`, name: `email`, type: `email`, placeholder: `ada@example.com`, value: { path: `/form/email` }, checks: [{ condition: { call: `email`, args: { value: { path: `/form/email` } } }, message: `Enter a valid email` }] } },
    { id: `platform`, component: `Select`, props: { label: `Platform`, name: `platform`, options: [{ label: `Web`, value: `web` }, { label: `iOS`, value: `ios` }, { label: `Android`, value: `android` }, { label: `Desktop`, value: `desktop` }], value: `web` } },
    { id: `news`, component: `Switch`, props: { label: `Product news`, name: `news`, checked: true, description: `At most one email a month.` } },
    {
      id: `actions`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, justify: `end` },
      children: [
        { id: `cancel`, component: `Button`, props: { label: `Not now`, variant: `ghost` } },
        { id: `submit`, component: `Button`, props: { label: `Request a seat` }, on: { press: { event: { name: `signup`, context: { name: { path: `/form/name` }, email: { path: `/form/email` } } } } } },
      ],
    },
  ],
}

/** A data-driven list: a template child per item of the data model. */
const list = [
  createSurface(`inbox`, CORE_CATALOG),
  {
    version: `v0.9`,
    updateComponents: {
      surfaceId: `inbox`,
      components: [
        { id: `root`, component: `Group`, title: `Inbox`, footer: `3 unread`, children: { componentId: `row`, path: `/items` } },
        { id: `row`, component: `ListRow`, title: { path: `title` }, subtitle: { path: `who` }, meta: { path: `when` }, icon: `inbox`, chevron: true },
      ],
    },
  },
  {
    version: `v0.9`,
    updateDataModel: {
      surfaceId: `inbox`,
      path: `/items`,
      value: [
        { title: `Release 0.18.84 shipped`, who: `deploy bot`, when: `2m` },
        { title: `Review: theme builder import`, who: `Danny`, when: `1h` },
        { title: `Flaky test in the Compose suite`, who: `CI`, when: `3h` },
      ],
    },
  },
]

/** An A2UI BASIC catalog surface: mapped onto the core on the way in. */
const basic = [
  createSurface(`basic`, BASIC_CATALOG),
  {
    version: `v0.9`,
    updateComponents: {
      surfaceId: `basic`,
      components: [
        { id: `root`, component: `Column`, children: [`title`, `body`, `field`, `agree`, `row`] },
        { id: `title`, component: `Text`, text: `Basic catalog`, variant: `h2` },
        { id: `body`, component: `Text`, text: `Row, Column, TextField, CheckBox and a Button with a Text child: A2UI's basic catalog, mapped onto the core.` },
        { id: `field`, component: `TextField`, label: `Your name`, value: { path: `/name` } },
        { id: `agree`, component: `CheckBox`, label: `Send me updates`, value: true },
        { id: `row`, component: `Row`, children: [`ok`, `more`] },
        { id: `ok`, component: `Button`, child: `okText`, variant: `primary`, action: { event: { name: `submit` } } },
        { id: `okText`, component: `Text`, text: `Submit` },
        { id: `more`, component: `Button`, child: `moreIcon`, variant: `borderless`, action: { event: { name: `more` } } },
        { id: `moreIcon`, component: `Icon`, name: `settings` },
      ],
    },
  },
]

export const EXAMPLES: Example[] = [
  { id: `demo`, label: `Release report (stream)`, blurb: `The home page demo as JSONL: createSurface, then growing updateComponents.`, source: demoReplay },
  { id: `form`, label: `Sign-up form (nested)`, blurb: `A nested node: inputs bound to the data model, a check, a press action.`, source: JSON.stringify(form, null, 2) },
  { id: `list`, label: `Inbox list (data-driven)`, blurb: `A template child list over /items, filled by updateDataModel.`, source: JSON.stringify(list, null, 2) },
  { id: `basic`, label: `A2UI basic catalog`, blurb: `A surface in A2UI's own basic catalog, mapped onto the core.`, source: JSON.stringify(basic, null, 2) },
]

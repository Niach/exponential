import {
  AssigneePicker,
  PickerItemBody,
  PickerList,
  PickerTrigger,
  UserAvatar,
  assigneePickerItems,
} from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* The email is the row's second line AND a search keyword — two people can
   share a name, and the id is what `value` carries either way. */
const MEMBERS = [
  { id: `mina`, name: `Mina Kay`, email: `mina@example.com` },
  { id: `jonas`, name: `Jonas Stern`, email: `jonas@example.com` },
  { id: `sam`, name: `Sam Lee`, email: `sam@example.com` },
]

export const entry: StyleguideEntry = {
  id: `picker-assignee`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Assignee picker`,
  blurb: `The team's members by AVATAR, name and email — the issue properties, the create-issue dialog, the list row's context menu and the bulk bar. The avatar is the one row body the primitive does not draw itself (a picker glyph is an icon, never a photo), and the email is both the muted second line and a search term. \`Unassigned\` is a row like any other and reports \`null\`: no \`__unassign__\` sentinel exists anywhere. On a solo team the control is hidden entirely — there is nobody to assign to.`,
  status: typedPickerStatus(`AssigneePicker`, {
    web: `assignee-picker.tsx`,
    desktop: `assignee_picker.rs`,
    ios: `AssigneePicker.swift`,
    android: `AssigneePicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <AssigneePicker
          members={MEMBERS}
          allowsNone
          value="mina"
          onChange={noop}
          trigger={
            <PickerTrigger
              variant="pill"
              label="Assignee"
              value={
                <span className="flex items-center gap-1.5">
                  <UserAvatar size={16} user={MEMBERS[0]} />
                  Mina Kay
                </span>
              }
            />
          }
        />
      }
      surface={
        <PickerList
          mode="single"
          search
          searchPlaceholder="Search people…"
          items={assigneePickerItems(MEMBERS)}
          value="mina"
          onChange={noop}
          noneLabel="Unassigned"
          onNone={noop}
          // Exactly what `AssigneePicker` hands the primitive: the avatar,
          // then the primitive's own row body.
          renderItem={(item) => {
            const member = MEMBERS.find((row) => row.id === item.value)
            return (
              <>
                {member && <UserAvatar size={20} user={member} />}
                <PickerItemBody item={item} />
              </>
            )
          }}
        />
      }
    />
  ),
}

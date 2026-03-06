import { expect, test as base } from "@playwright/test"
import { cleanupNamespace } from "./helpers/db"

export interface TestUser {
  email: string
  initials: string
  name: string
  password: string
}

export interface DueDateFixture {
  dataDay: string
  isoDate: string
  text: string
}

export interface AppFixture {
  dueDate: DueDateFixture
  emailPrefix: string
  issueDescription: string
  issueTitle: string
  labelColor: string
  labelName: string
  member: TestUser
  namespace: string
  owner: TestUser
  projectName: string
  projectPrefix: string
  projectSlug: string
  updatedIssueDescription: string
  updatedIssueTitle: string
}

function buildNamespace() {
  return `pw-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function getInitials(value: string) {
  return value
    .split(` `)
    .map((part) => part[0] ?? ``)
    .join(``)
    .toUpperCase()
    .slice(0, 2)
}

function derivePrefix(name: string) {
  return name
    .split(/[\s-_]+/)
    .map((word) => word[0] ?? ``)
    .join(``)
    .toUpperCase()
    .slice(0, 5)
}

function slugify(name: string) {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, ``)
    .replace(/[\s_]+/g, `-`)
    .replace(/-+/g, `-`)
    .replace(/^-|-$/g, ``)
}

function formatDateOnly(date: Date) {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, `0`)
  const day = String(date.getDate()).padStart(2, `0`)

  return `${year}-${month}-${day}`
}

function buildDueDate(): DueDateFixture {
  const now = new Date()
  const lastDay = new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate()
  const day = now.getDate() < lastDay ? now.getDate() + 1 : now.getDate()
  const date = new Date(now.getFullYear(), now.getMonth(), day)

  return {
    dataDay: date.toLocaleDateString(`en-US`),
    isoDate: formatDateOnly(date),
    text: date.toLocaleDateString(`en-US`, { month: `short`, day: `numeric` }),
  }
}

function buildUser(role: `owner` | `member`, emailPrefix: string, namespace: string) {
  const name = `${role === `owner` ? `Owner` : `Member`} ${namespace}`

  return {
    email: `${emailPrefix}.${role}@example.com`,
    initials: getInitials(name),
    name,
    password: `Pass-${namespace}-secret`,
  } satisfies TestUser
}

function buildAppFixture(): AppFixture {
  const namespace = buildNamespace()
  const emailPrefix = `e2e+${namespace}`
  const projectName = `Alpha ${namespace}`

  return {
    dueDate: buildDueDate(),
    emailPrefix,
    issueDescription: `Initial issue description for ${namespace}`,
    issueTitle: `Issue ${namespace}`,
    labelColor: `#ef4444`,
    labelName: `Label ${namespace}`,
    member: buildUser(`member`, emailPrefix, namespace),
    namespace,
    owner: buildUser(`owner`, emailPrefix, namespace),
    projectName,
    projectPrefix: derivePrefix(projectName),
    projectSlug: slugify(projectName),
    updatedIssueDescription: `Updated issue description for ${namespace}`,
    updatedIssueTitle: `Updated issue ${namespace}`,
  }
}

type Fixtures = {
  app: AppFixture
}

export const test = base.extend<Fixtures>({
  app: async ({}, use) => {
    const app = buildAppFixture()

    try {
      await use(app)
    } finally {
      await cleanupNamespace(app.emailPrefix)
    }
  },
})

export { expect }

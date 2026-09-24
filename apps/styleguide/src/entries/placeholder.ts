import { escapeHtml } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

/** The one-line body every placeholder entry renders until its owner fills it. */
export function placeholderMarkup(entry: Pick<StyleguideEntry, `owner` | `title`>): string {
  return `<p class="blurb">${escapeHtml(entry.title)} — placeholder, ${escapeHtml(entry.owner)} fills this entry.</p>`
}

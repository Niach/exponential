import fixture from "../fixtures/via-app.json" with { type: "json" }

// VAPP-229 (fixtures/via-app.json): the "via <app>" caption of every row an
// app caused (issues, sessions, records; comments read reporter-reply.json's
// `captions.viaApp`, the same string). TS reference; desktop domain::via_app,
// iOS ExpCore ViaApp.swift and Android domain/ViaApp.kt replay the cases.

export const VIA_APP_COPY = fixture.copy

/** "via {app}", or null without a (resolved) app name. */
export function viaAppCaption(appName: string | null | undefined): string | null {
  return appName ? VIA_APP_COPY.viaApp.replace(`{app}`, appName) : null
}

/** `text` with the caption appended after the separator (the text alone
 *  without an app). */
export function withViaApp(text: string, appName: string | null | undefined): string {
  const caption = viaAppCaption(appName)
  return caption ? `${text}${VIA_APP_COPY.separator}${caption}` : text
}

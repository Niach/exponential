// Every user-facing string of the widget (FEED-113), in one typed table.
// `de` is typed by `en`'s shape, so a key missing from either fails the
// typecheck. Strings that come from the server config (board-configured
// launcher label, custom field labels, label names) are NOT here: they render
// as the owner wrote them.

export type WidgetLocale = `en` | `de`
// `auto` = the host page's <html lang>, then navigator.language.
export type WidgetLocalePreference = WidgetLocale | `auto`

const en = {
  // Launcher
  launcherLabel: `Feedback`,
  launcherAriaLabel: `Send feedback`,
  // Panel chrome
  panelTitle: `Send feedback`,
  close: `Close`,
  poweredByPrefix: `Powered by`,
  // Screenshot slot
  screenshotAlt: `Screenshot of this page`,
  annotate: `Annotate`,
  retake: `Retake`,
  remove: `Remove`,
  screenshotFailed: `Screenshot couldn't be captured.`,
  screenshotPrompt: `Attach a screenshot of this page.`,
  tryAgain: `Try again`,
  takeScreenshot: `Take screenshot`,
  delayOff: `Off`,
  delaySeconds: (seconds: number) => `${seconds}s`,
  delayAriaLabel: (value: string) => `Screenshot delay: ${value}`,
  delayTitle: `Delay the capture to catch menus and popups`,
  // Attached pictures
  removeImage: (filename: string) => `Remove ${filename}`,
  addImage: `Add image`,
  addImageTitle: `Attach pictures — you can also drop or paste them`,
  imageWrongType: `Only image files can be attached.`,
  imageTooLarge: `Images must be 10 MB or smaller.`,
  imageTooMany: (max: number) => `You can attach up to ${max} images.`,
  // Form fields
  messageLabel: `What happened?`,
  messagePlaceholder: `Describe what you saw, or what you'd like to see.`,
  labelsLabel: `What is this about?`,
  optional: (label: string) => `${label} (optional)`,
  nameLabel: `Name`,
  namePlaceholder: `Your name`,
  emailLabel: `Email`,
  emailPlaceholder: `you@example.com`,
  // Submit button
  submit: `Send feedback`,
  sending: `Sending…`,
  preparingScreenshot: `Preparing screenshot…`,
  // Success card
  sentAriaLabel: `Feedback sent`,
  successTitle: `Thanks, your report is in.`,
  filedAs: `Filed as`,
  emailSent: `We emailed you a link to follow the conversation.`,
  emailNotSent: `We could not send the follow-up email; your report still reached the team.`,
  // Validation
  messageRequired: `Tell us what happened.`,
  nameRequired: `Your name is required.`,
  emailRequired: `Your email is required.`,
  invalidEmail: `Please enter a valid email address.`,
  fieldRequired: (label: string) => `Please fill in "${label}".`,
  responsesTooLong: `Your responses are too long to submit. Please shorten them.`,
  // Server / network errors
  originNotAllowed: `This site isn't allowed to send feedback. The site owner can add it under the widget's allowed domains.`,
  unknownKey: `This feedback widget isn't set up correctly. The site owner needs to check the widget key.`,
  widgetDisabled: `This widget is currently disabled.`,
  rateLimited: (wait: string | null) =>
    wait
      ? `Too many reports right now. Try again in ${wait}.`
      : `Too many reports right now. Try again later.`,
  requestTooLarge: `Your report is too large to send. Remove an image or the screenshot and try again.`,
  genericError: `Something went wrong. Please try again.`,
  networkError: `Network error. Please try again.`,
  // Retry-After wait, already rounded up
  seconds: (n: number) => (n === 1 ? `1 second` : `${n} seconds`),
  minutes: (n: number) => (n === 1 ? `1 minute` : `${n} minutes`),
  hours: (n: number) => (n === 1 ? `1 hour` : `${n} hours`),
  // Annotator
  annotatorAriaLabel: `Annotate screenshot`,
  toolRect: `Rectangle`,
  toolPen: `Free line`,
  toolArrow: `Arrow`,
  toolCrop: `Crop`,
  undo: `Undo`,
  clearAnnotations: `Clear annotations`,
  removeCrop: `Remove crop`,
  cancel: `Cancel`,
  done: `Done`,
}

export type WidgetMessages = typeof en

// One form throughout: Sie.
const de: WidgetMessages = {
  launcherLabel: `Feedback`,
  launcherAriaLabel: `Feedback senden`,
  panelTitle: `Feedback senden`,
  close: `Schließen`,
  poweredByPrefix: `Bereitgestellt von`,
  screenshotAlt: `Screenshot dieser Seite`,
  annotate: `Markieren`,
  retake: `Neu aufnehmen`,
  remove: `Entfernen`,
  screenshotFailed: `Der Screenshot konnte nicht erstellt werden.`,
  screenshotPrompt: `Hängen Sie einen Screenshot dieser Seite an.`,
  tryAgain: `Erneut versuchen`,
  takeScreenshot: `Screenshot aufnehmen`,
  delayOff: `Aus`,
  delaySeconds: (seconds: number) => `${seconds} s`,
  delayAriaLabel: (value: string) => `Verzögerung für den Screenshot: ${value}`,
  delayTitle: `Aufnahme verzögern, um Menüs und Pop-ups mit aufzunehmen`,
  removeImage: (filename: string) => `${filename} entfernen`,
  addImage: `Bild hinzufügen`,
  addImageTitle: `Bilder anhängen – Sie können sie auch hineinziehen oder einfügen`,
  imageWrongType: `Es können nur Bilddateien angehängt werden.`,
  imageTooLarge: `Bilder dürfen höchstens 10 MB groß sein.`,
  imageTooMany: (max: number) => `Sie können bis zu ${max} Bilder anhängen.`,
  messageLabel: `Was ist passiert?`,
  messagePlaceholder: `Beschreiben Sie, was Sie gesehen haben oder was Sie sich wünschen.`,
  labelsLabel: `Worum geht es?`,
  optional: (label: string) => `${label} (optional)`,
  nameLabel: `Name`,
  namePlaceholder: `Ihr Name`,
  emailLabel: `E-Mail`,
  emailPlaceholder: `sie@beispiel.de`,
  submit: `Feedback senden`,
  sending: `Wird gesendet…`,
  preparingScreenshot: `Screenshot wird vorbereitet…`,
  sentAriaLabel: `Feedback gesendet`,
  successTitle: `Danke, Ihre Meldung ist angekommen.`,
  filedAs: `Erfasst als`,
  emailSent: `Wir haben Ihnen einen Link per E-Mail geschickt, über den Sie die Unterhaltung verfolgen können.`,
  emailNotSent: `Die Bestätigungs-E-Mail konnte nicht gesendet werden; Ihre Meldung ist trotzdem beim Team angekommen.`,
  messageRequired: `Beschreiben Sie bitte, was passiert ist.`,
  nameRequired: `Bitte geben Sie Ihren Namen an.`,
  emailRequired: `Bitte geben Sie Ihre E-Mail-Adresse an.`,
  invalidEmail: `Bitte geben Sie eine gültige E-Mail-Adresse ein.`,
  fieldRequired: (label: string) => `Bitte füllen Sie „${label}“ aus.`,
  responsesTooLong: `Ihre Angaben sind zu lang zum Senden. Bitte kürzen Sie sie.`,
  originNotAllowed: `Diese Website darf kein Feedback senden. Der Betreiber der Website kann sie bei den erlaubten Domains des Widgets hinzufügen.`,
  unknownKey: `Dieses Feedback-Widget ist nicht richtig eingerichtet. Der Betreiber der Website muss den Widget-Schlüssel prüfen.`,
  widgetDisabled: `Dieses Widget ist derzeit deaktiviert.`,
  rateLimited: (wait: string | null) =>
    wait
      ? `Gerade gehen zu viele Meldungen ein. Versuchen Sie es in ${wait} erneut.`
      : `Gerade gehen zu viele Meldungen ein. Versuchen Sie es später erneut.`,
  requestTooLarge: `Ihre Meldung ist zu groß zum Senden. Entfernen Sie ein Bild oder den Screenshot und versuchen Sie es erneut.`,
  genericError: `Etwas ist schiefgelaufen. Bitte versuchen Sie es erneut.`,
  networkError: `Netzwerkfehler. Bitte versuchen Sie es erneut.`,
  seconds: (n: number) => (n === 1 ? `1 Sekunde` : `${n} Sekunden`),
  minutes: (n: number) => (n === 1 ? `1 Minute` : `${n} Minuten`),
  hours: (n: number) => (n === 1 ? `1 Stunde` : `${n} Stunden`),
  annotatorAriaLabel: `Screenshot markieren`,
  toolRect: `Rechteck`,
  toolPen: `Freihandlinie`,
  toolArrow: `Pfeil`,
  toolCrop: `Zuschneiden`,
  undo: `Rückgängig`,
  clearAnnotations: `Markierungen löschen`,
  removeCrop: `Zuschnitt entfernen`,
  cancel: `Abbrechen`,
  done: `Fertig`,
}

export const widgetMessages: Record<WidgetLocale, WidgetMessages> = { en, de }

export function isLocalePreference(
  value: unknown
): value is WidgetLocalePreference {
  return value === `en` || value === `de` || value === `auto`
}

// `de`, `de-AT`, `de_CH` … → German; anything else → English.
function localeFromTag(tag: string | null | undefined): WidgetLocale | null {
  if (typeof tag !== `string` || !tag.trim()) return null
  return tag.trim().toLowerCase().startsWith(`de`) ? `de` : `en`
}

// `auto`: the host page's declared language wins (a German app on an
// English browser stays German), then the browser's.
export function detectLocale(): WidgetLocale {
  const fromPage =
    typeof document !== `undefined`
      ? localeFromTag(document.documentElement?.lang)
      : null
  if (fromPage) return fromPage
  const fromBrowser =
    typeof navigator !== `undefined` ? localeFromTag(navigator.language) : null
  return fromBrowser ?? `en`
}

// setLocale() > init({ locale }) > auto, mirroring the theme precedence.
export function resolveLocale(
  override: WidgetLocalePreference | null | undefined,
  option: WidgetLocalePreference | null | undefined
): WidgetLocale {
  const preference = isLocalePreference(override)
    ? override
    : isLocalePreference(option)
      ? option
      : `auto`
  return preference === `auto` ? detectLocale() : preference
}

export function messagesFor(state: {
  localeOverride?: WidgetLocalePreference | null
  options: { locale?: WidgetLocalePreference }
}): WidgetMessages {
  return widgetMessages[resolveLocale(state.localeOverride, state.options.locale)]
}

// A Retry-After wait in words, rounded UP to the unit that reads naturally:
// 30 → "30 seconds", 60 → "1 minute", 450 → "8 minutes", 3600 → "1 hour".
export function formatWait(seconds: number, messages: WidgetMessages): string {
  const total = Math.max(1, Math.ceil(seconds))
  if (total < 60) return messages.seconds(total)
  if (total < 3600) return messages.minutes(Math.ceil(total / 60))
  return messages.hours(Math.ceil(total / 3600))
}

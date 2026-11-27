// #EXP-2191: text a device sends back (a job's error, a compose outcome) may
// quote a URL it failed on; a presigned bucket URL's query IS a credential
// (X-Amz-Credential, X-Amz-Signature). Every such text drops URL queries
// before it is stored or logged: the scheme, host and path stay.
const URL_WITH_QUERY = /(\bhttps?:\/\/[^\s?#"'<>]+)[?#][^\s"'<>)]*/g

/** `text` with the query and fragment of every http(s) URL in it removed. */
export function redactUrlQueries(text: string): string
export function redactUrlQueries(text: string | null): string | null
export function redactUrlQueries(text: string | null): string | null {
  return text === null ? null : text.replace(URL_WITH_QUERY, `$1`)
}

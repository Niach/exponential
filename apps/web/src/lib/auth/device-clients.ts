// GHSA-q84f-53jg-9ppm: the device-code grant (RFC 8628) is open to any
// client unless `validateClient` narrows it, and the plugin's approval page
// shows neither who asks nor what it gets. Both answers come from here.
//
// The allow-list of device-code clients, keyed by `client_id`, valued by the
// name /auth/device shows above Approve. The only legit client is the desktop
// app + `exponential` CLI daemon: its id is `DEVICE_CLIENT_ID` in
// apps/desktop/crates/api/src/login.rs (keep the two in sync). An id missing
// here can neither start (/device/code) nor poll (/device/token): the plugin
// answers `invalid_client`.
export const DEVICE_CLIENTS: Readonly<Record<string, string>> = {
  "exponential-cli": `Exponential desktop or CLI`,
}

/** `deviceAuthorization({ validateClient })`: only allow-listed ids. */
export function isAllowedDeviceClient(clientId: string): boolean {
  return Object.hasOwn(DEVICE_CLIENTS, clientId)
}

/** The human name of an allow-listed client; null for anything else. */
export function deviceClientName(clientId: string | null | undefined): string | null {
  return clientId && isAllowedDeviceClient(clientId) ? DEVICE_CLIENTS[clientId]! : null
}

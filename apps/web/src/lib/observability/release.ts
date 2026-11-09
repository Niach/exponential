import webPackage from "../../../package.json" with { type: "json" }

// #LOGS-2: the release every web report files under, client and server alike.
export const WEB_RELEASE = `exponential-web@${webPackage.version}`

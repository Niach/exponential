import { loadConfig } from "../config"
import {
  loadAccessToken,
  logoutGithub,
  runDeviceFlow,
} from "../github-auth"
import { getAuthedUser, listAccessibleRepos } from "../github-api"
import {
  clearGithubIdentity,
  reportGithubIdentity,
} from "../exponential-api"

async function refreshIdentityOnServer(token: string): Promise<{
  login: string
  repoCount: number
}> {
  const config = await loadConfig()
  const user = await getAuthedUser(token)
  const repos = await listAccessibleRepos(token)
  await reportGithubIdentity(config, user.login, repos)
  return { login: user.login, repoCount: repos.length }
}

export async function runGithubLogin(): Promise<void> {
  const config = await loadConfig()
  const clientId = config.exponential.githubOauthClientId
  if (!clientId) {
    console.error(
      `No GitHub OAuth client ID configured for this Exponential instance.`
    )
    console.error(
      `Ask the instance admin to set EXPONENTIAL_GITHUB_OAUTH_CLIENT_ID, then re-run \`companion setup\`.`
    )
    process.exit(1)
  }

  await runDeviceFlow({
    clientId,
    onPrompt: ({ verificationUri, userCode }) => {
      console.log(``)
      console.log(`Open this URL in a browser:`)
      console.log(`  ${verificationUri}`)
      console.log(``)
      console.log(`And enter this code:`)
      console.log(`  ${userCode}`)
      console.log(``)
      console.log(`Waiting for authorization…`)
    },
  })

  const result = await loadAccessToken()
  if (!result) {
    throw new Error(`Token storage failed; aborting.`)
  }
  const info = await refreshIdentityOnServer(result.token)
  console.log(``)
  console.log(
    `Authenticated as @${info.login}. ${info.repoCount} repos accessible.`
  )
  console.log(`Link them to projects from the workspace settings page.`)
}

export async function runGithubLogout(): Promise<void> {
  await logoutGithub()
  try {
    const config = await loadConfig()
    await clearGithubIdentity(config)
  } catch (e) {
    console.error(
      `Local token removed; could not clear server-side identity: ${e instanceof Error ? e.message : e}`
    )
  }
  console.log(`Logged out of GitHub.`)
}

export async function runGithubStatus(): Promise<void> {
  const result = await loadAccessToken().catch((e) => {
    console.error(`Failed to load GitHub token:`, e instanceof Error ? e.message : e)
    process.exit(1)
  })
  if (!result) {
    console.log(`Not authenticated. Run \`companion github login\`.`)
    process.exit(1)
  }
  try {
    const info = await refreshIdentityOnServer(result.token)
    console.log(
      `Authenticated as @${info.login}. ${info.repoCount} repos accessible.`
    )
  } catch (e) {
    console.error(
      `Token loaded but GitHub call failed: ${e instanceof Error ? e.message : e}`
    )
    process.exit(1)
  }
}

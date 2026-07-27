/**
 * Remotion configuration for the ClosedLoop movie (studio + renders only —
 * the site embeds the composition through @remotion/player and never reads
 * this file). All options: https://remotion.dev/docs/config
 */

import { Config } from "@remotion/cli/config"

Config.setVideoImageFormat("jpeg")
Config.setOverwriteOutput(true)

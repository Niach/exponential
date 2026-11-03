/* The docs sidebar, one file per group so each area edits only its own. */
import { BUILD } from "./build.ts"
import { EAPPS } from "./eapps.ts"
import { GET_STARTED } from "./get-started.ts"
import { LIBRARIES_GROUP } from "./libraries.ts"
import { REFERENCE } from "./reference.ts"
import { USING } from "./using.ts"

export const SIDEBAR = [GET_STARTED, USING, EAPPS, BUILD, LIBRARIES_GROUP, REFERENCE]

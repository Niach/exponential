import { contract } from "@exp/domain-contract"
import { classifyPendingFile } from "@/lib/pending-images"

// Release train 2026-10-10 (F6): a device that localizes non-image FILE
// attachments (`[name](/api/attachments/<id>)` lines in start prompts and
// steer messages) advertises the `steer-files` cap in its synced `caps`
// (desktop IDE + CLI daemon 0.14.66). Against an older device the server
// refuses a start/message carrying a file with the sentence below, so the
// composers ×4 never stage one: the "Add file or image" pick accepts IMAGES
// only and says why. Web has no local-machine case. The rule is pure so
// both the start composer and the steer composer share it.

/** The device cap that says files (not just images) can be attached. */
export const STEER_FILES_CAP: string = contract.codingSession.steerFilesCap

/** The server's refusal, word for word (×4 toast/notice). */
export const FILES_NEED_NEWER_DEVICE: string = contract.composerUi.filesNeedNewerDevice

/** Whether the chosen device takes file attachments. No device (none picked
 *  yet, no synced row) = nothing to judge: files stay allowed and the
 *  server's gate has the last word. */
export function deviceAcceptsFiles(
  device: { caps?: readonly string[] | null } | null | undefined
): boolean {
  if (!device || device.caps == null) return true
  return device.caps.includes(STEER_FILES_CAP)
}

/** The pick, gated: with files refused, every non-image attachment is
 *  dropped (`droppedFiles` counts them, for the notice); oversized or empty
 *  picks pass through so the usual size toast still fires. */
export function gateFilesForDevice(
  files: readonly File[],
  acceptsFiles: boolean
): { files: File[]; droppedFiles: number } {
  if (acceptsFiles) return { files: [...files], droppedFiles: 0 }
  let droppedFiles = 0
  const kept = files.filter((file) => {
    if (classifyPendingFile(file) !== `file`) return true
    droppedFiles++
    return false
  })
  return { files: kept, droppedFiles }
}

/** The file input's `accept`: images only while files are refused, else
 *  everything (undefined = no attribute). */
export function filePickAccept(acceptsFiles: boolean): string | undefined {
  return acceptsFiles ? undefined : `image/*`
}

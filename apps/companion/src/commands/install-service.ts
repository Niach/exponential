import { platform } from "node:os"
import { writeSystemdUnit, systemdUnitPath } from "../supervisor/systemd"

export async function runInstallService(): Promise<void> {
  const os = platform()
  if (os === `linux`) {
    const path = await writeSystemdUnit()
    console.log(`Wrote ${path}`)
    console.log(``)
    console.log(`Activate with:`)
    console.log(`  systemctl --user daemon-reload`)
    console.log(`  systemctl --user enable --now exponential-companion`)
    console.log(`To survive logout:`)
    console.log(`  sudo loginctl enable-linger $USER`)
    console.log(`Stop with:`)
    console.log(`  systemctl --user disable --now exponential-companion`)
    console.log(`Logs:`)
    console.log(`  journalctl --user -u exponential-companion -f`)
    return
  }
  console.error(
    `install-service: unsupported platform ${os}. Linux only at MVP.`
  )
  process.exit(1)
  // Use systemdUnitPath here so it's referenced (silences TS warning).
  void systemdUnitPath
}

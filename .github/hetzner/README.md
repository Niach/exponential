# hetzner-1 disk

hetzner-1 (75 GB) carries prod web + Postgres, the package repo and GlitchTip
on one disk. On 2026-11-12 GlitchTip's debug-file uploads filled it to 100%
(#ERR-15). What keeps it from filling again:

| What | Where | When |
| --- | --- | --- |
| Upload only what symbolication reads: an ELF with DWARF goes up as its zlib-compressed debug sections (`objcopy --only-keep-debug --compress-debug-sections=zlib`, same build-id, identical lookups in GlitchTip's `symbolic`) | `.github/scripts/glitchtip-debug-files.sh` | every upload |
| Delete upload chunks an hour after the upload (GlitchTip waits 24 h) | Coolify service `glitchtip` → scheduled task `glitchtip-chunks`: `python /ops/glitchtip_maintenance.py chunks` in `worker` | hourly, :07 |
| Keep the newest 5 debug files per family (project, file name without cargo's `-<hash>`, arch, ELF/Mach-O/PDB), nothing younger than 2 days goes | scheduled task `glitchtip-dif-retention`: `python /ops/glitchtip_maintenance.py retention` | daily, 03:40 UTC |
| Alert at 80% used | Coolify → server `hetzner-1`: disk-usage notification threshold 80, checked `*/10 * * * *`, mailed to the team's members | every 10 min, at most one mail a day |
| Build cache ≤ 5 GB, dangling images; first aid at 90% (chunks now, unused images > 1 day, journal to 100 MB) | `disk-keeper.sh`, root systemd timer `exp-disk-keeper` (`install-disk-keeper.sh`) | every 15 min |
| Journal ≤ 200 MB | `/etc/systemd/journald.conf.d/exp-size.conf` (`install-disk-keeper.sh`) | always |
| Container logs 3 × 10 MB | `/etc/docker/daemon.json` | always |
| Unused images | Coolify → server `hetzner-1` → Docker cleanup (forced) | daily, 00:00 |

`glitchtip_maintenance.py` reaches the worker as a Coolify file storage
(`/data/coolify/services/<glitchtip>/ops/glitchtip_maintenance.py`, declared
as a bind in the service compose). To change it: edit it here, update the
storage's content in Coolify (Service → worker → Storages, or `coolify service
storage update`), restart the service. `--dry-run` prints what it would
delete:

```
ssh root@<hetzner-1> docker exec worker-<glitchtip> python /ops/glitchtip_maintenance.py retention --dry-run
```

`GLITCHTIP_DIF_KEEP` (5) and `GLITCHTIP_DIF_MIN_AGE_DAYS` (2) on the worker
tune the retention. Five releases cover the desktop versions events still
arrive from (a week at ~one release a day); an older build's crash keeps its
addresses, unsymbolicated.

Budget (2026-11-12): one desktop + CLI release uploads 2.8 GB (was 4.4 GB:
the Linux binaries went from 1.1 + 2 × 0.48 GB to 0.24 + 2 × 0.1 GB; the
macOS dSYMs, 2 × 0.73 GB, and the Windows PDB, 0.58 GB, stay whole: GlitchTip
reads no compressed Mach-O or PDB and Breakpad loses inline frames), an eDE
release 0.27 GB (was 1.2 GB), Android 0.2 GB. Five of each ≈ 16 GB of debug
files next to ~33 GB of everything else.

Logs: `journalctl -t exp-disk-keeper`; the scheduled tasks' runs are under the
service's Scheduled Tasks in Coolify (a failure notifies like the disk alert).

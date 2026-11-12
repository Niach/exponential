"""#ERR-15: GlitchTip debug-file housekeeping on hetzner-1.

Mounted into the GlitchTip worker as /ops/glitchtip_maintenance.py (a Coolify
file storage) and run by the service's Coolify scheduled tasks:

  python /ops/glitchtip_maintenance.py chunks      # hourly
  python /ops/glitchtip_maintenance.py retention   # daily
  ... --dry-run                                     # report, delete nothing

chunks: the 32 MiB upload chunks of an assembled multi-chunk file are
FileBlobs no File points at. GlitchTip's own maintenance deletes those only
after 24 h, which doubled each release's footprint for a day; this deletes
them after an hour (no sentry-cli run sits an hour between its chunk upload
and its assemble call).

retention: GlitchTip keeps debug files GLITCHTIP_FILE_RETENTION_DAYS (30). At
~one desktop release a day that outgrows the disk, so per family (project,
file name without cargo's -<hash> suffix, arch, format) only the newest KEEP
uploads stay (GLITCHTIP_DIF_KEEP, default 5 = the last five releases), and
nothing younger than GLITCHTIP_DIF_MIN_AGE_DAYS (2) goes. The dropped files'
blobs are deleted by the chunks pass that follows.
"""

import asyncio
import os
import re
import sys
from collections import defaultdict
from datetime import timedelta

sys.path.insert(0, "/code")
os.environ.setdefault("DJANGO_SETTINGS_MODULE", "glitchtip.settings")

import django  # noqa: E402

django.setup()

from django.db.models import Exists, OuterRef, Sum  # noqa: E402
from django.utils.timezone import now  # noqa: E402

from apps.difs.models import DebugInformationFile  # noqa: E402
from apps.files.maintenance import _delete_file_blobs  # noqa: E402
from apps.files.models import File, FileBlob  # noqa: E402
from apps.sourcecode.models import DebugSymbolBundle  # noqa: E402

CHUNK_GRACE = timedelta(hours=1)
KEEP = int(os.environ.get("GLITCHTIP_DIF_KEEP", "5"))
MIN_AGE = timedelta(days=int(os.environ.get("GLITCHTIP_DIF_MIN_AGE_DAYS", "2")))
HASH_SUFFIX = re.compile(r"-[0-9a-f]{16}$")


def gib(n):
    return f"{(n or 0) / 2**30:.2f} GiB"


def family(dif):
    data = dif.data or {}
    if data.get("symbol_type") == "proguard":
        return (dif.project_id, "proguard")
    code_id = data.get("code_id") or ""
    fmt = {40: "elf", 32: "macho"}.get(len(code_id), "other")
    return (dif.project_id, HASH_SUFFIX.sub("", dif.name), data.get("arch"), fmt)


def chunks(dry_run):
    orphans = FileBlob.objects.filter(created__lt=now() - CHUNK_GRACE).exclude(
        Exists(File.objects.filter(blob_id=OuterRef("id")))
    )
    count, size = orphans.count(), orphans.aggregate(s=Sum("size"))["s"]
    print(f"chunks: {count} unreferenced blobs older than 1 h, {gib(size)}")
    if count and not dry_run:
        asyncio.run(_delete_file_blobs(orphans, "unreferenced (ERR-15)"))


def retention(dry_run):
    by_family = defaultdict(list)
    for dif in DebugInformationFile.objects.select_related("file").order_by("-created"):
        by_family[family(dif)].append(dif)
    cutoff = now() - MIN_AGE
    drop = [
        dif
        for difs in by_family.values()
        for dif in difs[KEEP:]
        if dif.created < cutoff
    ]
    size = sum(dif.file.size for dif in drop)
    print(f"retention: keep {KEEP} per family, {len(by_family)} families, dropping {len(drop)} files, {gib(size)}")
    for dif in drop:
        print(f"  {dif.created:%Y-%m-%d %H:%M} {dif.project_id} {dif.name} {dif.data.get('arch')} {gib(dif.file.size)}")
    if dry_run or not drop:
        return
    file_ids = {dif.file_id for dif in drop}
    DebugInformationFile.objects.filter(id__in=[dif.id for dif in drop]).delete()
    File.objects.filter(id__in=file_ids).exclude(
        Exists(DebugInformationFile.objects.filter(file_id=OuterRef("id")))
    ).exclude(Exists(DebugSymbolBundle.objects.filter(file_id=OuterRef("id")))).exclude(
        Exists(DebugSymbolBundle.objects.filter(sourcemap_file_id=OuterRef("id")))
    ).delete()


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    dry_run = "--dry-run" in sys.argv
    if command == "chunks":
        chunks(dry_run)
    elif command == "retention":
        retention(dry_run)
        chunks(dry_run)
    else:
        sys.exit("usage: glitchtip_maintenance.py chunks|retention [--dry-run]")

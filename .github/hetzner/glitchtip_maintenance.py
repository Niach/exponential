"""#ERR-15 / #EXP-2147: GlitchTip debug-file housekeeping on hetzner-1.

Mounted into the GlitchTip worker as /ops/glitchtip_maintenance.py (a Coolify
file storage) and run by the service's Coolify scheduled tasks:

  python /ops/glitchtip_maintenance.py chunks              # hourly
  python /ops/glitchtip_maintenance.py retention --apply   # daily

Without --apply `retention` only lists what it would delete; `chunks
--dry-run` likewise.

chunks: the 32 MiB upload chunks of an assembled multi-chunk file are
FileBlobs no File points at. GlitchTip's own maintenance deletes those only
after 24 h, which doubled each release's footprint for a day; this deletes
them after an hour (no sentry-cli run sits an hour between its chunk upload
and its assemble call). It is also what frees the bytes of the files
`retention` dropped.

retention, desktop + CLI + iOS + Android (#EXP-2147): a debug file is kept
while it belongs to a version at or above the server's CLIENT_MIN_VERSION
floor (GET /api/version, public) or to one of its platform's last 2 releases.
GlitchTip stores no version with a debug file, so a file belongs to the newest
release tag at its upload: a tag's upload always comes after the tag, so a
wrong guess is a LATER version, which is kept longer. sentry-cli does not
upload a file again whose build-id did not change, so the newest 2 of a
family (file name, arch, format) also stay whatever their age. Kept too:
every file whose platform or version is unknown, and everything younger than
VERSION_MIN_AGE. The WHOLE run refuses, deleting nothing, when a floor is
missing or no version, a platform has no release tags, or a platform would
lose all its files.

retention, eDE (it has no floor): the newest GLITCHTIP_DIF_KEEP (default 5;
prod sets 3 inline in the scheduled task's command) uploads per family, and
nothing younger than GLITCHTIP_DIF_MIN_AGE_DAYS (2).
"""

import asyncio
import json
import os
import re
import sys
import urllib.request
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone

CHUNK_GRACE = timedelta(hours=1)
KEEP = int(os.environ.get("GLITCHTIP_DIF_KEEP", "5"))
MIN_AGE = timedelta(days=int(os.environ.get("GLITCHTIP_DIF_MIN_AGE_DAYS", "2")))
VERSION_MIN_AGE = timedelta(hours=6)
KEEP_RELEASES = 2
HASH_SUFFIX = re.compile(r"-[0-9a-f]{16}$")

REPO_API = "https://api.github.com/repos/Niach/exponential"
VERSION_URL = "https://app.exponential.at/api/version"
VERSION_PLATFORMS = ("desktop", "cli", "ios", "android")
# The iOS app is the one platform without GitHub releases: its tags are dated
# one by one (the API allows 60 unauthenticated calls an hour).
IOS_TAGS = 20
# The binaries of the GlitchTip project `desktop`, which the CLI shares.
DESKTOP_FILES = {"exp-desktop": "desktop", "exp_desktop": "desktop", "exponential": "cli"}


@dataclass(frozen=True)
class Symbol:
    id: object
    project: str
    name: str
    family: tuple
    created: datetime
    size: int


def parse_version(text):
    """`0.14.183` as a tuple, None when it is not exactly three numbers."""
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", (text or "").strip())
    return tuple(int(part) for part in match.groups()) if match else None


def platform_of(symbol):
    """The version platform a file belongs to, None = not ours to judge."""
    if symbol.project in ("ios", "android"):
        return symbol.project
    if symbol.project == "desktop":
        base = HASH_SUFFIX.sub("", re.sub(r"\.(pdb|exe|debug)$", "", symbol.name.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]))
        return DESKTOP_FILES.get(base)
    return None


class Refused(Exception):
    pass


def select_by_version(symbols, floors, tags, now, keep_releases=KEEP_RELEASES):
    """The symbols to delete, [(Symbol, version)]. Pure; raises Refused.

    floors: {platform: "0.14.140"}; tags: {platform: [("0.14.140", datetime)]}."""
    drop = []
    by_platform = defaultdict(list)
    for symbol in symbols:
        platform = platform_of(symbol)
        if platform:
            by_platform[platform].append(symbol)
    for platform, files in sorted(by_platform.items()):
        floor = parse_version(floors.get(platform))
        if floor is None:
            raise Refused(f"{platform} has no usable CLIENT_MIN_VERSION floor ({floors.get(platform)!r})")
        releases = sorted((at, parse_version(version)) for version, at in tags.get(platform, []) if parse_version(version))
        last = set(sorted({version for _, version in releases}, reverse=True)[:keep_releases])
        if not releases or len(last) < keep_releases:
            raise Refused(f"{platform} has fewer than {keep_releases} known releases")
        families = defaultdict(list)
        for symbol in files:
            families[symbol.family].append(symbol)
        newest = {
            symbol.id
            for family in families.values()
            for symbol in sorted(family, key=lambda s: s.created, reverse=True)[:keep_releases]
        }
        gone = []
        for symbol in files:
            # The highest version, not the latest date: a tag cut on an older
            # commit must not make a file look older than it is.
            version = max((v for at, v in releases if at <= symbol.created), default=None)
            if version is None or version >= floor or version in last:
                continue
            if symbol.id in newest or symbol.created > now - VERSION_MIN_AGE:
                continue
            gone.append((symbol, version))
        if gone and len(gone) == len(files):
            raise Refused(f"every file of {platform} would go")
        drop += gone
    return drop


def get_json(url):
    request = urllib.request.Request(url, headers={"user-agent": "glitchtip-maintenance"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def fetch_floors():
    payload = get_json(VERSION_URL)
    return {platform: (payload.get(platform) or {}).get("min") for platform in VERSION_PLATFORMS}


def fetch_tags():
    """{platform: [(version, commit date)]}: never later than the tag's build and upload."""
    tags = defaultdict(list)
    for page in (1, 2, 3):
        for release in get_json(f"{REPO_API}/releases?per_page=100&page={page}"):
            platform, _, version = release["tag_name"].partition("-v")
            if platform in VERSION_PLATFORMS and not release["draft"]:
                tags[platform].append((version, datetime.fromisoformat(release["created_at"].replace("Z", "+00:00"))))
    refs = [ref["ref"].removeprefix("refs/tags/ios-v") for ref in get_json(f"{REPO_API}/git/matching-refs/tags/ios-v")]
    for version in sorted(filter(parse_version, refs), key=parse_version)[-IOS_TAGS:]:
        date = get_json(f"{REPO_API}/commits/ios-v{version}")["commit"]["committer"]["date"]
        tags["ios"].append((version, datetime.fromisoformat(date.replace("Z", "+00:00"))))
    return tags


def gib(n):
    return f"{(n or 0) / 2**30:.2f} GiB"


def setup():
    sys.path.insert(0, "/code")
    os.environ.setdefault("DJANGO_SETTINGS_MODULE", "glitchtip.settings")
    import django

    django.setup()


def family(dif):
    data = dif.data or {}
    if data.get("symbol_type") == "proguard":
        return (dif.project_id, "proguard")
    code_id = data.get("code_id") or ""
    fmt = {40: "elf", 32: "macho"}.get(len(code_id), "other")
    return (dif.project_id, HASH_SUFFIX.sub("", dif.name), data.get("arch"), fmt)


def chunks(dry_run):
    from django.db.models import Exists, OuterRef, Sum
    from apps.files.maintenance import _delete_file_blobs
    from apps.files.models import File, FileBlob

    orphans = FileBlob.objects.filter(created__lt=datetime.now(timezone.utc) - CHUNK_GRACE).exclude(
        Exists(File.objects.filter(blob_id=OuterRef("id")))
    )
    count, size = orphans.count(), orphans.aggregate(s=Sum("size"))["s"]
    print(f"chunks: {count} unreferenced blobs older than 1 h, {gib(size)}")
    if count and not dry_run:
        asyncio.run(_delete_file_blobs(orphans, "unreferenced (ERR-15)"))


def select_by_count(symbols, now):
    """eDE: no floor, so the newest KEEP per family stay. Pure."""
    by_family = defaultdict(list)
    for symbol in sorted(symbols, key=lambda s: s.created, reverse=True):
        if symbol.project == "ede":
            by_family[symbol.family].append(symbol)
    return [s for files in by_family.values() for s in files[KEEP:] if s.created < now - MIN_AGE]


def retention(apply):
    from django.db.models import Exists, OuterRef
    from apps.difs.models import DebugInformationFile
    from apps.files.models import File
    from apps.sourcecode.models import DebugSymbolBundle

    difs = {dif.id: dif for dif in DebugInformationFile.objects.select_related("file", "project")}
    symbols = [
        Symbol(dif.id, dif.project.slug, dif.name, family(dif), dif.created, dif.file.size) for dif in difs.values()
    ]
    now = datetime.now(timezone.utc)
    floors, tags = fetch_floors(), fetch_tags()
    print("floors: " + ", ".join(f"{platform} {floors[platform]}" for platform in VERSION_PLATFORMS))
    drop = select_by_version(symbols, floors, tags, now)
    drop += [(symbol, None) for symbol in select_by_count(symbols, now)]
    gone = {symbol.id for symbol, _ in drop}
    for symbol, version in sorted(drop, key=lambda entry: entry[0].created):
        label = ".".join(map(str, version)) if version else "beyond the newest"
        print(f"  {symbol.created:%Y-%m-%d %H:%M} {symbol.project} {symbol.name} {symbol.family[2:]} {label} {gib(symbol.size)}")
    total = defaultdict(lambda: [0, 0])
    for symbol in symbols:
        row = total[platform_of(symbol) or f"{symbol.project} (no version)"]
        row[symbol.id in gone] += symbol.size
    for name, (kept, dropped) in sorted(total.items()):
        print(f"{name}: keep {gib(kept)}, drop {gib(dropped)}")
    print(f"retention: {'deleting' if apply else 'would delete'} {len(drop)} of {len(symbols)} files")
    if not apply or not drop:
        return
    file_ids = {difs[id].file_id for id in gone}
    DebugInformationFile.objects.filter(id__in=gone).delete()
    File.objects.filter(id__in=file_ids).exclude(
        Exists(DebugInformationFile.objects.filter(file_id=OuterRef("id")))
    ).exclude(Exists(DebugSymbolBundle.objects.filter(file_id=OuterRef("id")))).exclude(
        Exists(DebugSymbolBundle.objects.filter(sourcemap_file_id=OuterRef("id")))
    ).delete()


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command not in ("chunks", "retention"):
        sys.exit("usage: glitchtip_maintenance.py chunks [--dry-run] | retention [--apply]")
    setup()
    if command == "chunks":
        chunks("--dry-run" in sys.argv)
    else:
        apply = "--apply" in sys.argv and "--dry-run" not in sys.argv
        try:
            retention(apply)
        except Refused as reason:
            sys.exit(f"retention refused, nothing deleted: {reason}")
        chunks(not apply)

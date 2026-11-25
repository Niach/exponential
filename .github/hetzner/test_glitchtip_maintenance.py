"""#EXP-2147: the symbol selection. Run: python3 .github/hetzner/test_glitchtip_maintenance.py"""

import unittest
from datetime import datetime, timedelta, timezone

import glitchtip_maintenance as m

NOW = datetime(2026, 11, 25, 16, 0, tzinfo=timezone.utc)
DAY0 = datetime(2026, 11, 1, tzinfo=timezone.utc)


def tags(*versions):
    """One release a day from DAY0."""
    return [(version, DAY0 + timedelta(days=i)) for i, version in enumerate(versions)]


def symbol(id, day, project="desktop", name="exp-desktop", arch="arm64", hours=2):
    created = DAY0 + timedelta(days=day, hours=hours)
    return m.Symbol(id, project, name, (project, name, arch, "macho"), created, 100)


TAGS = {"desktop": tags("0.14.1", "0.14.2", "0.14.3", "0.14.4", "0.14.5", "0.14.10")}
ALL = [symbol(i, i) for i in range(6)]


def dropped(symbols, floor, tag_map=TAGS, now=NOW):
    return [s.id for s, _ in m.select_by_version(symbols, {"desktop": floor}, tag_map, now)]


class Selection(unittest.TestCase):
    def test_keeps_the_floor_and_above(self):
        self.assertEqual(dropped(ALL, "0.14.3"), [0, 1])

    def test_keeps_the_last_two_releases_below_the_floor(self):
        # The floor is the newest release itself (prod, 2026-11-25): 5 and 10 stay.
        self.assertEqual(dropped(ALL, "0.14.10"), [0, 1, 2, 3])
        self.assertEqual(dropped(ALL, "0.15.0"), [0, 1, 2, 3])

    def test_versions_compare_as_numbers(self):
        self.assertEqual(dropped(ALL, "0.14.9"), [0, 1, 2, 3])
        self.assertGreater(m.parse_version("0.14.10"), m.parse_version("0.14.9"))

    def test_reports_the_version(self):
        self.assertEqual([v for _, v in m.select_by_version(ALL, {"desktop": "0.14.2"}, TAGS, NOW)], [(0, 14, 1)])

    def test_a_file_older_than_every_known_tag_is_kept(self):
        self.assertEqual(dropped([symbol(99, -3)] + ALL, "0.14.3"), [0, 1])

    def test_an_upload_during_the_next_release_counts_as_the_later_version(self):
        late = symbol(98, 0, hours=25)  # 0.14.1's upload, after 0.14.2's tag
        self.assertNotIn(98, dropped([late] + ALL, "0.14.2"))

    def test_a_hotfix_tag_on_an_older_commit_never_makes_a_file_older(self):
        tag_map = {"desktop": TAGS["desktop"] + [("0.14.0", DAY0 + timedelta(days=5, hours=1))]}
        self.assertEqual(dropped(ALL, "0.14.5", tag_map), [0, 1, 2, 3])

    def test_an_unchanged_library_uploaded_once_long_ago_is_kept(self):
        # sentry-cli skips a build-id the server has: the one upload serves every later version.
        library = symbol(97, 0, project="android", name="libfoo.so")
        apps = [symbol(100 + i, i, project="android", name="libapp.so") for i in range(6)]
        got = m.select_by_version([library] + apps, {"android": "0.14.10"}, {"android": TAGS["desktop"]}, NOW)
        self.assertEqual([s.id for s, _ in got], [100, 101, 102, 103])

    def test_unknown_files_and_projects_are_kept(self):
        strange = [symbol(90, 0, name="libmystery.so"), symbol(91, 0, project="web"), symbol(92, 0, project="ede")]
        self.assertEqual(dropped(strange + ALL, "0.14.3"), [0, 1])

    def test_a_fresh_upload_is_kept(self):
        now = DAY0 + timedelta(days=3, hours=4)
        self.assertEqual(dropped(ALL[:4], "0.14.10", now=now), [0, 1])
        self.assertEqual(dropped([symbol(i, 0, hours=20 + i) for i in range(4)] + ALL[4:], "0.15.0", now=DAY0 + timedelta(hours=24)), [])


class Refusal(unittest.TestCase):
    def test_no_floor(self):
        for floor in (None, "", "latest", "0.14", "v0.14.3"):
            with self.assertRaises(m.Refused):
                dropped(ALL, floor)

    def test_one_platform_without_a_floor_refuses_the_whole_run(self):
        with self.assertRaises(m.Refused):
            m.select_by_version(ALL + [symbol(50, 1, project="ios", name="Exponential")], {"desktop": "0.14.3"}, TAGS, NOW)

    def test_no_or_too_few_releases(self):
        for tag_map in ({}, {"desktop": tags("0.14.1")}, {"desktop": tags("nightly", "x")}):
            with self.assertRaises(m.Refused):
                dropped(ALL, "0.14.3", tag_map)

    def test_everything_of_a_platform(self):
        with self.assertRaises(m.Refused):
            m.select_by_version(ALL, {"desktop": "9.0.0"}, TAGS, NOW, keep_releases=0)


class Platforms(unittest.TestCase):
    def test_names(self):
        for project, name, platform in [
            ("desktop", "exp-desktop", "desktop"),
            ("desktop", "exp_desktop.pdb", "desktop"),
            ("desktop", "exp_desktop-0123456789abcdef", "desktop"),
            ("desktop", "D:\\a\\target\\release\\exp_desktop.pdb", "desktop"),
            ("desktop", "exponential", "cli"),
            ("desktop", "exponential-0123456789abcdef", "cli"),
            ("desktop", "exponential-shot", None),
            ("ios", "NotificationService", "ios"),
            ("android", "proguard", "android"),
            ("ede", "exponential-desktop", None),
        ]:
            self.assertEqual(m.platform_of(symbol(1, 1, project=project, name=name)), platform, name)

    def test_ede_keeps_the_newest_of_a_family(self):
        files = [symbol(i, i, project="ede", name="exponential-desktop") for i in range(8)]
        self.assertEqual(sorted(s.id for s in m.select_by_count(files + ALL, NOW)), list(range(8 - m.KEEP)))


if __name__ == "__main__":
    unittest.main()

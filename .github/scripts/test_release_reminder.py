import datetime as dt
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import release_reminder as reminder


class ReminderTests(unittest.TestCase):
    release = {
        "id": 42,
        "tag_name": "v1.3.0",
        "published_at": "2026-01-31T12:00:00Z",
        "html_url": "https://example.org/releases/v1.3.0",
    }

    def test_calendar_months_include_short_months_and_year_changes(self):
        for start, expected in [
            ("2026-01-31", "2026-04-30"),
            ("2026-11-30", "2027-02-28"),
            ("2023-11-30", "2024-02-29"),
        ]:
            self.assertEqual(
                reminder.three_months_after(dt.date.fromisoformat(start)),
                dt.date.fromisoformat(expected),
            )

    def test_recent_release_does_not_even_query_issues(self):
        with patch.object(reminder, "gh", return_value=json.dumps(self.release)) as gh:
            reminder.remind("owner/repo", dt.datetime(2026, 4, 30, 11, tzinfo=dt.timezone.utc))
        gh.assert_called_once_with("api", "repos/owner/repo/releases/latest")

    def test_closed_reminder_on_a_later_page_prevents_duplicates(self):
        pages = [[{"body": "unrelated"}], [{
            "state": "closed", "body": "<!-- brumm:apple-token-reminder:42 -->",
        }]]
        with patch.object(reminder, "gh", side_effect=[json.dumps(self.release), json.dumps(pages)]) as gh:
            reminder.remind("owner/repo", dt.datetime(2026, 5, 1, tzinfo=dt.timezone.utc))
        self.assertEqual(gh.call_count, 2)
        self.assertIn("--paginate", gh.call_args.args)

    def test_old_release_opens_only_an_issue_with_the_warning(self):
        calls = []

        def gh(*args):
            calls.append(args)
            if args[0] == "api":
                return json.dumps(self.release if args[-1].endswith("/latest") else [[{
                    "body": "<!-- brumm:apple-token-reminder:41 -->",
                }]])
            self.assertEqual(args[:2], ("issue", "create"))
            body = Path(args[args.index("--body-file") + 1]).read_text()
            self.assertIn("valid for at most six months", body)
            self.assertIn("<!-- brumm:apple-token-reminder:42 -->", body)
            self.assertIn(self.release["html_url"], body)
            return "https://example.org/issues/1\n"

        with patch.object(reminder, "gh", side_effect=gh):
            reminder.remind("owner/repo", dt.datetime(2026, 4, 30, 12, tzinfo=dt.timezone.utc))
        self.assertEqual(len(calls), 3)

    def test_no_stable_release_does_nothing(self):
        error = subprocess.CalledProcessError(1, "gh", stderr="gh: Not Found (HTTP 404)")
        with patch.object(reminder, "gh", side_effect=error) as gh:
            reminder.remind("owner/repo", dt.datetime.now(dt.timezone.utc))
        self.assertEqual(gh.call_count, 1)

    def test_api_failures_are_not_mistaken_for_no_release(self):
        error = subprocess.CalledProcessError(1, "gh", stderr="gh: Forbidden (HTTP 403)")
        with patch.object(reminder, "gh", side_effect=error):
            with self.assertRaises(subprocess.CalledProcessError):
                reminder.remind("owner/repo", dt.datetime.now(dt.timezone.utc))


if __name__ == "__main__":
    unittest.main()

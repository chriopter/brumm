"""Remind maintainers to renew an aging release's Apple Music token."""

import calendar
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import tempfile


def gh(*args):
    return subprocess.run(
        ["gh", *args], check=True, capture_output=True, text=True
    ).stdout


def three_months_after(date):
    month = date.month + 3
    year = date.year + (month - 1) // 12
    month = (month - 1) % 12 + 1
    day = min(date.day, calendar.monthrange(year, month)[1])
    return date.replace(year=year, month=month, day=day)


def remind(repo, now):
    # GitHub's latest-release endpoint excludes drafts and prereleases.
    try:
        release = json.loads(gh("api", f"repos/{repo}/releases/latest"))
    except subprocess.CalledProcessError as error:
        if "(HTTP 404)" in (error.stderr or ""):
            print("No stable release yet; nothing to remind about.")
            return
        raise
    published = dt.datetime.fromisoformat(release["published_at"].replace("Z", "+00:00"))
    if now < three_months_after(published):
        print("The latest stable release is less than three months old.")
        return

    marker = f"<!-- brumm:apple-token-reminder:{release['id']} -->"
    pages = json.loads(gh(
        "api", "--paginate", "--slurp",
        f"repos/{repo}/issues?state=all&per_page=100",
    ))
    if any(
        marker in (issue.get("body") or "")
        for page in pages for issue in page
        if "pull_request" not in issue
    ):
        print("This release already has a reminder, open or closed.")
        return

    tag = release["tag_name"]
    body = f"""The latest stable release [{tag}]({release['html_url']}) was published on {published:%Y-%m-%d} and is at least three months old.

The embedded Apple Music developer token is valid for at most six months. This reminder checks the release's age, not the token's actual expiry date.

Please test `main`, then publish a signed release tag to build a release with a fresh token and write its release notes.

This check has not created a release or tag.

{marker}
"""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "body.md"
        path.write_text(body, encoding="utf-8")
        url = gh(
            "issue", "create", "--repo", repo,
            "--title", f"Renew Apple Music token after {tag}",
            "--body-file", str(path),
        )
    print(url.strip())


if __name__ == "__main__":
    remind(os.environ["GH_REPO"], dt.datetime.now(dt.timezone.utc))

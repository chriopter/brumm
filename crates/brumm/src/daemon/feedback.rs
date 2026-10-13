//! Feedback is an issue on GitHub, written in a player: the browser opens
//! on it, filled in, or Omarchy's coding agent looks into it first.

use crate::apple::encode_query as text_encode_query;
use crate::error::{Result, err};
use crate::update;

const ISSUES_REPO: &str = "chriopter/brumm";

fn issues_url() -> String {
    format!("https://github.com/{ISSUES_REPO}/issues/new")
}

/// Names the issue: this brumm, now.
fn title(version: &str, at: chrono::DateTime<chrono::Local>) -> String {
    format!("brumm {version} – {}", at.format("%Y-%m-%d %H:%M"))
}

/// What the user wrote, and under it what runs here.
fn body(text: &str, version: &str, face: &str) -> String {
    let face = if face.is_empty() { "?" } else { face };
    format!(
        "{}\n\n---\nbrumm {version} · {face} · {}",
        text.trim(),
        system()
    )
}

/// The new-issue page with title and body filled in.
fn issue_link(title: &str, body: &str) -> String {
    format!(
        "{}?{}",
        issues_url(),
        text_encode_query(&[("title".into(), title.into()), ("body".into(), body.into())])
    )
}

/// Opens the browser on the new issue, filled in, to be sent there with
/// one click. With agent set, and an Omarchy agent chosen, the agent gets
/// it instead: it looks into the logs and files the issue. Says how it
/// went: "agent" or "browser".
pub fn send(text: &str, version: &str, face: &str, agent: bool) -> Result<String> {
    if text.trim().is_empty() {
        return Err(err("feedback: nothing written"));
    }
    if agent
        && update::has_agent()
        && spawn(
            "omarchy-agent",
            &["--prompt", &agent_prompt(text, version, face)],
        )
    {
        return Ok("agent".into());
    }
    let url = issue_link(
        &title(version, chrono::Local::now()),
        &body(text, version, face),
    );
    if !spawn("xdg-open", &[&url]) {
        return Err(err("could not open the browser (xdg-open)"));
    }
    Ok("browser".into())
}

/// Starts a program and lets it run, reaped in the background.
fn spawn(program: &str, args: &[&str]) -> bool {
    let child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match child {
        Ok(mut c) => {
            std::thread::spawn(move || {
                let _ = c.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Asks the agent to look into what the user reports and file it.
fn agent_prompt(text: &str, version: &str, face: &str) -> String {
    format!(
        r#"A user of brumm (an Apple Music player for Omarchy, https://github.com/{ISSUES_REPO}) reports:

  {text}

Running: brumm {version} · {face} · {system}

Look into it: is it a bug or a feature request? For a bug, check the logs
(journalctl --user -u brumm -n 200, and ~/.cache/brumm/*.log) for what went
wrong. Then write a short, clear GitHub issue titled "{title}: <summary>"
with the report, what you found and the versions above; leave out anything
private (tokens, account names, paths in the home directory beyond ~/.cache/brumm).
Show it to the user, and once they agree create it in {ISSUES_REPO}
(gh issue create -R {ISSUES_REPO}, or open {url} in the
browser filled in if gh is not signed in) and open it in the browser."#,
        text = text.trim(),
        system = system(),
        title = title(version, chrono::Local::now()),
        url = issues_url(),
    )
}

/// What brumm runs on: the distribution and its version, the desktop.
fn system() -> String {
    let mut name = "Linux".to_string();
    if let Ok(b) = std::fs::read_to_string("/etc/os-release") {
        for l in b.lines() {
            if let Some(v) = l.strip_prefix("PRETTY_NAME=") {
                name = v.trim_matches('"').to_string();
            }
        }
    }
    if let Ok(b) = std::fs::read_to_string("/usr/share/omarchy/version") {
        name = format!("{name} {}", b.trim());
    }
    if let Ok(d) = std::env::var("XDG_CURRENT_DESKTOP")
        && !d.is_empty()
    {
        name = format!("{name} · {d}");
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// The issue is named for the version and the time, and carries what
    /// was written above what runs here.
    #[test]
    fn issue_text() {
        let at = chrono::Local
            .with_ymd_and_hms(2026, 10, 1, 9, 30, 0)
            .unwrap();
        let t = title("v1.2.3", at);
        assert_eq!(t, "brumm v1.2.3 – 2026-10-01 09:30");
        let b = body("  the cover is blank\n", "v1.2.3", "gui");
        assert!(
            b.starts_with("the cover is blank\n\n---\nbrumm v1.2.3 · gui · "),
            "{b:?}"
        );
        let u = url::Url::parse(&issue_link(&t, &b)).unwrap();
        assert_eq!(
            (u.host_str(), u.path()),
            (Some("github.com"), "/chriopter/brumm/issues/new")
        );
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(
            (q["title"].as_str(), q["body"].as_str()),
            (t.as_str(), b.as_str())
        );
        let p = agent_prompt("no sound", "v1.2.3", "tui");
        assert!(p.contains("no sound") && p.contains("journalctl --user -u brumm"));
        assert!(send("  ", "v1", "tui", false).is_err(), "sent nothing");
    }
}

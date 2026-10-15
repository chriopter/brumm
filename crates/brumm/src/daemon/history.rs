//! Account-scoped listening history, appended once per played song.
//! Byte cursors keep paging stable while new songs are appended.
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::error::Result;
use crate::music::Track;

pub fn record(dir: &Path, track: &Track) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut data = serde_json::to_vec(track)?;
    data.push(b'\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(dir.join("history.jsonl"))?
        .write_all(&data)?;
    Ok(())
}

pub fn page(dir: &Path, cursor: Option<u64>, limit: usize) -> Result<(Vec<Track>, String)> {
    let mut file = match File::open(dir.join("history.jsonl")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(e.into()),
    };
    let end = cursor
        .unwrap_or(file.metadata()?.len())
        .min(file.metadata()?.len());
    let mut start = end;
    let mut data = Vec::new();
    while start > 0 && data.iter().filter(|&&b| b == b'\n').count() < limit + 2 {
        let size = start.min(8192) as usize;
        start -= size as u64;
        file.seek(SeekFrom::Start(start))?;
        let mut chunk = vec![0; size];
        file.read_exact(&mut chunk)?;
        chunk.extend(data);
        data = chunk;
    }
    let mut offset = 0;
    let lines: Vec<_> = data
        .split_inclusive(|&b| b == b'\n')
        .map(|line| {
            let at = offset;
            offset += line.len();
            (at, line)
        })
        .collect();
    let mut tracks = Vec::new();
    let mut next = 0;
    for (offset, line) in lines.into_iter().rev() {
        if offset == 0 && start > 0 {
            break;
        }
        next = start + offset as u64;
        if let Ok(track) = serde_json::from_slice::<Track>(line) {
            tracks.push(track);
            if tracks.len() == limit {
                break;
            }
        }
    }
    Ok((
        tracks,
        if next > 0 {
            next.to_string()
        } else {
            String::new()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_stay_stable_when_new_songs_arrive_and_accounts_are_separate() {
        let dir = tempfile_path();
        for n in 0..125 {
            record(
                &dir,
                &Track {
                    id: n.to_string(),
                    title: "Song".repeat(100),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let (first, cursor) = page(&dir, None, 50).unwrap();
        assert_eq!(first.len(), 50);
        assert_eq!(first[0].id, "124");
        assert_eq!(first[49].id, "75");
        record(
            &dir,
            &Track {
                id: "new".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let (second, cursor) = page(&dir, Some(cursor.parse().unwrap()), 50).unwrap();
        assert_eq!(second[0].id, "74");
        assert_eq!(second[49].id, "25");
        let (last, cursor) = page(&dir, Some(cursor.parse().unwrap()), 50).unwrap();
        assert_eq!(last.len(), 25);
        assert_eq!(last[24].id, "0");
        assert!(cursor.is_empty());
        assert!(
            page(&dir.join("other-account"), None, 50)
                .unwrap()
                .0
                .is_empty()
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.join("history.jsonl"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn tempfile_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "brumm-history-{}-{}",
            std::process::id(),
            getrandom::u64().unwrap()
        ))
    }
}

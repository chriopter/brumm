//! Starting other programs: Chrome, tied to the daemon's life and talking
//! over inherited pipes, and players and daemons detached into sessions of
//! their own. The few unsafe calls brumm makes live here, each between fork
//! and exec, where only async-signal-safe system calls may run.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::sync::mpsc;

type Job = Box<dyn FnOnce() + Send>;

/// The thread every child that must die with brumm is started from.
/// PR_SET_PDEATHSIG fires when the thread that forked the child ends, not
/// the process; this thread lives as long as brumm does, so the signal
/// comes exactly when brumm goes.
fn spawner() -> &'static mpsc::Sender<Job> {
    static SPAWNER: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    SPAWNER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("brumm-spawn".into())
            .spawn(move || {
                for job in rx {
                    job();
                }
            })
            .expect("spawner thread");
        tx
    })
}

/// A copy of fd numbered at least 10 (and close-on-exec), so moving it to
/// 3 or 4 in the child can never overwrite another one.
fn high(fd: OwnedFd) -> io::Result<OwnedFd> {
    // SAFETY: F_DUPFD_CLOEXEC on a valid descriptor we own returns a new
    // descriptor (or -1), which we then own.
    let n = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: n is a fresh descriptor nobody else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(n) })
}

/// Starts cmd with `fd3` and `fd4` as its descriptors 3 and 4, killed with
/// SIGKILL when brumm dies (however it dies). The descriptors are closed
/// here once the child has its copies.
pub fn spawn_tied(mut cmd: Command, fd3: OwnedFd, fd4: OwnedFd) -> io::Result<Child> {
    let (fd3, fd4) = (high(fd3)?, high(fd4)?);
    let parent = std::process::id() as libc::pid_t;
    let (a, b): (RawFd, RawFd) = (fd3.as_raw_fd(), fd4.as_raw_fd());
    // SAFETY: the closure runs in the forked child before exec and calls
    // only async-signal-safe functions (dup2, prctl, getppid, _exit). dup2
    // clears close-on-exec on the copies, which Chrome then reads as its
    // DevTools pipe (--remote-debugging-pipe).
    unsafe {
        cmd.pre_exec(move || {
            if libc::dup2(a, 3) < 0 || libc::dup2(b, 4) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::prctl(
                libc::PR_SET_PDEATHSIG,
                libc::SIGKILL as libc::c_ulong,
                0,
                0,
                0,
            ) < 0
            {
                return Err(io::Error::last_os_error());
            }
            // brumm died between fork and prctl: the signal will never come.
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        });
    }
    let (tx, rx) = mpsc::channel();
    spawner()
        .send(Box::new(move || {
            let r = cmd.spawn();
            drop((fd3, fd4));
            let _ = tx.send(r);
        }))
        .map_err(|_| io::Error::other("spawner thread is gone"))?;
    rx.recv()
        .map_err(|_| io::Error::other("spawner thread is gone"))?
}

/// Starts cmd in a session of its own, so it outlives brumm.
pub fn detach(mut cmd: Command) -> io::Result<()> {
    // SAFETY: setsid is async-signal-safe; it runs in the forked child.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    // Reaped in the background: nothing waits for it, nothing turns zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Kills process pid with SIGKILL.
pub fn kill(pid: u32) {
    // SAFETY: kill has no memory effects; pid is a child we started and
    // have not reaped (the caller checks it has not exited).
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn child_reads_fd3_and_writes_fd4() {
        let (to_child_r, mut to_child_w) = std::io::pipe().unwrap();
        let (mut from_child_r, from_child_w) = std::io::pipe().unwrap();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "read line <&3; echo \"got $line\" >&4"]);
        let mut child = spawn_tied(cmd, to_child_r.into(), from_child_w.into()).unwrap();
        to_child_w.write_all(b"ping\n").unwrap();
        drop(to_child_w);
        let mut out = String::new();
        from_child_r.read_to_string(&mut out).unwrap();
        assert_eq!(out, "got ping\n");
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn tied_children_die_with_their_parent() {
        // A middle process starts a tied sleeper and exits; the sleeper
        // must not outlive it. Run as a separate brumm test process: the
        // test binary itself plays the parent.
        if std::env::var_os("BRUMM_TIED_PARENT").is_some() {
            let (r, _w) = std::io::pipe().unwrap();
            let (_r2, w2) = std::io::pipe().unwrap();
            let mut cmd = Command::new("sleep");
            cmd.arg("30");
            let child = spawn_tied(cmd, r.into(), w2.into()).unwrap();
            println!("\nsleeper={}", child.id());
            std::process::exit(0);
        }
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "proc::tests::tied_children_die_with_their_parent",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BRUMM_TIED_PARENT", "1")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let pid: u32 = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("sleeper=")?.parse().ok())
            .expect("the sleeper's pid");
        for _ in 0..50 {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            // Gone, or a zombie waiting for init to reap it.
            if stat.is_empty() || stat.split(") ").nth(1).is_some_and(|s| s.starts_with('Z')) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        kill(pid);
        panic!("the tied child outlived its parent");
    }
}

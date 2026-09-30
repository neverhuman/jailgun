use anyhow::{Context, Result};
use jailgun_core::browser_registry::ensure_private_dir;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    os::{
        fd::OwnedFd,
        unix::{fs::OpenOptionsExt, net::UnixStream},
    },
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::{Mutex, OwnedSemaphorePermit, Semaphore},
};
#[cfg(test)]
mod tests;

pub struct ManagedDisplay {
    child: Mutex<Child>,
    display: String,
    authority: PathBuf,
    viewer: Arc<Semaphore>,
}

pub struct LoginView {
    pub stream: tokio::net::UnixStream,
    child: Child,
    // The permit lives as long as the private transport, including failed upgrades.
    _permit: OwnedSemaphorePermit,
}

impl ManagedDisplay {
    pub async fn start(state_dir: &Path) -> Result<Self> {
        let directory = state_dir.join("displays");
        ensure_private_dir(&directory)?;
        let authority = directory.join(format!("{}.Xauthority", uuid::Uuid::new_v4()));
        write_authority(&authority)?;
        let mut command = private_command("Xvfb");
        command
            .args([
                "-displayfd",
                "1",
                "-nolisten",
                "tcp",
                "-screen",
                "0",
                "1280x900x24",
                "-noreset",
                "-auth",
            ])
            .arg(&authority)
            .stdout(Stdio::piped());
        let mut child = command.spawn().context(
            "server-display-missing: install Xvfb and x11vnc; run jailgun doctor --server-browser",
        )?;
        let mut stdout = child.stdout.take().context("display output missing")?;
        let display = tokio::time::timeout(Duration::from_secs(10), async {
            let mut digits = Vec::new();
            loop {
                let byte = stdout.read_u8().await?;
                if byte == b'\n' {
                    break;
                }
                anyhow::ensure!(
                    byte.is_ascii_digit() && digits.len() < 6,
                    "invalid display allocation"
                );
                digits.push(byte);
            }
            anyhow::ensure!(!digits.is_empty(), "missing display allocation");
            Ok::<_, anyhow::Error>(format!(":{}", String::from_utf8(digits)?))
        })
        .await
        .context("server-display-timeout")??;
        anyhow::ensure!(child.try_wait()?.is_none(), "server-display-exited");
        Ok(Self {
            child: Mutex::new(child),
            display,
            authority,
            viewer: Arc::new(Semaphore::new(1)),
        })
    }

    pub fn environment(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("DISPLAY".into(), self.display.clone()),
            ("XAUTHORITY".into(), self.authority.to_string_lossy().into()),
            // Force this account's private X display even in a Wayland desktop.
            ("WAYLAND_DISPLAY".into(), String::new()),
        ])
    }

    pub async fn alive(&self) -> bool {
        matches!(self.child.lock().await.try_wait(), Ok(None))
    }

    pub async fn open_view(&self) -> Result<LoginView> {
        let permit = self
            .viewer
            .clone()
            .try_acquire_owned()
            .context("login-view-in-use: close the other view first")?;
        anyhow::ensure!(self.alive().await, "server-display-exited");
        let (stream, child_socket) = UnixStream::pair()?;
        let input: OwnedFd = child_socket.try_clone()?.into();
        let output: OwnedFd = child_socket.into();
        let mut command = private_command("x11vnc");
        command
            .args(["-inetd", "-display", &self.display, "-auth"])
            .arg(&self.authority)
            .args([
                "-nopw",
                "-noxdamage",
                "-noshm",
                "-noclipboard",
                "-nosetclipboard",
                "-norc",
                "-quiet",
            ])
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output));
        let child = command.spawn().context(
            "login-view-dependency-missing: install x11vnc and run jailgun doctor --server-browser",
        )?;
        stream.set_nonblocking(true)?;
        Ok(LoginView {
            stream: tokio::net::UnixStream::from_std(stream)?,
            child,
            _permit: permit,
        })
    }

    pub async fn shutdown(&self) {
        stop(&mut *self.child.lock().await).await;
    }
}

impl LoginView {
    pub async fn shutdown(&mut self) {
        stop(&mut self.child).await;
    }
}

async fn stop(child: &mut Child) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}

fn private_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for name in ["PATH", "HOME", "LANG", "LC_ALL", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: this child-side hook invokes only syscalls and constructs an OS
    // error; it does not allocate or acquire locks between fork and exec. The
    // second check closes the race where the daemon died before prctl ran.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        });
    }
    command
}

fn write_authority(path: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    // FamilyWild with an empty display number is accepted before -displayfd
    // chooses a free display. The random cookie still gates all X clients.
    file.write_all(&65535_u16.to_be_bytes())?;
    for field in [
        &b""[..],
        &b""[..],
        &b"MIT-MAGIC-COOKIE-1"[..],
        uuid::Uuid::new_v4().as_bytes(),
    ] {
        file.write_all(&(field.len() as u16).to_be_bytes())?;
        file.write_all(field)?;
    }
    file.sync_all()?;
    Ok(())
}

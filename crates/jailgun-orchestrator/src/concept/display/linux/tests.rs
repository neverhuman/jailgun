use super::*;
use std::os::unix::fs::PermissionsExt;
use tokio::io::AsyncWriteExt;

#[tokio::test]
#[ignore = "requires Xvfb, x11vnc and xdpyinfo; executed by the browser E2E lane"]
async fn private_display_and_rfb_isolation() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/display-proofs");
    ensure_private_dir(&root)?;
    let directory = tempfile::tempdir_in(root)?;
    let first = ManagedDisplay::start(&directory.path().join("one")).await?;
    let second = ManagedDisplay::start(&directory.path().join("two")).await?;
    assert_ne!(first.display, second.display);
    assert_eq!(
        std::fs::metadata(&first.authority)?.permissions().mode() & 0o777,
        0o600
    );
    for (authority, accepted) in [
        (first.authority.as_path(), true),
        (second.authority.as_path(), false),
        (Path::new("/nonexistent-jailgun-authority"), false),
    ] {
        let mut check = private_command("xdpyinfo");
        check
            .args(["-display", &first.display])
            .env("XAUTHORITY", authority);
        let status = tokio::time::timeout(Duration::from_secs(5), check.status()).await??;
        assert_eq!(status.success(), accepted, "X display authority isolation");
    }
    let port = 6000 + first.display.trim_start_matches(':').parse::<u16>()?;
    assert!(
        tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
    );
    let mut view = first.open_view().await?;
    assert!(
        first.open_view().await.is_err(),
        "a second viewer acquired the account"
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut greeting = [0_u8; 12];
        view.stream.read_exact(&mut greeting).await?;
        assert_eq!(&greeting, b"RFB 003.008\n");
        view.stream.write_all(&greeting).await?;
        assert_eq!(view.stream.read_u8().await?, 1);
        assert_eq!(view.stream.read_u8().await?, 1); // None on the private descriptor only.
        view.stream.write_all(&[1]).await?;
        assert_eq!(view.stream.read_u32().await?, 0);
        view.stream.write_all(&[1]).await?;
        assert_eq!(view.stream.read_u16().await?, 1280);
        assert_eq!(view.stream.read_u16().await?, 900);
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    view.shutdown().await;
    drop(view);
    let mut reopened = first.open_view().await?;
    reopened.shutdown().await;
    first.shutdown().await;
    assert!(!first.alive().await);
    assert!(
        second.alive().await,
        "closing one display stopped another account"
    );
    second.shutdown().await;
    parent_death_stops_display(directory.path()).await?;
    Ok(())
}

async fn parent_death_stops_display(directory: &Path) -> Result<()> {
    use tokio::io::AsyncBufReadExt;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "concept::display::linux::tests::display_parent_process",
            "--ignored",
            "--nocapture",
        ])
        .env(
            "JAILGUN_DISPLAY_CHILD_PROOF",
            directory.join("parent-death"),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut lines = tokio::io::BufReader::new(child.stdout.take().context("proof stdout")?).lines();
    let pid = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(line) = lines.next_line().await? {
            if let Some(pid) = line.strip_prefix("DISPLAY_PID=") {
                return Ok::<u32, anyhow::Error>(pid.parse()?);
            }
        }
        anyhow::bail!("display child did not report its PID")
    })
    .await??;
    child.kill().await?;
    child.wait().await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = std::fs::read_to_string(format!("/proc/{pid}/stat"));
            if status
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                || status.as_ref().is_ok_and(|s| {
                    s.split_once(") ")
                        .is_some_and(|(_, fields)| fields.starts_with("Z "))
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("display survived daemon death")?;
    Ok(())
}

#[tokio::test]
#[ignore = "subprocess of private_display_and_rfb_isolation"]
async fn display_parent_process() -> Result<()> {
    let directory = std::env::var_os("JAILGUN_DISPLAY_CHILD_PROOF")
        .context("parent proof must supply its private directory")?;
    let display = ManagedDisplay::start(Path::new(&directory)).await?;
    println!(
        "DISPLAY_PID={}",
        display.child.lock().await.id().context("display PID")?
    );
    std::io::stdout().flush()?;
    tokio::time::sleep(Duration::from_secs(60)).await;
    anyhow::bail!("parent proof failed to terminate this process")
}

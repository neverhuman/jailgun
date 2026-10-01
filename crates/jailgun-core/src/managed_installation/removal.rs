use super::{
    inventory::{exists, verify_file, Inventory},
    *,
};
use std::io::Write;

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct Journal {
    schema_version: u32,
    record: Record,
    current: PathBuf,
    manifests: BTreeMap<String, String>,
}

/// A validated removal holds both process-use and installer ownership locks.
/// Dropping it before execution leaves the journal for a subsequent invocation.
pub struct Removal {
    installation: ManagedInstallation,
    journal: Journal,
    inventory: Inventory,
    _use: File,
    installer: InstallerLock,
}

pub fn uninstall(prefix: &Path) -> Result<()> {
    Removal::prepare(prefix)?.execute()
}

impl Removal {
    pub fn prepare(prefix: &Path) -> Result<Self> {
        let prefix = prefix.canonicalize()?;
        let base = prefix.join("lib/jailgun");
        for path in [prefix.join("bin"), prefix.join("lib"), base.clone()] {
            directory(&path)?;
        }
        let installer = InstallerLock::acquire(&base)?;
        let journal_path = base.join(".uninstall.json");
        let resume = exists(&journal_path)?;
        let journal: Journal = if resume {
            serde_json::from_slice(&regular_bytes(&journal_path, 64 * 1024 * 1024)?)
                .map_err(|_| invalid())?
        } else {
            let installation = ManagedInstallation::load(&prefix)?;
            let record = read_record(&base)?;
            let mut manifests = BTreeMap::new();
            for name in installation.releases.keys() {
                directory(&base.join("releases").join(name))?;
                let bytes = regular_bytes(
                    &base.join("releases").join(name).join("manifest.json"),
                    8 * 1024 * 1024,
                )?;
                manifests.insert(
                    name.clone(),
                    String::from_utf8(bytes).map_err(|_| invalid())?,
                );
            }
            Journal {
                schema_version: 1,
                record,
                current: fs::read_link(base.join("current"))?,
                manifests,
            }
        };
        if journal.schema_version != 1 {
            return Err(invalid());
        }
        let installation = ManagedInstallation::from_record(prefix, journal.record.clone())?;
        let use_guard = installation.exclusive_use()?;
        if use_guard.metadata()?.len() != 0 {
            return Err(invalid());
        }
        let inventory = Inventory::parse(&installation, &journal.manifests)?;
        let removal = Self {
            installation,
            journal,
            inventory,
            _use: use_guard,
            installer,
        };
        removal.verify(resume)?;
        if !resume {
            let bytes = serde_json::to_vec(&removal.journal).map_err(|_| invalid())?;
            if bytes.len() > 64 * 1024 * 1024 {
                return Err(invalid());
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&journal_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            File::open(&base)?.sync_all()?;
        }
        Ok(removal)
    }

    pub fn execute(self) -> Result<()> {
        self.verify(true)?;
        let base = &self.installation.base;
        // Never recursively delete: verify every file immediately before unlinking.
        for (path, entry) in &self.inventory.files {
            if verify_file(base, path, entry, true)? {
                fs::remove_file(base.join(path))?;
            }
        }
        for path in self.inventory.directories.iter().rev() {
            let path = base.join(path);
            if exists(&path)? {
                directory(&path)?;
                fs::remove_dir(path)?;
            }
        }
        for (path, target) in self.links() {
            if verify_link(&path, &target, true)? {
                fs::remove_file(path)?;
            }
        }
        let marker = base.join("installation.json");
        if exists(&marker)? {
            if read_record(base)? != self.journal.record {
                return Err(invalid());
            }
            fs::remove_file(marker)?;
        }
        File::open(base)?.sync_all()?;
        // All application files are gone before retiring the recovery journal.
        let recorded: Journal = serde_json::from_slice(&regular_bytes(
            &base.join(".uninstall.json"),
            64 * 1024 * 1024,
        )?)
        .map_err(|_| invalid())?;
        if recorded != self.journal {
            return Err(invalid());
        }
        fs::remove_file(base.join(".uninstall.json"))?;
        fs::remove_file(base.join(".usage.lock"))?;
        self.installer.release()?;
        fs::remove_dir(base)?;
        Ok(())
    }

    fn verify(&self, resume: bool) -> Result<()> {
        let base = &self.installation.base;
        for child in fs::read_dir(base)? {
            let name = child?.file_name();
            if ![
                "installation.json",
                ".usage.lock",
                ".install-lock",
                ".uninstall.json",
                "current",
                "releases",
            ]
            .iter()
            .any(|allowed| name == *allowed)
            {
                return Err(invalid());
            }
        }
        if exists(&base.join("installation.json"))? {
            if read_record(base)? != self.journal.record {
                return Err(invalid());
            }
        } else if !resume {
            return Err(invalid());
        }
        let current = self
            .journal
            .current
            .to_str()
            .and_then(|path| path.strip_prefix("releases/"))
            .ok_or_else(invalid)?;
        if !release_name(current) || !self.installation.releases.contains_key(current) {
            return Err(invalid());
        }
        for (path, target) in self.links() {
            verify_link(&path, &target, resume)?;
        }
        self.inventory.verify(base, resume)
    }
    fn links(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut links = vec![(
            self.installation.base.join("current"),
            self.journal.current.clone(),
        )];
        for name in ["jailgun", "jailhard"] {
            links.push((
                self.installation.prefix.join("bin").join(name),
                format!("../lib/jailgun/current/bin/{name}").into(),
            ));
        }
        links
    }
}

fn read_record(base: &Path) -> Result<Record> {
    serde_json::from_slice(&regular_bytes(
        &base.join("installation.json"),
        8 * 1024 * 1024,
    )?)
    .map_err(|_| invalid())
}
fn verify_link(path: &Path, target: &Path, missing: bool) -> Result<bool> {
    directory(path.parent().ok_or_else(invalid)?)?;
    if missing && !exists(path)? {
        return Ok(false);
    }
    if fs::read_link(path)? != target {
        return Err(invalid());
    }
    Ok(true)
}

struct InstallerLock {
    path: PathBuf,
    owner: String,
}
impl InstallerLock {
    fn acquire(base: &Path) -> Result<Self> {
        let path = base.join(".install-lock");
        let mut options = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            options.mode(0o700);
        }
        options.create(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                InstallationError::Busy
            } else {
                error.into()
            }
        })?;
        let owner = serde_json::json!({"pid":std::process::id(),"operation":"uninstall","id":uuid::Uuid::new_v4()}).to_string();
        let guard = Self { path, owner };
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(guard.path.join("owner.json"))?;
        file.write_all(guard.owner.as_bytes())?;
        file.sync_all()?;
        Ok(guard)
    }
    fn release(&self) -> Result<()> {
        directory(&self.path)?;
        let owner = self.path.join("owner.json");
        if regular_bytes(&owner, 1024)? != self.owner.as_bytes() {
            return Err(invalid());
        }
        fs::remove_file(owner)?;
        fs::remove_dir(&self.path)?;
        Ok(())
    }
}
impl Drop for InstallerLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

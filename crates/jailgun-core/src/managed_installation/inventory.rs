use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Entry {
    pub path: String,
    pub bytes: u64,
    pub executable: bool,
    pub sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    files: Vec<Entry>,
}
pub(super) struct Inventory {
    pub files: BTreeMap<PathBuf, Entry>,
    pub directories: BTreeSet<PathBuf>,
}

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Inventory {
    pub fn parse(
        installation: &ManagedInstallation,
        manifests: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let mut result = Self {
            files: BTreeMap::new(),
            directories: BTreeSet::from([PathBuf::from("releases")]),
        };
        if manifests.len() != installation.releases.len() {
            return Err(invalid());
        }
        for (name, release) in &installation.releases {
            let bytes = manifests.get(name).ok_or_else(invalid)?.as_bytes();
            if bytes.len() > 8 * 1024 * 1024 || hash(bytes) != release.manifest_sha256 {
                return Err(invalid());
            }
            let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            if manifest.schema_version != 1
                || manifest.files.is_empty()
                || manifest.files.len() >= 100_000
            {
                return Err(invalid());
            }
            let root = PathBuf::from("releases").join(name);
            let mut seen = BTreeSet::new();
            for entry in manifest.files {
                if !safe_path(&entry.path)
                    || entry.path == "manifest.json"
                    || !digest(&entry.sha256)
                    || !seen.insert(entry.path.clone())
                {
                    return Err(invalid());
                }
                result.add(root.join(&entry.path), entry)?;
            }
            result.add(
                root.join("manifest.json"),
                Entry {
                    path: "manifest.json".into(),
                    bytes: bytes.len() as u64,
                    executable: false,
                    sha256: release.manifest_sha256.clone(),
                },
            )?;
        }
        if result
            .files
            .keys()
            .any(|path| result.directories.contains(path))
        {
            return Err(invalid());
        }
        Ok(result)
    }
    fn add(&mut self, path: PathBuf, entry: Entry) -> Result<()> {
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            self.directories.insert(parent.into());
        }
        if self.files.insert(path, entry).is_some() {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn verify(&self, base: &Path, resume: bool) -> Result<()> {
        self.walk(base, Path::new("releases"))?;
        for (path, entry) in &self.files {
            verify_file(base, path, entry, resume)?;
        }
        for path in &self.directories {
            match fs::symlink_metadata(base.join(path)) {
                Ok(metadata) if metadata.is_dir() => {}
                Err(error) if resume && error.kind() == io::ErrorKind::NotFound => {}
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }
    fn walk(&self, base: &Path, relative: &Path) -> Result<()> {
        let path = base.join(relative);
        if !exists(&path)? {
            return Ok(());
        }
        directory(&path)?;
        for child in fs::read_dir(&path)? {
            let relative = relative.join(child?.file_name());
            if self.directories.contains(&relative) {
                self.walk(base, &relative)?;
            } else if !self.files.contains_key(&relative) {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path
            .bytes()
            .any(|byte| byte < 32 || byte == 127 || byte == b'\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
pub(super) fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn verify_file(
    base: &Path,
    relative: &Path,
    entry: &Entry,
    missing: bool,
) -> Result<bool> {
    // Check ancestors on each destructive operation, including resumed removals.
    directory(base)?;
    for parent in relative
        .ancestors()
        .skip(1)
        .filter(|path| !path.as_os_str().is_empty())
    {
        let path = base.join(parent);
        if missing && !exists(&path)? {
            return Ok(false);
        }
        directory(&path)?;
    }
    let path = base.join(relative);
    if missing && !exists(&path)? {
        return Ok(false);
    }
    let mut file = regular_file(&path)?;
    let metadata = file.metadata()?;
    if metadata.len() != entry.bytes {
        return Err(invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if (metadata.permissions().mode() & 0o111 != 0) != entry.executable {
            return Err(invalid());
        }
    }
    let mut digest = Sha256::new();
    io::copy(&mut file, &mut digest)?;
    if format!("{:x}", digest.finalize()) != entry.sha256 {
        return Err(invalid());
    }
    Ok(true)
}

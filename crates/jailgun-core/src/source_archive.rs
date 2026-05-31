use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceArchiveConfig {
    pub enabled: bool,
    pub repo_url_env: String,
    pub ref_name: String,
    pub prefix: String,
    pub archive_filename: String,
    pub delete_after_upload: bool,
}

impl SourceArchiveConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.repo_url_env.trim().is_empty() {
            return Err(
                "source_archive.repo_url_env is required when source upload is enabled".into(),
            );
        }
        if !is_env_var_name(&self.repo_url_env) {
            return Err("source_archive.repo_url_env must be an environment variable name".into());
        }
        if self.ref_name.trim().is_empty() {
            return Err("source_archive.ref_name is required".into());
        }
        if self.ref_name.starts_with('-') || self.ref_name.contains("..") {
            return Err("source_archive.ref_name must be a safe git ref".into());
        }
        if !self.prefix.ends_with('/') {
            return Err("source_archive.prefix must end with /".into());
        }
        if self.prefix.starts_with('/') || self.prefix.contains("..") {
            return Err(
                "source_archive.prefix must be relative and must not contain traversal".into(),
            );
        }
        if !self.archive_filename.ends_with(".tar.gz") {
            return Err("source_archive.archive_filename must end with .tar.gz".into());
        }
        if self.archive_filename.contains('/') || self.archive_filename.contains("..") {
            return Err("source_archive.archive_filename must be a safe basename".into());
        }
        if !self.delete_after_upload {
            return Err(
                "source_archive.delete_after_upload must be true for temporary archives".into(),
            );
        }
        Ok(())
    }
}

fn is_env_var_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

impl Default for SourceArchiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            repo_url_env: "JAILGUN_SOURCE_REPO_URL".into(),
            ref_name: "HEAD".into(),
            prefix: "source/".into(),
            archive_filename: "source.tar.gz".into(),
            delete_after_upload: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_archive_shape() {
        let config = SourceArchiveConfig {
            enabled: true,
            repo_url_env: "JAILGUN_SOURCE_REPO_URL".into(),
            ref_name: "HEAD".into(),
            prefix: "source/".into(),
            archive_filename: "source.tar.gz".into(),
            delete_after_upload: true,
        };
        config.validate().expect("valid");
    }

    #[test]
    fn rejects_unsafe_prefix() {
        let config = SourceArchiveConfig {
            enabled: true,
            prefix: "../source/".into(),
            ..SourceArchiveConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_non_temporary_archive_policy() {
        let config = SourceArchiveConfig {
            enabled: true,
            delete_after_upload: false,
            ..SourceArchiveConfig::default()
        };
        assert!(config.validate().is_err());
    }
}

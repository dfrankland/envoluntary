use std::ffi::OsStr;

use once_cell::sync::Lazy;
use regex::Regex;
use semver::{Comparator, Op, Prerelease, Version, VersionReq};

use crate::nix_command;

static REQUIRED_NIX_VERSION: Lazy<VersionReq> = Lazy::new(|| VersionReq {
    comparators: vec![Comparator {
        op: Op::GreaterEq,
        major: 2,
        minor: Some(10),
        patch: Some(0),
        pre: Prerelease::EMPTY,
    }],
});

static SEMVER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([0-9]+\.[0-9]+\.[0-9]+)").unwrap());

pub fn check_nix_version() -> anyhow::Result<()> {
    check_nix_program_version(OsStr::new("nix"))
}

fn check_nix_program_version(nix_executable_path: impl AsRef<OsStr>) -> anyhow::Result<()> {
    let stdout_content = nix_command::nix_program(nix_executable_path.as_ref(), ["--version"])?;

    if stdout_content.is_empty() {
        return Err(anyhow::format_err!("`nix --version` failed to execute."));
    }

    let nix_version_match = SEMVER_RE
        .find(&stdout_content)
        .ok_or_else(|| anyhow::format_err!("SemVer from `nix --version` could not be found."))?;
    let nix_version = Version::parse(nix_version_match.as_str())?;

    if REQUIRED_NIX_VERSION.matches(&nix_version) {
        Ok(())
    } else {
        Err(anyhow::format_err!("`nix` version too old for flakes."))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write, os::unix::fs::PermissionsExt};

    use super::check_nix_program_version;

    fn create_nix_executable(file_contents: impl AsRef<[u8]>) -> tempfile::NamedTempFile {
        let mut nix_executable = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o777))
            .tempfile()
            .unwrap();
        nix_executable
            .as_file_mut()
            .write_all(file_contents.as_ref())
            .unwrap();
        nix_executable
    }

    #[test]
    fn test_error_on_exit_failure() {
        let nix_executable = create_nix_executable(r#"exit 1;"#);
        assert_eq!(
            check_nix_program_version(nix_executable.path())
                .unwrap_err()
                .to_string(),
            format!(
                "`{} --extra-experimental-features \"nix-command flakes\" --version` failed with error:\nprocess exited unsuccessfully: exit status: 1",
                nix_executable.path().to_string_lossy()
            )
        );
    }

    #[test]
    fn test_error_on_empty_stdout() {
        let nix_executable = create_nix_executable(r#"echo -n "";"#);
        assert_eq!(
            check_nix_program_version(nix_executable.path())
                .unwrap_err()
                .to_string(),
            "`nix --version` failed to execute."
        );
    }

    #[test]
    fn test_error_on_missing_semver() {
        let nix_executable = create_nix_executable(r#"echo "hello";"#);
        assert_eq!(
            check_nix_program_version(nix_executable.path())
                .unwrap_err()
                .to_string(),
            "SemVer from `nix --version` could not be found."
        );
    }

    #[test]
    fn test_error_on_too_old_version() {
        let nix_executable = create_nix_executable(r#"echo "nix (Nix) 0.0.0";"#);
        assert_eq!(
            check_nix_program_version(nix_executable.path())
                .unwrap_err()
                .to_string(),
            "`nix` version too old for flakes."
        );
    }

    #[test]
    fn test_version_matches_minimum() {
        let nix_executable = create_nix_executable(r#"echo "nix (Nix) 2.10.0";"#);
        check_nix_program_version(nix_executable.path()).unwrap();
    }

    #[test]
    fn test_version_matches_newer() {
        let nix_executable = create_nix_executable(r#"echo "nix (Nix) 2.30.0";"#);
        check_nix_program_version(nix_executable.path()).unwrap();
    }
}

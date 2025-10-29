use std::{
    ffi::OsStr,
    num,
    process::{Command, ExitStatus, Stdio},
};

use bstr::BString;
use shell_quote::Sh;

pub(crate) trait SimplifiedExitOk {
    fn simplified_exit_ok(&self) -> anyhow::Result<()>;
}

impl SimplifiedExitOk for ExitStatus {
    /// Simplified implementation of <https://github.com/rust-lang/rust/issues/84908>
    // TODO: Remove this and use `exit_ok` when it's stabilized.
    fn simplified_exit_ok(&self) -> anyhow::Result<()> {
        match num::NonZero::try_from(self.code().unwrap_or(-1)) {
            Ok(_) => Err(anyhow::format_err!(
                "process exited unsuccessfully: {}",
                &self
            )),
            Err(_) => Ok(()),
        }
    }
}

pub(crate) fn nix(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> anyhow::Result<String> {
    nix_program("nix", args)
}

pub(crate) fn nix_program(
    program: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> anyhow::Result<String> {
    let mut command = Command::new(program.as_ref());
    command
        .args(["--extra-experimental-features", "nix-command flakes"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output()?;
    output.status.simplified_exit_ok().map_err(|err| {
        anyhow::format_err!(
            "`{} {}` failed with error:\n{}",
            BString::new(Sh::quote_vec(command.get_program())),
            BString::new(bstr::join(
                " ",
                command
                    .get_args()
                    .map(|arg| { BString::new(Sh::quote_vec(arg)) })
                    .collect::<Vec<_>>()
            )),
            err
        )
    })?;
    let stdout_content = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout_content)
}

#[cfg(test)]
mod tests {
    use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};

    use super::nix_program;

    #[derive(Debug)]
    struct NixExecutable {
        // NB: `_dir` needed to prevent tempfile cleanup
        pub _dir: tempfile::TempDir,
        pub file_path: PathBuf,
    }

    impl NixExecutable {
        fn new(file_contents: &str) -> Self {
            // NB: Use a temp dir instead of a temp file since executing a file requires the file is
            // not open for writing / deleting
            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("nix");
            let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
            fs::write(&file_path, format!("#! {bash_path}\n{file_contents}")).unwrap();
            fs::set_permissions(&file_path, fs::Permissions::from_mode(0o777)).unwrap();
            Self {
                _dir: dir,
                file_path,
            }
        }
    }

    #[test]
    fn test_run_process_success() {
        let nix_executable = NixExecutable::new(
            r#"#! /bin/bash
exit 0;"#,
        );
        let stdout_content =
            nix_program(nix_executable.file_path, Vec::<&str>::with_capacity(0)).unwrap();
        assert_eq!(stdout_content, "");
    }

    #[test]
    fn test_run_process_failure() {
        let nix_executable = NixExecutable::new(r#"exit 1;"#);
        let result = nix_program(&nix_executable.file_path, Vec::<&str>::with_capacity(0));
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "`{} --extra-experimental-features nix-command' flakes'` failed with error:\nprocess exited unsuccessfully: exit status: 1",
                nix_executable.file_path.display()
            )
        );
    }

    #[test]
    fn test_run_process_stdout() {
        let nix_executable = NixExecutable::new(r#"echo "echoed";"#);
        let stdout_content =
            nix_program(nix_executable.file_path, Vec::<&str>::with_capacity(0)).unwrap();
        assert_eq!(stdout_content, "echoed\n");
    }
}

use std::{
    ffi::OsStr,
    num,
    process::{Command, ExitStatus, Stdio},
};

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
            command.get_program().to_string_lossy(),
            command
                .get_args()
                .map(|arg| {
                    let mut arg = arg.to_string_lossy().to_string();
                    if arg.contains(' ') {
                        arg = format!(r#""{}""#, arg);
                    }
                    arg
                })
                .collect::<Vec<_>>()
                .join(" "),
            err
        )
    })?;
    let stdout_content = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout_content)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write, os::unix::fs::PermissionsExt};

    use super::nix_program;

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
    fn test_run_process_success() {
        let nix_executable = create_nix_executable(r#"exit 0;"#);
        let stdout_content =
            nix_program(nix_executable.path(), Vec::<&str>::with_capacity(0)).unwrap();
        assert_eq!(stdout_content, "");
    }

    #[test]
    fn test_run_process_failure() {
        let nix_executable = create_nix_executable(r#"exit 1;"#);
        let result = nix_program(nix_executable.path(), Vec::<&str>::with_capacity(0));
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "`{} --extra-experimental-features \"nix-command flakes\"` failed with error:\nprocess exited unsuccessfully: exit status: 1",
                nix_executable.path().to_string_lossy()
            )
        );
    }

    #[test]
    fn test_run_process_stdout() {
        let nix_executable = create_nix_executable(r#"echo "echoed";"#);
        let stdout_content =
            nix_program(nix_executable.path(), Vec::<&str>::with_capacity(0)).unwrap();
        assert_eq!(stdout_content, "echoed\n");
    }
}

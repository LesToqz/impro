//! Child-process spawning that behaves the same way on every platform.
use std::ffi::OsStr;
use std::process::{Command, Stdio};

/// `CREATE_NO_WINDOW`: do not allocate a console for a console-subsystem child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A [`Command`] for `program`. Standard input defaults to the null device on every
/// platform: a GUI process has no usable stdin to inherit (on Windows the handle can be
/// invalid, which makes some runtimes abort), and no caller wants a child reading the
/// parent's terminal. Callers that feed the child call `.stdin(..)` themselves. On Windows
/// the child is also created without a console window, so running `git` from the GUI
/// never flashes a terminal.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Stdio;

    #[test]
    fn runs_the_program_with_arguments() {
        let out = command("sh")
            .args(["-c", "printf '%s' \"$0\"", "hello"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hello");
    }

    #[test]
    fn is_a_plain_command_on_unix() {
        let c = command("echo");
        assert_eq!(c.get_program(), "echo");
        assert_eq!(c.get_args().count(), 0);
        assert!(c.get_envs().next().is_none());
    }

    #[test]
    fn stdin_defaults_to_the_null_device() {
        // A plain `spawn` would inherit the parent's stdin; `cat` must see end-of-file at once.
        let child = command("cat").stdout(Stdio::piped()).spawn().unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn stdin_can_be_overridden_by_the_caller() {
        use std::io::Write;
        let mut child = command("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"fed").unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.stdout, b"fed");
    }

    #[test]
    fn missing_program_is_an_error_not_a_panic() {
        assert!(command("impro-no-such-program-xyz").output().is_err());
    }
}

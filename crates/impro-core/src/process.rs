//! Child-process spawning that behaves the same way on every platform.
use std::ffi::OsStr;
use std::process::Command;

/// `CREATE_NO_WINDOW`: do not allocate a console for a console-subsystem child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A [`Command`] for `program`. On Windows the child is created without a console window,
/// so running `git` from the GUI never flashes a terminal; on Unix this is `Command::new`.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
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
    fn missing_program_is_an_error_not_a_panic() {
        assert!(command("impro-no-such-program-xyz").output().is_err());
    }
}

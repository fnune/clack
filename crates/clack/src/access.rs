//! Granting Clack permission to read keyboards, and taking it back.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::{env, fs, io};

const RULE: &str = include_str!("../../../packaging/65-clack.rules");
const RULE_PATH: &str = "/etc/udev/rules.d/65-clack.rules";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleStatus {
    Installed,
    Outdated,
    Missing,
}

/// What a privileged run of Clack is being asked to do to the rule. Both
/// directions go through the same elevation, so revoking access is as ordinary
/// an act as granting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Allow,
    Revoke,
}

impl Change {
    /// The flag Clack passes to itself once polkit has raised it to root.
    pub fn flag(self) -> &'static str {
        match self {
            Self::Allow => "--write-udev-rule",
            Self::Revoke => "--remove-udev-rule",
        }
    }
}

/// Whether this run is the privileged half of one of these changes. Every binary
/// that can be re-executed through polkit has to answer this before it does
/// anything else: root running a second copy of the app instead of writing the
/// rule looks exactly like the user refusing the prompt.
pub fn privileged_change(arguments: impl IntoIterator<Item = String>) -> Option<Change> {
    arguments.into_iter().find_map(|argument| {
        [Change::Allow, Change::Revoke]
            .into_iter()
            .find(|change| change.flag() == argument)
    })
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("this needs to run as root; it is meant to be reached through `clack --grant-access`")]
    NotRoot,
    #[error("changing {RULE_PATH} failed: {0}")]
    Writing(#[source] io::Error),
    #[error("pkexec is not installed, so the rule cannot be changed for you")]
    NoPkexec,
    #[error("running pkexec failed: {0}")]
    Pkexec(#[source] io::Error),
    #[error("nothing was changed: {0}")]
    Declined(String),
    #[error("the change could not be made: {0}")]
    Failed(String),
    #[error("finding this executable failed: {0}")]
    Unlocatable(#[source] io::Error),
    #[error("{0} did not run: {1}")]
    Udevadm(&'static str, #[source] io::Error),
}

pub fn rule_text() -> &'static str {
    RULE
}

pub fn rule_path() -> &'static Path {
    Path::new(RULE_PATH)
}

pub fn status() -> RuleStatus {
    match fs::read_to_string(RULE_PATH) {
        Ok(installed) if installed == RULE => RuleStatus::Installed,
        Ok(_) => RuleStatus::Outdated,
        Err(_) => RuleStatus::Missing,
    }
}

pub fn install() -> Result<(), AccessError> {
    elevate(Change::Allow)
}

pub fn remove() -> Result<(), AccessError> {
    elevate(Change::Revoke)
}

/// Asks polkit to make the change, which prompts the user once. Their desktop
/// shows which program is asking and what it wants, and nothing stays elevated
/// afterwards.
pub fn elevate(change: Change) -> Result<(), AccessError> {
    if which("pkexec").is_none() {
        return Err(AccessError::NoPkexec);
    }

    let mut command = Command::new("pkexec");

    // pkexec refuses to run at all if SHELL is not listed in /etc/shells, which
    // it is not inside a Nix shell. It replaces the environment for the program
    // it runs anyway, so there is nothing to lose by dropping it.
    command.env_remove("SHELL");

    if env::var_os("APPIMAGE").is_some() {
        command.arg("sh").arg("-c").arg(script(change));
    } else {
        let executable = env::current_exe().map_err(AccessError::Unlocatable)?;
        command.arg(executable).arg(change.flag());
    }

    // stdin stays attached so a terminal authentication agent can still prompt.
    let outcome = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(Child::wait_with_output)
        .map_err(AccessError::Pkexec)?;

    // 126 and 127 are the two codes pkexec keeps for itself, covering a dismissed
    // prompt and a refusal alike: polkit reports a cancelled dialog as not being
    // authorised, so the two cannot be told apart and neither is a malfunction.
    // Everything else is the exit code of Clack's own privileged half.
    match outcome.status.code() {
        Some(0) => Ok(()),
        Some(126 | 127) => Err(AccessError::Declined(complaint(&outcome.stderr, None))),
        code => Err(AccessError::Failed(complaint(&outcome.stderr, code))),
    }
}

fn complaint(stderr: &[u8], code: Option<i32>) -> String {
    let said = String::from_utf8_lossy(stderr).trim().to_owned();

    if !said.is_empty() {
        return said;
    }

    match code {
        Some(code) => format!("it exited with status {code}"),
        None => "the prompt was closed".to_owned(),
    }
}

/// An AppImage runs from a FUSE mount that only the user who started it can
/// read, so root cannot re-execute Clack from there and has to be given the
/// work directly. Both the rule and its path are compile-time constants, so
/// there is no input for a caller to influence.
fn script(change: Change) -> String {
    let act = match change {
        Change::Allow => format!(
            "mkdir -p \"$(dirname '{RULE_PATH}')\"\n\
             cat > '{RULE_PATH}' <<'CLACK_RULE'\n\
             {rule}\n\
             CLACK_RULE\n",
            rule = RULE.trim_end()
        ),
        Change::Revoke => format!("rm -f '{RULE_PATH}'\n"),
    };

    format!(
        "set -e\n\
         {act}\
         udevadm control --reload\n\
         udevadm trigger --subsystem-match=input\n"
    )
}

/// Runs as root, having been re-entered through polkit. Writes or deletes one
/// fixed file and reloads udev. It takes no input beyond which of the two, so
/// there is nothing for a caller to influence.
pub fn apply_as_root(change: Change) -> Result<(), AccessError> {
    let path = Path::new(RULE_PATH);

    match change {
        Change::Allow => {
            if let Some(directory) = path.parent() {
                fs::create_dir_all(directory).map_err(AccessError::Writing)?;
            }

            fs::write(path, RULE).map_err(as_access_error)?;
        }
        // Already gone is the outcome the caller wanted, not a failure.
        Change::Revoke => {
            match fs::remove_file(path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    return Err(as_access_error(error));
                }
                _ => {}
            }

            close_existing_nodes();
        }
    }

    udevadm(&["control", "--reload"])?;
    udevadm(&["trigger", "--subsystem-match=input"])
}

/// Deleting the rule only stops the ACL being granted again. logind already put
/// one on every device node that existed, and nothing takes it off until the node
/// is recreated or the user logs out, so revoking has to strip those itself or it
/// changes nothing anyone can see.
///
/// Best effort: a system without `setfacl` still loses the rule, and access then
/// ends at logout. Only keyboards are touched, and the caller's `udevadm trigger`
/// puts back anything another rule still tags.
fn close_existing_nodes() {
    let (Some(user), Some(setfacl)) = (invoking_user(), which("setfacl")) else {
        return;
    };

    let Ok(devices) = crate::input::survey() else {
        return;
    };

    for device in devices {
        if !matches!(device.access, crate::input::Access::Keyboard) {
            continue;
        }

        let _ = Command::new(&setfacl)
            .arg("-x")
            .arg(format!("u:{user}"))
            .arg(&device.path)
            .status();
    }
}

/// Who asked, rather than who is running. pkexec keeps the original user's id
/// here, and root's own id is no use for taking an ACL away.
fn invoking_user() -> Option<String> {
    ["PKEXEC_UID", "SUDO_UID"]
        .into_iter()
        .filter_map(|name| env::var(name).ok())
        .find(|value| is_user_id(value))
}

/// The value reaches a command line, so it has to be an id and nothing else.
fn is_user_id(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

fn as_access_error(error: io::Error) -> AccessError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        AccessError::NotRoot
    } else {
        AccessError::Writing(error)
    }
}

fn udevadm(arguments: &[&str]) -> Result<(), AccessError> {
    let program = which("udevadm").unwrap_or_else(|| PathBuf::from("udevadm"));

    Command::new(program)
        .args(arguments)
        .status()
        .map_err(|error| AccessError::Udevadm("udevadm", error))
        .map(drop)
}

fn which(program: &str) -> Option<PathBuf> {
    let common = [
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "/run/current-system/sw/bin",
    ];

    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(common.iter().map(PathBuf::from))
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_targets_keyboards_and_grants_access_to_the_active_seat() {
        assert!(RULE.contains("ID_INPUT_KEYBOARD"));
        assert!(RULE.contains("uaccess"));
    }

    #[test]
    fn the_script_writes_the_same_rule_the_binary_would() {
        let script = script(Change::Allow);

        assert!(script.contains(RULE_PATH));
        assert!(script.contains(RULE.trim_end()));
        assert!(script.contains("udevadm control --reload"));
    }

    #[test]
    fn revoking_removes_the_rule_rather_than_rewriting_it() {
        let script = script(Change::Revoke);

        assert!(script.contains(&format!("rm -f '{RULE_PATH}'")));
        assert!(
            !script.contains("uaccess"),
            "it should write no rule at all"
        );
        assert!(script.contains("udevadm control --reload"));
    }

    #[test]
    fn the_two_directions_do_not_share_a_flag() {
        assert_ne!(Change::Allow.flag(), Change::Revoke.flag());
    }

    #[test]
    fn only_digits_count_as_a_user_id() {
        assert!(is_user_id("1000"));
        assert!(!is_user_id(""));
        assert!(!is_user_id("1000 -x"), "an id never carries an argument");
        assert!(!is_user_id("fausto"), "setfacl would read a name as a name");
    }

    #[test]
    fn the_rule_sorts_after_the_one_that_identifies_keyboards() {
        let name = rule_path().file_name().and_then(|name| name.to_str());

        assert!(name.is_some_and(|name| name > "60-input-id.rules"));
        assert!(name.is_some_and(|name| name < "73-seat-late.rules"));
    }
}

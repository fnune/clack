//! Granting Clack permission to read keyboards.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

const RULE: &str = include_str!("../../../packaging/65-clack.rules");
const RULE_PATH: &str = "/etc/udev/rules.d/65-clack.rules";

/// The flag Clack passes to itself once polkit has raised it to root.
pub const PRIVILEGED_FLAG: &str = "--write-udev-rule";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleStatus {
    Installed,
    Outdated,
    Missing,
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("this needs to run as root; it is meant to be reached through `clack --install-rule`")]
    NotRoot,
    #[error("writing {RULE_PATH} failed: {0}")]
    Writing(#[source] io::Error),
    #[error("pkexec is not installed, so the rule cannot be installed for you")]
    NoPkexec,
    #[error("running pkexec failed: {0}")]
    Pkexec(#[source] io::Error),
    #[error("the request to install the rule was declined")]
    Declined,
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

/// Asks polkit to install the rule, which prompts the user once. Their desktop
/// shows which program is asking and what it wants, and nothing stays elevated
/// afterwards.
pub fn install() -> Result<(), AccessError> {
    let executable = env::current_exe().map_err(AccessError::Unlocatable)?;

    if which("pkexec").is_none() {
        return Err(AccessError::NoPkexec);
    }

    let outcome = Command::new("pkexec")
        .arg(executable)
        .arg(PRIVILEGED_FLAG)
        .status()
        .map_err(AccessError::Pkexec)?;

    if outcome.success() {
        Ok(())
    } else {
        Err(AccessError::Declined)
    }
}

/// Runs as root, having been re-entered through polkit. Writes one fixed file
/// with fixed contents and reloads udev. It takes no input, so there is nothing
/// for a caller to influence.
pub fn write_rule_as_root() -> Result<(), AccessError> {
    let path = Path::new(RULE_PATH);
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(AccessError::Writing)?;
    }

    fs::write(path, RULE).map_err(|error| {
        if error.kind() == io::ErrorKind::PermissionDenied {
            AccessError::NotRoot
        } else {
            AccessError::Writing(error)
        }
    })?;

    udevadm(&["control", "--reload"])?;
    udevadm(&["trigger", "--subsystem-match=input"])
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
    fn the_rule_sorts_after_the_one_that_identifies_keyboards() {
        let name = rule_path().file_name().and_then(|name| name.to_str());

        assert!(name.is_some_and(|name| name > "60-input-id.rules"));
        assert!(name.is_some_and(|name| name < "73-seat-late.rules"));
    }
}

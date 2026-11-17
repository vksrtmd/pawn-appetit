use log::info;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::process::Command;

use crate::error::Error;

#[derive(Debug, Type, Serialize, Deserialize)]
pub struct PackageManagerResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

fn validate_package_name(name: &str) -> Result<(), Error> {
    let mut chars = name.chars();
    let ok = name.len() <= 100
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+._@/-".contains(c));
    if ok {
        Ok(())
    } else {
        Err(Error::PackageManager(format!(
            "Invalid package name: {name:?}"
        )))
    }
}

type Argv = (&'static str, Vec<String>);

fn argv(program: &'static str, args: &[&str], pkg: &str) -> Argv {
    let mut v: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    v.push("--".to_string());
    v.push(pkg.to_string());
    (program, v)
}

/// Linux managers run through `pkexec` because there is no terminal for `sudo` to prompt on.
fn install_argv(manager: &str, pkg: &str) -> Result<Argv, Error> {
    validate_package_name(pkg)?;
    match manager {
        "brew" => Ok(argv("brew", &["install"], pkg)),
        "apt" => Ok(argv("pkexec", &["apt-get", "install", "-y"], pkg)),
        "dnf" => Ok(argv("pkexec", &["dnf", "install", "-y"], pkg)),
        "pacman" => Ok(argv("pkexec", &["pacman", "-S", "--noconfirm"], pkg)),
        _ => Err(Error::PackageManager(
            "Unsupported package manager".to_string(),
        )),
    }
}

fn check_argv(manager: &str, pkg: &str) -> Result<Argv, Error> {
    validate_package_name(pkg)?;
    match manager {
        "brew" => Ok(argv("brew", &["list"], pkg)),
        "apt" => Ok(argv("dpkg", &["-l"], pkg)),
        "dnf" => Ok(argv("dnf", &["list", "installed"], pkg)),
        "pacman" => Ok(argv("pacman", &["-Q"], pkg)),
        _ => Err(Error::PackageManager(
            "Unsupported package manager".to_string(),
        )),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn check_package_manager_available(manager: String) -> Result<bool, Error> {
    let program = match manager.as_str() {
        "brew" => "brew",
        "apt" => "apt",
        "dnf" => "dnf",
        "pacman" => "pacman",
        _ => return Ok(false),
    };
    Ok(Command::new(program)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false))
}

#[tauri::command]
#[specta::specta]
pub async fn install_package(
    manager: String,
    package_name: String,
) -> Result<PackageManagerResult, Error> {
    let (program, args) = install_argv(&manager, &package_name)?;
    info!("Installing package {} using {}", package_name, manager);

    let output = tokio::process::Command::new(program)
        .args(&args)
        .output()
        .await
        .map_err(|e| Error::PackageManager(format!("Failed to install package: {}", e)))?;
    Ok(PackageManagerResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn check_package_installed(manager: String, package_name: String) -> Result<bool, Error> {
    let (program, args) = check_argv(&manager, &package_name)?;
    match tokio::process::Command::new(program)
        .args(&args)
        .output()
        .await
    {
        Ok(output) => Ok(output.status.success()),
        Err(e) => {
            info!("Error checking package installation: {}", e);
            Ok(false)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn find_executable_path(executable_name: String) -> Result<Option<String>, Error> {
    let output = Command::new("which").arg(&executable_name).output();

    match output {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                Ok(Some(path))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_lc0_installs_through_pkexec_with_name_after_double_dash() {
        let (program, args) = install_argv("apt", "lc0").unwrap();
        assert_eq!(program, "pkexec");
        assert_eq!(args, ["apt-get", "install", "-y", "--", "lc0"]);
    }

    #[test]
    fn dnf_pacman_brew_argv() {
        assert_eq!(
            install_argv("dnf", "lc0").unwrap(),
            (
                "pkexec",
                vec!["dnf", "install", "-y", "--", "lc0"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            )
        );
        assert_eq!(
            install_argv("pacman", "lc0").unwrap().1,
            ["pacman", "-S", "--noconfirm", "--", "lc0"]
        );
        assert_eq!(
            install_argv("brew", "lc0").unwrap(),
            (
                "brew",
                vec!["install".to_string(), "--".into(), "lc0".into()]
            )
        );
    }

    #[test]
    fn check_argv_passes_name_after_double_dash() {
        assert_eq!(check_argv("apt", "lc0").unwrap().1, ["-l", "--", "lc0"]);
    }

    #[test]
    fn hostile_package_names_are_rejected() {
        for bad in [
            "-rf",
            "--foo",
            "lc0; rm",
            "",
            "a b",
            "lc0\n",
            &"a".repeat(101),
        ] {
            assert!(install_argv("apt", bad).is_err(), "{bad:?}");
            assert!(check_argv("apt", bad).is_err(), "{bad:?}");
        }
        assert!(install_argv("apt", "lib+x.y_z@1/2-3").is_ok());
    }

    #[test]
    fn unknown_manager_is_rejected() {
        assert!(install_argv("npm", "lc0").is_err());
        assert!(check_argv("npm", "lc0").is_err());
    }
}

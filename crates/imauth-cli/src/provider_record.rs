use crate::cli_support::ProviderAction;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) const RECORDER_SOURCE: &str = include_str!("../../../scripts/provider-record.mjs");
pub(crate) const REDACTION_SOURCE: &str =
    include_str!("../../../scripts/provider-record-redaction.mjs");
const PLAYWRIGHT_VERSION: &str = "1.62.1";
const PACKAGE_SOURCE: &str = r#"{
  "private": true,
  "type": "module",
  "dependencies": { "playwright": "1.62.1" }
}
"#;

pub(crate) fn run(action: ProviderAction) -> Result<(), Box<dyn std::error::Error>> {
    let ProviderAction::Record {
        url,
        domain,
        cdp_url,
        output_root,
        headless,
        auto_finish,
        deep,
    } = action;

    let runtime = prepare_runtime(cdp_url.is_none())?;
    let mut arguments = vec![OsString::from("--url"), OsString::from(url)];
    if let Some(domain) = domain {
        arguments.extend([OsString::from("--domain"), OsString::from(domain)]);
    }
    if let Some(cdp_url) = cdp_url {
        arguments.extend([OsString::from("--cdp-url"), OsString::from(cdp_url)]);
    }
    arguments.extend([
        OsString::from("--output-root"),
        output_root.into_os_string(),
    ]);
    if headless {
        arguments.push(OsString::from("--headless"));
    }
    if auto_finish {
        arguments.push(OsString::from("--auto-finish"));
    }
    if deep {
        arguments.push(OsString::from("--deep"));
    }
    let status = Command::new("node")
        .arg(runtime.join("provider-record.mjs"))
        .args(arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("Failed to start Node.js provider recorder: {error}"))?;
    if !status.success() {
        return Err(format!("Provider recorder exited with {status}").into());
    }
    Ok(())
}

fn prepare_runtime(install_browser: bool) -> Result<PathBuf, Box<dyn std::error::Error>> {
    require_command("node", "Install Node.js 20 or newer")?;
    require_command("npm", "Install npm with Node.js")?;
    let directory = runtime_directory()?;
    std::fs::create_dir_all(&directory)?;
    write_if_changed(&directory.join("package.json"), PACKAGE_SOURCE)?;
    write_if_changed(&directory.join("provider-record.mjs"), RECORDER_SOURCE)?;
    write_if_changed(
        &directory.join("provider-record-redaction.mjs"),
        REDACTION_SOURCE,
    )?;

    if !directory
        .join("node_modules/playwright/package.json")
        .is_file()
    {
        run_npm(
            &directory,
            &["install", "--omit=dev", "--no-audit", "--no-fund"],
        )?;
    }
    if install_browser {
        run_npm(
            &directory,
            &["exec", "--", "playwright", "install", "chromium"],
        )?;
    }
    Ok(directory)
}

fn runtime_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(value) = std::env::var_os("IMAUTH_PROVIDER_RECORDER_CACHE") {
        return Ok(PathBuf::from(value).join(PLAYWRIGHT_VERSION));
    }
    let base = if cfg!(target_os = "macos") {
        home_directory()?.join("Library/Caches")
    } else if let Some(value) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(value)
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("LOCALAPPDATA is not set")?
    } else {
        home_directory()?.join(".cache")
    };
    Ok(base
        .join("imauth/provider-recorder")
        .join(PLAYWRIGHT_VERSION))
}

fn home_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

fn require_command(name: &str, help: &str) -> Result<(), Box<dyn std::error::Error>> {
    match Command::new(name).arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(format!("{name} is required. {help}.").into()),
    }
}

fn run_npm(directory: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("npm")
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("npm {} failed with {status}", arguments.join(" ")).into())
    }
}

fn write_if_changed(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let current = std::fs::read_to_string(path).ok();
    if current.as_deref() != Some(content) {
        std::fs::write(path, content)?;
    }
    Ok(())
}

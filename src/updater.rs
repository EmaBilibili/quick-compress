use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_API: &str = "https://api.github.com/repos/EmaBilibili/quick-compress/releases/latest";

#[derive(Deserialize, Debug, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug)]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: Option<String>,
    pub release_url: String,
}

/// Query GitHub API for the latest release
pub fn check_for_updates() -> Result<UpdateCheckResult, String> {
    let response_result = ureq::get(REPO_API)
        .set("User-Agent", "QuickCompress-App")
        .call();

    let response: ReleaseInfo = match response_result {
        Ok(resp) => resp
            .into_json()
            .map_err(|e| format!("Failed to parse release info: {}", e))?,
        Err(ureq::Error::Status(404, _)) => {
            // No releases published yet on the GitHub repository
            return Ok(UpdateCheckResult {
                has_update: false,
                latest_version: CURRENT_VERSION.to_string(),
                release_notes: "No releases published yet on GitHub.".to_string(),
                download_url: None,
                release_url: "https://github.com/EmaBilibili/quick-compress/releases".to_string(),
            });
        }
        Err(e) => return Err(format!("Failed to connect to GitHub: {}", e)),
    };

    let latest_tag = response.tag_name.trim_start_matches('v').to_string();
    let current = CURRENT_VERSION.trim_start_matches('v');

    let has_update = latest_tag != current && is_newer_version(&latest_tag, current);

    // Look specifically for the raw standalone executable binary first
    let download_url = response
        .assets
        .iter()
        .find(|a| a.name == "quick-compress-x86_64" || a.name == "quick-compress")
        .or_else(|| {
            response
                .assets
                .iter()
                .find(|a| a.name.contains("quick-compress") && !a.name.ends_with(".tar.gz") && !a.name.ends_with(".zip"))
        })
        .map(|a| a.browser_download_url.clone());

    Ok(UpdateCheckResult {
        has_update,
        latest_version: latest_tag,
        release_notes: response.body.unwrap_or_else(|| "Bug fixes and improvements".to_string()),
        download_url,
        release_url: response.html_url,
    })
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|part| part.split('-').next().unwrap_or(part).parse().ok())
            .collect()
    };

    let latest_parts = parse(latest);
    let current_parts = parse(current);

    latest_parts > current_parts
}

/// Download replacement binary, atomically replace current running executable, and relaunch
pub fn perform_self_update_and_restart(download_url: &str) -> Result<(), String> {
    let current_exe = env::current_exe().map_err(|e| format!("Cannot locate current executable: {}", e))?;

    let resp = ureq::get(download_url)
        .set("User-Agent", "QuickCompress-App")
        .call()
        .map_err(|e| format!("Failed to download update: {}", e))?;

    let temp_exe = current_exe.with_extension("new_update");
    {
        let mut file = File::create(&temp_exe).map_err(|e| format!("Cannot create temp file: {}", e))?;

        io::copy(&mut resp.into_reader(), &mut file)
            .map_err(|e| format!("Failed to write downloaded binary: {}", e))?;

        file.sync_all().map_err(|e| format!("Failed to flush binary: {}", e))?;
    }

    // Make executable (chmod +x)
    let mut perms = fs::metadata(&temp_exe)
        .map_err(|e| format!("Failed to get permissions: {}", e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&temp_exe, perms)
        .map_err(|e| format!("Failed to set executable permission: {}", e))?;

    // Atomic replace
    fs::rename(&temp_exe, &current_exe)
        .map_err(|e| format!("Failed to replace executable: {}", e))?;

    // Relaunch the new executable and exit
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to restart updated app: {}", e))?;

    std::process::exit(0);
}

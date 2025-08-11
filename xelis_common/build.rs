// build.rs
//
// This script runs before the build process.
// It determines the current Git commit hash and combines it with the crate version
// to generate a unique `BUILD_VERSION`, exposed as a compile-time environment variable.

use std::env;
use std::process::Command;

use std::process::Command;

/// Entry point for Cargo build script
fn main() {
    // Try env var first (CI/CD override), otherwise fallback to Git, otherwise "unknown"
    let commit_hash = option_env!("XELIS_COMMIT_HASH")
        .map(|hash| hash.chars().take(7).collect::<String>())
        .or_else(get_git_commit_hash_safe)
        .unwrap_or_else(|| "unknown".to_string());

    let version = env!("CARGO_PKG_VERSION");
    let build_version = format!("{version}-{commit_hash}");

    // Re-run build script if this env changes (important for CI)
    println!("cargo:rerun-if-env-changed=XELIS_COMMIT_HASH");

    // Set BUILD_VERSION as env var at compile time
    println!("cargo:rustc-env=BUILD_VERSION={build_version}");
}

/// Try to get Git commit hash (short). Return None if git isn't available or fails.
fn get_git_commit_hash_safe() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?; // Return None if command fails to run

    if !output.status.success() {
        return None;
    }

    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string(),
    )
}

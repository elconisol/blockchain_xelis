// build.rs
//
// This script runs before the build process.
// It determines the current Git commit hash and combines it with the crate version
// to generate a unique `BUILD_VERSION`, exposed as a compile-time environment variable.

use std::env;
use std::process::Command;

fn main() {
    // Try to read commit hash from environment (CI/CD override)
    let commit_hash = option_env!("XELIS_COMMIT_HASH")
        .map(|hash| hash[..7].to_string())
        .unwrap_or_else(get_git_commit_hash);

    let version = env!("CARGO_PKG_VERSION");
    let build_version = format!("{version}-{commit_hash}");

    // Re-run build script if this env changes (useful in CI workflows)
    println!("cargo:rerun-if-env-changed=XELIS_COMMIT_HASH");

    // Set BUILD_VERSION as env var at compile time
    println!("cargo:rustc-env=BUILD_VERSION={build_version}");
}

/// Fallback function: get the current Git commit hash (short)
fn get_git_commit_hash() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("Failed to execute `git rev-parse`");

    if !output.status.success() {
        panic!(
            "Git command returned non-zero exit status: {}",
            output.status
        );
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string()
}

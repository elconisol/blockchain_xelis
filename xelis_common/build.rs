// build.rs
//
// This file is executed before the build process.
// It fetches the current Git commit hash and sets it as part of the build version,
// which is then exposed as an environment variable (`BUILD_VERSION`).

use std::env;
use std::process::Command;

fn main() {
    // Try to get the commit hash from the environment variable (e.g., CI/CD pipelines)
    let commit_hash = option_env!("XELIS_COMMIT_HASH").map(|hash| hash[..7].to_string()).unwrap_or_else(|| {
        // Fallback: get the short Git commit hash locally
        let output = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .expect("Failed to execute git command");

        if !output.status.success() {
            panic!("Git command failed with status: {}", output.status);
        }

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    });

    // Construct the full build version (e.g., 1.17.0-ab12cd3)
    let build_version = format!("{}-{}", env!("CARGO_PKG_VERSION"), commit_hash);

    // Instruct Cargo to re-run this script if BUILD_VERSION changes externally
    println!("cargo:rerun-if-env-changed=BUILD_VERSION");

    // Pass build version as environment variable at compile time
    println!("cargo:rustc-env=BUILD_VERSION={}", build_version);
}

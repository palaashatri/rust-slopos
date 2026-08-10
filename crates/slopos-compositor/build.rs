use std::process::Command;

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn main() {
    println!("cargo:rerun-if-env-changed=SLOPOS_COMMIT");
    println!("cargo:rerun-if-env-changed=SLOPOS_BRANCH");

    let commit = std::env::var("SLOPOS_COMMIT")
        .ok()
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .or_else(|| command_output("git", &["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "0000000000000000000000000000000000000000".to_owned());
    let branch = std::env::var("SLOPOS_BRANCH")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| command_output("git", &["branch", "--show-current"]))
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=SLOPOS_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SLOPOS_BUILD_BRANCH={branch}");
}

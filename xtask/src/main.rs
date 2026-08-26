use std::env;
use std::ffi::OsString;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some("check") if env::args().nth(2).is_none() => match check() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: cargo xtask check");
            ExitCode::from(2)
        }
    }
}

fn check() -> Result<(), String> {
    run("format", &["fmt", "--all", "--check"], &[])?;
    run(
        "lint",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        &[],
    )?;
    run(
        "documentation",
        &[
            "doc",
            "--locked",
            "--workspace",
            "--no-deps",
            "--all-features",
            "--document-private-items",
        ],
        &[("RUSTDOCFLAGS", "-D warnings")],
    )?;
    run(
        "tests and coverage",
        &[
            "llvm-cov",
            "--locked",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-features",
            "--fail-under-regions",
            "95",
            "--fail-under-functions",
            "95",
            "--fail-under-lines",
            "95",
        ],
        &[],
    )?;
    run("dependency audit", &["audit"], &[])
}

fn run(label: &str, arguments: &[&str], environment: &[(&str, &str)]) -> Result<(), String> {
    eprintln!("==> {label}");
    let status = Command::new(cargo())
        .args(arguments)
        .envs(environment.iter().copied())
        .status()
        .map_err(|error| format!("failed to start {label}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with {status}"))
    }
}

fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

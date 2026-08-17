use std::process::ExitCode;

fn main() -> ExitCode {
    match knoxmancer::run(std::env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => ExitCode::from(error.exit_code()),
    }
}

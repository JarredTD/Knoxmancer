use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(knoxmancer::execute(std::env::args_os()))
}

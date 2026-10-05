//! Command-line transport over the same dispatcher shipped in host bindings.
use std::process::ExitCode;

fn main() -> ExitCode {
    match nirs4all::archive_command::execute_archive_command(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Core archive refusal: {error}");
            ExitCode::FAILURE
        }
    }
}

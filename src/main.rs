fn main() {
    if let Err(err) = fde::cli::run() {
        let exit_code = err.downcast_ref::<fde::cli::CliExitError>().map_or_else(
            || {
                err.downcast_ref::<fde::ImplementationRunError>()
                    .map_or(1, fde::ImplementationRunError::exit_code)
            },
            fde::cli::CliExitError::code,
        );
        let already_reported = err
            .downcast_ref::<fde::ImplementationRunError>()
            .is_some_and(|error| error.partial_report().is_some());
        if !already_reported {
            eprintln!("error: {err:#}");
        }
        std::process::exit(exit_code);
    }
}

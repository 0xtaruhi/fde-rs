fn main() {
    if let Err(err) = fde::cli::run_place_wrapper() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

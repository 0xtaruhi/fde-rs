fn main() {
    if let Err(err) = fde::cli::run_map_wrapper() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

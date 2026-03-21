fn main() {
    if let Err(err) = fde::cli::run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

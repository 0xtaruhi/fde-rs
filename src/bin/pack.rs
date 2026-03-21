fn main() {
    if let Err(err) = fde::cli::run_pack_wrapper() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

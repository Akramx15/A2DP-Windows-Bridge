//! a2dpwb-core.exe: the resident A2DPWB process. `--cli` runs the command
//! line instead (same core, no tray and no IPC).

#![forbid(unsafe_code)]

mod cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("A2DPWB_LOG").unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    if args.iter().any(|a| a == "--cli") {
        std::process::exit(cli::run(&args));
    }
    // The resident mode (tray, IPC for the UI) comes with the UI; until then
    // the command line is the way to stream
    eprintln!("a2dpwb-core: the resident mode is not implemented yet; use --cli (see --cli -h)");
    std::process::exit(2);
}

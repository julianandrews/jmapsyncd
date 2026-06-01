use clap::Parser;

fn main() {
    let args = jmapsyncd::args::Args::parse();
    let command = args.command.unwrap_or_default();
    jmapsyncd::logging::init(args.log_level);

    let overrides: jmapsyncd::config::Overrides = args.overrides.into();
    let _config = jmapsyncd::config::Config::load(args.config_file.as_deref(), &overrides)
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });

    match command {
        jmapsyncd::args::Command::Sync { account } => {
            match account {
                None => log::info!("syncing all accounts"),
                Some(account) => log::info!("syncing account: {:?}", account),
            }
            if args.dry_run {
                log::info!("dry-run mode, no changes will be applied");
            }
        }
        jmapsyncd::args::Command::Daemon => {
            log::info!("starting daemon mode");
            if args.dry_run {
                log::info!("dry-run mode, no changes will be applied");
            }
        }
    }
}

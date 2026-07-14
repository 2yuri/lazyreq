use std::env;
use std::process::exit;

use colored::*;
use config::{Config, Mode};
use lazyreq::LazyReq;

mod cache;
mod config;
mod functions;
mod history;
mod import;
mod lazyreq;
mod request;
mod theme;
mod timest;
mod tui;
mod update;
mod vault;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if let Err(msg) = run(&args).await {
        eprintln!("{} {}", "error:".red().bold(), msg);
        exit(1);
    }
}

async fn run(args: &[String]) -> Result<(), String> {
    let config = Config::new(args)?;

    if let Mode::Version = config.mode {
        println!("lazyreq {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Mode::Import(command) = &config.mode {
        print!("{}", import::curl_to_lreq(command)?);
        return Ok(());
    }

    if let Mode::Tui(path) = config.mode {
        return tui::run(path).await;
    }

    if let Mode::Update = config.mode {
        return update::self_update().await;
    }

    if let Mode::History(opts) = &config.mode {
        // History only needs the file's identity, not a successful parse —
        // past runs stay readable even while the file is mid-edit.
        let id = (!config.target.is_empty()).then_some(config.target.as_str());
        return history::show(&config.filename, id, opts);
    }

    let mut lazyreq = LazyReq::new();
    lazyreq.from_file(config.filename)?;

    match config.mode {
        Mode::Import(_) | Mode::Version | Mode::History(_) | Mode::Tui(_) | Mode::Update => unreachable!(),
        Mode::List => lazyreq.list(),
        Mode::ExportCurl => lazyreq.export_curl(config.target).await?,
        Mode::Retry(run_id) => lazyreq.retry(run_id).await?,
        Mode::Run => lazyreq.do_request(config.target).await?,
    }

    Ok(())
}

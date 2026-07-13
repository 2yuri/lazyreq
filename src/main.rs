use std::env;
use std::process::exit;

use colored::*;
use config::{Config, Mode};
use lazyreq::LazyReq;

mod cache;
mod config;
mod functions;
mod import;
mod lazyreq;
mod request;
mod timest;

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

    if let Mode::Import(command) = &config.mode {
        print!("{}", import::curl_to_lreq(command)?);
        return Ok(());
    }

    let mut lazyreq = LazyReq::new();
    lazyreq.from_file(config.filename)?;

    match config.mode {
        Mode::Import(_) => unreachable!(),
        Mode::List => lazyreq.list(),
        Mode::ExportCurl => lazyreq.export_curl(config.target).await?,
        Mode::Run => lazyreq.do_request(config.target).await?,
    }

    Ok(())
}

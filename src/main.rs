use std::env;

use config::Config;
use lazyreq::LazyReq;

mod request;
mod lazyreq;
mod config;



#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let config = Config::new(&args);

    let mut lazyreq = LazyReq::new();
    lazyreq.from_file(config.filename);
    lazyreq.do_request(config.target).await;
}
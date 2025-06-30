use clap::Parser;
use monoio_tcp_proxy::{run_proxy, Args};

#[monoio::main(entries = 512, timer_enabled = false)]
async fn main() {
    env_logger::builder().format_timestamp_nanos().init();
    let args = Args::parse();
    run_proxy(args).await;
}

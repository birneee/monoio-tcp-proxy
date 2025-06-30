use std::net;
use clap::Parser;
use monoio::io::{AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt, Splitable};
use monoio::net::{TcpListener, TcpStream};
use log::{error, info};

#[derive(Parser)]
pub struct Args {
    #[arg(long, help = "e.g. 0.0.0.0:50005")]
    pub bind: net::SocketAddr,
    #[arg(long, help = "e.g. 1.2.3.4:80")]
    pub target: net::SocketAddr,
}

pub async fn run_proxy(args: Args) {
    let listener = TcpListener::bind(args.bind)
        .unwrap_or_else(|_| panic!("Unable to bind to {}", args.bind));
    info!("listening on {}", listener.local_addr().unwrap());
    info!("target is {}", args.target);
    loop {
        let in_conn = if let Ok((in_conn, _addr)) = listener.accept().await {
            in_conn
        } else {
            error!("accept connection failed");
            continue;
        };
        let out_conn = if let Ok(out_conn) =  TcpStream::connect(args.target).await {
            out_conn
        } else {
            error!("dial outbound connection failed");
            continue;
        };
        let relay_name = format!("{} <-> {}", in_conn.peer_addr().unwrap(), out_conn.peer_addr().unwrap());
        info!("{relay_name}: connect");
        monoio::spawn(async move {
            let (mut in_r, mut in_w) = in_conn.into_split();
            let (mut out_r, mut out_w) = out_conn.into_split();
            let _ = monoio::join!(
                        copy_one_direction(&mut in_r, &mut out_w),
                        copy_one_direction(&mut out_r, &mut in_w),
                    );
            info!("{relay_name}: close");
        });
    }
}

pub async fn copy_one_direction<FROM: AsyncReadRent, TO: AsyncWriteRent>(
    mut from: FROM,
    to: &mut TO,
) -> Result<Vec<u8>, std::io::Error> {
    let mut buf = Vec::with_capacity(8 * 1024);
    let mut res;
    loop {
        // read
        (res, buf) = from.read(buf).await;
        if res? == 0 {
            return Ok(buf);
        }

        // write all
        (res, buf) = to.write_all(buf).await;
        res?;

        // clear
        buf.clear();
    }
}
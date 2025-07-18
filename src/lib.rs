use std::{fs, io, net};
use std::ffi::{c_char, CStr, OsString};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::str::FromStr;
use clap::Parser;
use monoio::io::Splitable;
use monoio::net::{TcpListener, TcpStream};
use log::{debug, error, info};
use nix::sys::socket::sockopt;
use crate::copy::copy;

mod copy;

pub const DEFAULT_COPY_BUF: usize = 128 * 1024;

#[derive(Parser)]
pub struct Args {
    #[arg(long, help = "e.g. 0.0.0.0:50005", value_name = "HOST:PORT")]
    pub bind: net::SocketAddr,
    #[arg(long, help = "e.g. 1.2.3.4:80", value_name = "HOST:PORT")]
    pub target: net::SocketAddr,
    #[arg(long, help = "TCP receive buffer size", value_name = "BYTES")]
    pub recv_buf: Option<usize>,
    #[arg(long, help = "TCP send buffer size", value_name = "BYTES")]
    pub send_buf: Option<usize>,
    #[arg(long = "cc", help = "Which system TCP congestion controller to use", value_name = "NAME")]
    pub congestion_controller: Option<String>,
    #[arg(long, help = "Copy buffer size", value_name = "BYTES", default_value_t = DEFAULT_COPY_BUF)]
    pub copy_buf: usize,
}

fn configure_socket(socket: &TcpStream, args: &Args) {
    let fd = socket.as_raw_fd();
    if let Some(send_buf) = args.send_buf {
        nix::sys::socket::setsockopt(fd, sockopt::SndBuf, &send_buf).unwrap();
        assert_eq!(
            nix::sys::socket::getsockopt(fd, sockopt::SndBuf).unwrap(),
            send_buf * 2
        );
    }
    if let Some(recv_buf) = args.recv_buf {
        nix::sys::socket::setsockopt(fd, sockopt::RcvBuf, &recv_buf).unwrap();
        assert_eq!(
            nix::sys::socket::getsockopt(fd, sockopt::RcvBuf).unwrap(),
            recv_buf * 2
        );
    }
    if let Some(cc) = &args.congestion_controller {
        nix::sys::socket::setsockopt(fd, sockopt::TcpCongestion, &OsString::from_str(cc).unwrap()).expect("failed to set congestion controller");
        assert_eq!(
            unsafe { CStr::from_ptr(nix::sys::socket::getsockopt(fd, sockopt::TcpCongestion).unwrap().as_bytes().as_ptr() as *const c_char).to_str().unwrap().to_string() },
            *cc
        )
    }
}

fn get_available_congestion_controllers() -> io::Result<Vec<String>> {
    let contents = fs::read_to_string("/proc/sys/net/ipv4/tcp_available_congestion_control")?;
    let algorithms = contents
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    Ok(algorithms)
}

#[derive(Debug)]
#[allow(dead_code)] // https://github.com/rust-lang/rust/issues/123068
struct TcpInfo {
    send_buf: usize,
    recv_buf: usize,
    congestion_controller: String,
}

impl TcpInfo {
    fn from(socket: &TcpStream) -> Self {
        let fd = socket.as_raw_fd();
        TcpInfo {
            send_buf: nix::sys::socket::getsockopt(fd, sockopt::SndBuf).unwrap(),
            recv_buf: nix::sys::socket::getsockopt(fd, sockopt::RcvBuf).unwrap(),
            congestion_controller: unsafe { CStr::from_ptr(nix::sys::socket::getsockopt(fd, sockopt::TcpCongestion).unwrap().as_bytes().as_ptr() as *const c_char).to_str().unwrap().to_string() },
        }
    }
}

pub async fn run_proxy(args: Args) {
    let listener = TcpListener::bind(args.bind)
        .unwrap_or_else(|_| panic!("Unable to bind to {}", args.bind));
    info!("listening on {}", listener.local_addr().unwrap());
    info!("target is {}", args.target);
    debug!("available congestion controllers: {}", get_available_congestion_controllers().unwrap().join(", "));
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
        configure_socket(&in_conn, &args);
        configure_socket(&out_conn, &args);
        debug!("in_socket: {:?}", TcpInfo::from(&in_conn));
        debug!("out_socket: {:?}", TcpInfo::from(&out_conn));
        monoio::spawn(async move {
            let (mut in_r, mut in_w) = in_conn.into_split();
            let (mut out_r, mut out_w) = out_conn.into_split();
            let _ = monoio::join!(
                        copy(&mut in_r, &mut out_w, args.copy_buf),
                        copy(&mut out_r, &mut in_w, args.copy_buf),
                    );
            info!("{relay_name}: close");
        });
    }
}

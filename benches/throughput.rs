use std::cmp::min;
use std::time::Duration;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use log::{error, info};
use monoio::{RuntimeBuilder, spawn, FusionDriver, Runtime, FusionRuntime, IoUringDriver, LegacyDriver, select, BufResult};
use monoio::buf::Slice;
use monoio::io::{AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt};
use monoio::net::{TcpListener, TcpStream};
use monoio::time::{sleep, TimeDriver};
use monoio_tcp_proxy::{run_proxy, Args};

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(10));
    targets = targets
);
criterion_main!(benches);

fn targets(c: &mut Criterion) {
    env_logger::builder().format_timestamp_nanos().init();
    let mut g = c.benchmark_group("proxy");

    let mut rt = RuntimeBuilder::<FusionDriver>::new()
        .enable_timer()
        .build()
        .unwrap();

    let num_bytes = 1E9 as usize;

    // Spawn server + proxy ONCE, keep addresses
    let (server_addr, proxy_addr) = setup_server_and_proxy(&mut rt, num_bytes);

    g.throughput(Throughput::Bytes(num_bytes as u64));
    g.bench_function("transmit-1G", |b| b.iter(|| transmit(num_bytes, proxy_addr, &mut rt)));
    g.finish()
}

fn transmit(num_bytes: usize, proxy_addr: std::net::SocketAddr, rt: &mut FusionRuntime<TimeDriver<IoUringDriver>, TimeDriver<LegacyDriver>>) {
    rt.block_on(async {
        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        let mut buf = vec![0u8; 1<<16];
        let mut bytes_received = 0;
        while bytes_received < num_bytes {
            let (res, b) = client.read(buf).await;
            buf = b;
            bytes_received += res.unwrap()
        }
        info!("done")
    });
}

fn setup_server_and_proxy(
    rt: &mut FusionRuntime<TimeDriver<IoUringDriver>, TimeDriver<LegacyDriver>>,
    num_bytes: usize,
) -> (std::net::SocketAddr, std::net::SocketAddr) {
    let proxy_addr: std::net::SocketAddr = "127.0.0.1:50005".parse().unwrap();

    let server_addr = rt.block_on(async {
        // Bind server to ephemeral port
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();

        // Spawn server loop
        spawn(async move {
            loop {
                let (mut socket, _) = server.accept().await.unwrap();
                spawn(async move {
                    let mut buf = vec![0u8; 1<<16];
                    let mut bytes_sent = 0;
                    while bytes_sent < num_bytes {
                        let to_send = min(buf.len(), num_bytes - bytes_sent);
                        let (res, slice) = socket.write(Slice::new(buf, 0, to_send)).await;
                        buf = slice.into_inner();
                        bytes_sent += match res {
                            Ok(n) => n,
                            Err(err) => {
                                error!("socket write failed: {err}");
                                return
                            }
                        }
                    }
                    socket.shutdown().await.unwrap();
                });
            }
        });

        // Spawn proxy once
        spawn(async move {
            let args = Args {
                bind: proxy_addr,
                target: server_addr,
                recv_buf: None,
                send_buf: None,
                congestion_controller: None,
            };
            run_proxy(args).await;
        });

        // Wait for proxy to start up
        sleep(Duration::from_millis(100)).await;

        server_addr
    });

    (server_addr, proxy_addr)
}
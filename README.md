# Monoio TCP Proxy

A simple TCP proxy with io_uring support.

```bash
$ monoio-tcp-proxy --help
Usage: monoio-tcp-proxy [OPTIONS] --bind <HOST:PORT> --target <HOST:PORT>

Options:
      --bind <HOST:PORT>    e.g. 0.0.0.0:50005
      --target <HOST:PORT>  e.g. 1.2.3.4:80
      --recv-buf <BYTES>    TCP receive buffer size
      --send-buf <BYTES>    TCP send buffer size
      --cc <NAME>           Which system TCP congestion controller to use
      --copy-buf <BYTES>    Copy buffer size [default: 131072]
  -h, --help                Print help
```

## Performance 

```bash
cargo bench
```

About 40 Gbps throughput.

possible improvements:
- GSO/GRO
- zero copy (monoio zero copy feature does not improve performance)

# module-proxy

A small HTTP reverse proxy in Rust, written to learn how a proxy can be split into small, independent modules that each inspect or change a request.

**Work in progress.** Right now it reads `config.toml`, picks an origin for each request from its `Host` header, forwards the method, path, query, headers and body, and returns the origin's response. Hop-by-hop headers are removed in both directions. A missing or unknown host gets `400`; an origin that can't be reached gets `502`.

Planned: a module pipeline with request and response phases, an IP block list (CIDR), a per-IP rate limiter, and config reload that keeps the last good config.

## Running

```sh
python -m http.server 3000        # a test origin
cargo run                         # the proxy, on 127.0.0.1:8080
curl -i -H "Host: example.com" http://127.0.0.1:8080/
```

`RUST_LOG=module_proxy=debug cargo run` shows more logging. `cargo test` runs the tests.

## Config

```toml
[[origin]]
host = "example.com"
url = "http://127.0.0.1:3000"
```

Origin URLs must be `http` with a host and a port. Hosts are matched case-insensitively and without the port; duplicate hosts are rejected at startup.

## Limits

Bodies are buffered in memory, not streamed.

Out of scope: TLS, HTTP/2 and 3, caching, more than one origin per host, IPv6.

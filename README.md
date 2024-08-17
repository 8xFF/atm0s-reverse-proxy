# Decentralized reverse proxy for HomeAssistant, IoT and more

This project aims to create an innovative decentralized reverse proxy server for HomeAssistant, IoT and more. The server allows contributions from anyone and provides a fast and optimized path between clients.

If you find it interesting or believe it could be helpful, we welcome your contributions to the codebase or consider starring the repository to show your support and motivate our team!

## Features

- **Decentralized:** The server is designed to be decentralized, allowing anyone to contribute and participate in the network. More details: [atm0s-sdn](https://github.com/8xff/atm0s-sdn)

- **Fast Relay:** The relay server ensures fast and efficient communication between clients, optimizing the path for data transmission using the atm0s-sdn overlay network.

- **Data Safety:** The TCP proxy used in this project is based on SNI (Server Name Indication), ensuring the safety and integrity of the transmitted data.

- **Written in Rust:** The project is implemented in Rust, a modern and efficient programming language known for its performance and memory safety.

- **No account required:** Each client will be identified by a public-private key, just connect and it works.

## Check list

- [x] Decentralized Routing, Relay between nodes
- [x] TLS(SNI) tunneling
- [x] HTTP tunneling
- [x] Stats Dashboard
- [x] Anonymous client
- [ ] Anonymous server contribution (WIP)

## Performance

Bellow is benchmarking results with Mac M1, and all nodes is running locally, it it very early version so it can be improve after finish all features:

- Direct http

```bash
wrk http://localhost:3000
Running 10s test @ http://localhost:3000
  2 threads and 10 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency   113.58us   32.97us   3.68ms   77.81%
    Req/Sec    41.60k     1.27k   43.00k    89.60%
  836084 requests in 10.10s, 143.52MB read
Requests/sec:  82780.91
Transfer/sec:     14.21MB
```

- Single node

```bash
wrk http://localhost:3000
Running 10s test @ http://localhost:3000
  2 threads and 10 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency   407.38us  489.34us  16.67ms   98.87%
    Req/Sec    13.18k     1.67k   17.61k    77.72%
  264801 requests in 10.10s, 45.46MB read
Requests/sec:  26218.89
Transfer/sec:      4.50MB
```

- Two nodes

```bash
wrk http://localhost:3000
Running 10s test @ http://localhost:3000
  2 threads and 10 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency   678.58us  681.76us  19.83ms   98.50%
    Req/Sec     8.06k     1.19k    9.60k    64.85%
  162101 requests in 10.10s, 27.83MB read
Requests/sec:  16049.83
Transfer/sec:      2.76MB
```

## Getting Started

To get started with the Decentralized HomeAssistant Proxy, follow these steps:

1. Clone the repository:

    ```shell
    git clone https://github.com/8xFF/atm0s-reverse-proxy.git
    ```

2. Build the project:

    ```shell
    cd atm0s-reverse-proxy
    cargo build --release
    ```

3. Run the server node1:

    ```shell
    ./target/release/relayer \
        --api-port 10001 \
        --http-port 11001 \
        --https-port 12001 \
        --connector-port 0.0.0.0:13001 \
        --root-domain local.ha.8xff.io \
        --sdn-node-id 1
    ```

3. Run the server node2:

    ```shell
    ./target/release/relayer \
        --api-port 10002 \
        --http-port 11002 \
        --https-port 12002 \
        --connector-port 0.0.0.0:13002 \
        --root-domain local.ha.8xff.io \
        --sdn-node-id 2 \
        --sdn-seeds '1@/ip4/192.168.1.39/udp/50001'
    ```

4. Run the client:

    ```shell
    ./target/release/agent \
        --connector-protocol tcp \
        --connector-addr 127.0.0.1:13001 \
        --http-dest 127.0.0.1:8080 \
        --https-dest 127.0.0.1:8443
    ```
    Client will print out assigned domain like: `a5fae3d220cb062c5aed9bd57d82f226.local.ha.8xff.io`, now we can access to local 8080 service with address:

    - Proxy over single node1:
    http://a5fae3d220cb062c5aed9bd57d82f226.local.ha.8xff.io:11001

    - Proxy with relay over node2 -> node1:
    http://a5fae3d220cb062c5aed9bd57d82f226.local.ha.8xff.io:11002

    Note that above url only can access in same machine.

## Contributing

Contributions are welcome! If you'd like to contribute to the project, please follow the guidelines outlined in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the [MIT License](LICENSE).

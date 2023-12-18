# Decentralized HomeAssistant Proxy

This project aims to create an innovative decentralized relay server for HomeAssistant. The server allows contributions from anyone and provides a fast and optimized path between clients.

## Features

- **Decentralized:** The server is designed to be decentralized, allowing anyone to contribute and participate in the network.

- **Fast Relay:** The relay server ensures fast and efficient communication between clients, optimizing the path for data transmission using the atm0s-sdn overlay network.

- **Data Safety:** The TCP proxy used in this project is based on SNI (Server Name Indication), ensuring the safety and integrity of the transmitted data.

- **Written in Rust:** The project is implemented in Rust, a modern and efficient programming language known for its performance and memory safety.

## Getting Started

To get started with the Decentralized HomeAssistant Proxy, follow these steps:

1. Clone the repository:

    ```shell
    git clone https://github.com/your-username/decentralized-homeassistant-proxy.git
    ```

2. Build the project:

    ```shell
    cd decentralized-homeassistant-proxy
    cargo build
    ```

3. Run the server:

    ```shell
    cargo run -- --root-domain local.ha.8xff.io
    ```

4. Run the client:

    ```shell
    cargo run --release -- --connector-addr 127.0.0.1:33333 --connector-protocol quic --http-dest 127.0.0.1:18080 --https-dest 127.0.0.1:18443
    ```

## Contributing

Contributions are welcome! If you'd like to contribute to the project, please follow the guidelines outlined in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the [MIT License](LICENSE).

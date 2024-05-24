RUST_LOG=debug cargo run -- \
    --connector-protocol quic \
    --connector-addr https://local.ha.8xff.io:13001 \
    --http-dest 127.0.0.1:8080 \
    --https-dest 127.0.0.1:8443 \
    --allow-quic-insecure

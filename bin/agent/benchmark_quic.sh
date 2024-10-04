cargo run --release --example benchmark_clients -- \
    --connector-protocol quic \
    --connector-addr https://127.0.0.1:13001 \
    --http-dest 127.0.0.1:8080 \
    --https-dest 127.0.0.1:8443 \
    --allow-quic-insecure \
    $@

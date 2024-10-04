RUST_LOG=info cargo run -- \
    --connector-protocol quic \
    --connector-addr https://210.211.96.145:43333 \
    --http-dest 127.0.0.1:8080 \
    --https-dest 127.0.0.1:8443 \
    --rtsp-dest 10.10.30.90:554 \
    --allow-quic-insecure

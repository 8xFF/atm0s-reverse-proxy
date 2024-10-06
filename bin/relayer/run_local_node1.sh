RUST_LOG=info cargo run -- \
    --proxy-http-listener 127.0.0.1:11001 \
    --proxy-tls-listener 127.0.0.1:12001 \
    --proxy-rtsp-listener 127.0.0.1:5341 \
    --proxy-rtsps-listener 127.0.0.1:35341 \
    --agent-listener 127.0.0.1:13001 \
    --root-domain local.ha.8xff.io \
    --sdn-listener 127.0.0.1:14001 \
    --sdn-advertise-address 127.0.0.1:14001 \
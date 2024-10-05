RUST_LOG=info cargo run -- \
    --proxy-http-listener 0.0.0.0:11001 \
    --proxy-tls-listener 0.0.0.0:12001 \
    --proxy-rtsp-listener 0.0.0.0:5341 \
    --proxy-rtsps-listener 0.0.0.0:35341 \
    --agent-listener 0.0.0.0:13001 \
    --root-domain local.ha.8xff.io \
    --sdn-listener 0.0.0.0:14001 \
    --sdn-advertise-address 127.0.0.1:14001 \
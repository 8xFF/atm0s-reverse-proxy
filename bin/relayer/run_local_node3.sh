RUST_LOG=info cargo run -- \
    --proxy-http-listener 0.0.0.0:11003 \
    --proxy-tls-listener 0.0.0.0:12003 \
    --proxy-rtsp-listener 0.0.0.0:5343 \
    --proxy-rtsps-listener 0.0.0.0:35343 \
    --agent-listener 0.0.0.0:13003 \
    --root-domain local.ha.8xff.io \
    --sdn-listener 127.0.0.1:14003 \
    --sdn-seeds 127.0.0.1:14001 \
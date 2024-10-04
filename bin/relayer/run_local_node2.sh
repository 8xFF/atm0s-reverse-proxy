RUST_LOG=info cargo run -- \
    --proxy-http-listener 0.0.0.0:11002 \
    --proxy-tls-listener 0.0.0.0:12002 \
    --proxy-rtsp-listener 0.0.0.0:5342 \
    --proxy-rtsps-listener 0.0.0.0:35342 \
    --agent-listener 0.0.0.0:13002 \
    --root-domain local.ha.8xff.io \
    --sdn-node-id 2 \
    --sdn-listener 127.0.0.1:14002 \
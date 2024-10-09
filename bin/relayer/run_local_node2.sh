RUST_LOG=info cargo run -- \
    --proxy-http-listener 127.0.0.1:11002 \
    --proxy-tls-listener 127.0.0.1:12002 \
    --proxy-rtsp-listener 127.0.0.1:5342 \
    --proxy-rtsps-listener 127.0.0.1:35342 \
    --agent-listener 127.0.0.1:13002 \
    --root-domain local.ha.8xff.io \
    --sdn-peer-id 2 \
    --sdn-listener 127.0.0.1:14002 \
    --sdn-seeds 1@127.0.0.1:14001 \
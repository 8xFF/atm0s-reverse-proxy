cargo run -- \
    --api-port 10001 \
    --http-port 11001 \
    --https-port 12001 \
    --connector-port 0.0.0.0:13001 \
    --root-domain local.ha.8xff.io \
    --sdn-node-id 1 \
    --sdn-port 50001

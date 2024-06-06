cargo run -- \
    --api-port 10002 \
    --http-port 11002 \
    --https-port 12002 \
    --connector-port 0.0.0.0:13002 \
    --root-domain local.ha.8xff.io \
    --sdn-node-id 2 \
    --sdn-port 50002 \
    --sdn-seeds '1@/ip4/127.0.0.1/udp/50001'

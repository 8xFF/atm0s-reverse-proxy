cargo run --release -- \
    --api-port 10002 \
    --http-port 11002 \
    --https-port 12002 \
    --connector-port 0.0.0.0:13002 \
    --root-domain local.ha.8xff.io \
    --sdn-node-id 2 \
    --sdn-seeds '1@/ip4/192.168.1.39/udp/50001'
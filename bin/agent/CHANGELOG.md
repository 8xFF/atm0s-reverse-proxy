# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.3.0...atm0s-reverse-proxy-agent-v0.4.0) - 2025-02-28

### Added

- ssl for tcp connection (#92)

### Fixed

- QUIC client should not verify hostname for adapting with multiple nodes (#94)

## [0.3.0](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.2.2...atm0s-reverse-proxy-agent-v0.3.0) - 2025-02-14

### Added

- turn tcp/quic into features in agent, optimize client binary size (#87)

## [0.2.2](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.2.1...atm0s-reverse-proxy-agent-v0.2.2) - 2024-11-11

### Other

- update Cargo.toml dependencies

## [0.2.1](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.2.0...atm0s-reverse-proxy-agent-v0.2.1) - 2024-10-22

### Fixed

- agent id generate crash ([#74](https://github.com/8xFF/atm0s-reverse-proxy/pull/74))

### Other

- small-sdn with quic ([#70](https://github.com/8xFF/atm0s-reverse-proxy/pull/70))

## [0.2.0](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.1.2...atm0s-reverse-proxy-agent-v0.2.0) - 2024-10-04

### Fixed

- tunnel stuck ([#63](https://github.com/8xFF/atm0s-reverse-proxy/pull/63))

### Other

- switched to tokio ([#66](https://github.com/8xFF/atm0s-reverse-proxy/pull/66))

## [0.1.2](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.1.1...atm0s-reverse-proxy-agent-v0.1.2) - 2024-09-23

### Fixed

- sometime handshake pkt is split to some small chunks ([#60](https://github.com/8xFF/atm0s-reverse-proxy/pull/60))

## [0.1.1](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-agent-v0.1.0...atm0s-reverse-proxy-agent-v0.1.1) - 2024-09-09

### Other

- updated the following local packages: atm0s-reverse-proxy-protocol

## [0.1.0](https://github.com/8xFF/atm0s-reverse-proxy/releases/tag/atm0s-reverse-proxy-agent-v0.1.0) - 2024-08-17

### Added
- rtsp proxy ([#37](https://github.com/8xFF/atm0s-reverse-proxy/pull/37))
- atm0s-sdn ([#2](https://github.com/8xFF/atm0s-reverse-proxy/pull/2))
- validate connection and auto generate domain from ed25519 privatekey
- first working version with http and sni proxy

### Fixed
- increase agent quic keep alive for reduce server load, added benchmark clients sample ([#30](https://github.com/8xFF/atm0s-reverse-proxy/pull/30))
- release action error ([#26](https://github.com/8xFF/atm0s-reverse-proxy/pull/26))
- agent quic timeout ([#19](https://github.com/8xFF/atm0s-reverse-proxy/pull/19))
- update quin for building in mipsel ([#16](https://github.com/8xFF/atm0s-reverse-proxy/pull/16))
- fixing warn and disable mips builds
- subdomain too long

### Other
- fix release-plz don't found default cert ([#42](https://github.com/8xFF/atm0s-reverse-proxy/pull/42))
- fix missing version for release-plz ([#41](https://github.com/8xFF/atm0s-reverse-proxy/pull/41))
- added release-plz and update deps ([#39](https://github.com/8xFF/atm0s-reverse-proxy/pull/39))
- agent log assigned domain to log ([#27](https://github.com/8xFF/atm0s-reverse-proxy/pull/27))
- update atm0s-sdn and switched to quinn forks for temporal fixing ring library ([#22](https://github.com/8xFF/atm0s-reverse-proxy/pull/22))
- fixing quinn deps in agent ([#18](https://github.com/8xFF/atm0s-reverse-proxy/pull/18))
- fixing quinn deps ([#17](https://github.com/8xFF/atm0s-reverse-proxy/pull/17))
- BREAKING CHANGE: Update newest atm0s-sdn with sans-io runtime ([#14](https://github.com/8xFF/atm0s-reverse-proxy/pull/14))
- split libs to allow customize ([#5](https://github.com/8xFF/atm0s-reverse-proxy/pull/5))
- simple quic implement
- fix grammar in message
- fix cli message
- switch to using spawn instead of spawn_local
- add more log to agent when proxy to target
- optimize agent binary size
- fmt

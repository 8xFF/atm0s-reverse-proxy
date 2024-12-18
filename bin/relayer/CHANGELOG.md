# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.4](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.4.3...atm0s-reverse-proxy-relayer-v0.4.4) - 2024-12-18

### Other

- update Cargo.toml dependencies

## [0.4.3](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.4.2...atm0s-reverse-proxy-relayer-v0.4.3) - 2024-11-26

### Other

- update Cargo.toml dependencies

## [0.4.2](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.4.1...atm0s-reverse-proxy-relayer-v0.4.2) - 2024-11-11

### Other

- update p2p lib for fixing alias issue ([#79](https://github.com/8xFF/atm0s-reverse-proxy/pull/79))

## [0.4.1](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.4.0...atm0s-reverse-proxy-relayer-v0.4.1) - 2024-11-07

### Fixed

- alias not release bug ([#77](https://github.com/8xFF/atm0s-reverse-proxy/pull/77))

## [0.4.0](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.3.0...atm0s-reverse-proxy-relayer-v0.4.0) - 2024-10-22

### Added

- add metrics for relayer ([#72](https://github.com/8xFF/atm0s-reverse-proxy/pull/72))
- update sdn for network connect authorizing ([#73](https://github.com/8xFF/atm0s-reverse-proxy/pull/73))

### Fixed

- agent id generate crash ([#74](https://github.com/8xFF/atm0s-reverse-proxy/pull/74))

### Other

- small-sdn with quic ([#70](https://github.com/8xFF/atm0s-reverse-proxy/pull/70))

## [0.3.0](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.2.0...atm0s-reverse-proxy-relayer-v0.3.0) - 2024-10-04

### Other

- update deps and switch to parking_lot ([#68](https://github.com/8xFF/atm0s-reverse-proxy/pull/68))

## [0.2.0](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.1.2...atm0s-reverse-proxy-relayer-v0.2.0) - 2024-10-04

### Fixed

- tunnel stuck ([#63](https://github.com/8xFF/atm0s-reverse-proxy/pull/63))
- fix remove worker after instantly agent is reconnected ([#62](https://github.com/8xFF/atm0s-reverse-proxy/pull/62))

### Other

- switched to tokio ([#66](https://github.com/8xFF/atm0s-reverse-proxy/pull/66))

## [0.1.2](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.1.1...atm0s-reverse-proxy-relayer-v0.1.2) - 2024-09-10

### Fixed

- add node_info struct to feedback status to sdn collector ([#52](https://github.com/8xFF/atm0s-reverse-proxy/pull/52))

## [0.1.1](https://github.com/8xFF/atm0s-reverse-proxy/compare/atm0s-reverse-proxy-relayer-v0.1.0...atm0s-reverse-proxy-relayer-v0.1.1) - 2024-09-09

### Fixed

- wrong handshake when tunnel request over cluster ([#48](https://github.com/8xFF/atm0s-reverse-proxy/pull/48))

### Other

- fix typo in CHANGELOG ([#45](https://github.com/8xFF/atm0s-reverse-proxy/pull/45))

## [0.1.0](https://github.com/8xFF/atm0s-reverse-proxy/releases/tag/atm0s-reverse-proxy-relayer-v0.1.0) - 2024-08-17

### Added
- rtsp proxy ([#37](https://github.com/8xFF/atm0s-reverse-proxy/pull/37))
- metrics outgoing cluster ([#34](https://github.com/8xFF/atm0s-reverse-proxy/pull/34))
- allow dynamic root domain, only check first sub-domain part ([#4](https://github.com/8xFF/atm0s-reverse-proxy/pull/4))
- atm0s-sdn ([#2](https://github.com/8xFF/atm0s-reverse-proxy/pull/2))
- add simple dashboard
- validate connection and auto generate domain from ed25519 privatekey
- first working version with http and sni proxy

### Fixed
- using spawn task outside run_agent_connection to wait when agent is disconnected ([#38](https://github.com/8xFF/atm0s-reverse-proxy/pull/38))
- histograms metrics to seconds ([#36](https://github.com/8xFF/atm0s-reverse-proxy/pull/36))
- fixed histogram metrics not working ([#35](https://github.com/8xFF/atm0s-reverse-proxy/pull/35))
- don't blocking proxy request from agent, refactor metrics ([#33](https://github.com/8xFF/atm0s-reverse-proxy/pull/33))
- deadlock in agents map => move agents map to separated struct AgentStorage for avoiding block ([#32](https://github.com/8xFF/atm0s-reverse-proxy/pull/32))
- quic_listener will stuck if have huge of waiting incoming conns and cause timeout ([#31](https://github.com/8xFF/atm0s-reverse-proxy/pull/31))
- increase agent quic keep alive for reduce server load, added benchmark clients sample ([#30](https://github.com/8xFF/atm0s-reverse-proxy/pull/30))
- release action error ([#26](https://github.com/8xFF/atm0s-reverse-proxy/pull/26))
- prometheus metric wrong format, use \_ instead of \. ([#24](https://github.com/8xFF/atm0s-reverse-proxy/pull/24))
- virtual socket memory leak, virtual socket port request safety ([#23](https://github.com/8xFF/atm0s-reverse-proxy/pull/23))
- agent quic timeout ([#19](https://github.com/8xFF/atm0s-reverse-proxy/pull/19))
- update quin for building in mipsel ([#16](https://github.com/8xFF/atm0s-reverse-proxy/pull/16))
- wrong check domain when proxy inside haproxy ([#3](https://github.com/8xFF/atm0s-reverse-proxy/pull/3))
- fixing warn and disable mips builds
- crash on parse invalid sni
- wrong ymux mode in relay agent connection

### Other
- fix release-plz don't found default cert ([#42](https://github.com/8xFF/atm0s-reverse-proxy/pull/42))
- fix missing version for release-plz ([#41](https://github.com/8xFF/atm0s-reverse-proxy/pull/41))
- added release-plz and update deps ([#39](https://github.com/8xFF/atm0s-reverse-proxy/pull/39))
- update metrics and metrics-dashboard version ([#29](https://github.com/8xFF/atm0s-reverse-proxy/pull/29))
- update atm0s-sdn and switched to quinn forks for temporal fixing ring library ([#22](https://github.com/8xFF/atm0s-reverse-proxy/pull/22))
- update atm0s-sdn with new authorization and encryption features ([#21](https://github.com/8xFF/atm0s-reverse-proxy/pull/21))
- update sdn for fixing some alias error ([#20](https://github.com/8xFF/atm0s-reverse-proxy/pull/20))
- fixing quinn deps ([#17](https://github.com/8xFF/atm0s-reverse-proxy/pull/17))
- expose atm0s-sdn and some functions to public ([#15](https://github.com/8xFF/atm0s-reverse-proxy/pull/15))
- BREAKING CHANGE: Update newest atm0s-sdn with sans-io runtime ([#14](https://github.com/8xFF/atm0s-reverse-proxy/pull/14))
- split libs to allow customize ([#5](https://github.com/8xFF/atm0s-reverse-proxy/pull/5))
- simple quic implement
- switch expose metrics to optional
- switch to using spawn instead of spawn_local
- fmt

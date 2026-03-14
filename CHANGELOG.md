# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.3](https://github.com/mahito1594/sf-pkgen-rs/compare/v0.4.2...v0.4.3) - 2026-03-14

### Other

- *(deps)* lock file maintenance ([#35](https://github.com/mahito1594/sf-pkgen-rs/pull/35))
- *(deps)* update all minor dependency updates ([#33](https://github.com/mahito1594/sf-pkgen-rs/pull/33))
- *(deps)* update rust crate quick-xml to v0.39.2 ([#31](https://github.com/mahito1594/sf-pkgen-rs/pull/31))
- remove version from NOTICE template to stabilize against dep updates ([#36](https://github.com/mahito1594/sf-pkgen-rs/pull/36))
- *(deps)* update actions/upload-artifact action to v7 ([#34](https://github.com/mahito1594/sf-pkgen-rs/pull/34))
- *(deps)* pin rust crate tempfile to v3.25.0 ([#29](https://github.com/mahito1594/sf-pkgen-rs/pull/29))
- *(deps)* update taiki-e/install-action action to v2.68.29 ([#32](https://github.com/mahito1594/sf-pkgen-rs/pull/32))
- *(deps)* update release-plz/action action to v0.5.128 ([#30](https://github.com/mahito1594/sf-pkgen-rs/pull/30))
- change renovate schedule to every weekend ([#27](https://github.com/mahito1594/sf-pkgen-rs/pull/27))

## [0.4.2](https://github.com/mahito1594/sf-pkgen-rs/compare/v0.4.1...v0.4.2) - 2026-03-07

### Fixed

- prevent intermittent test abort caused by PanicHookGuard race condition ([#16](https://github.com/mahito1594/sf-pkgen-rs/pull/16))

### Other

- update TUI keybindings to reflect right pane fuzzy search ([#23](https://github.com/mahito1594/sf-pkgen-rs/pull/23))
- fix wrong settings ([#22](https://github.com/mahito1594/sf-pkgen-rs/pull/22))
- add renovate.json ([#21](https://github.com/mahito1594/sf-pkgen-rs/pull/21))
- add coverage job with cargo-llvm-cov ([#18](https://github.com/mahito1594/sf-pkgen-rs/pull/18))

## [0.4.1](https://github.com/mahito1594/sf-pkgen-rs/compare/v0.4.0...v0.4.1) - 2026-03-07

### Added

- preserve search query when re-entering search mode ([#14](https://github.com/mahito1594/sf-pkgen-rs/pull/14))

### Other

- add release-plz for automated version bump and tag creation ([#12](https://github.com/mahito1594/sf-pkgen-rs/pull/12))

## [0.4.0](https://github.com/mahito1594/sf-pkgen-rs/compare/v0.3.0...v0.4.0) - 2026-03-07

### Added

- show active filter keyword in pane title after exiting search mode ([#11](https://github.com/mahito1594/sf-pkgen-rs/pull/11))

## [0.3.0](https://github.com/mahito1594/sf-pkgen-rs/compare/v0.2.0...v0.3.0) - 2026-03-05

### Added

- add fuzzy search to right pane (component list) ([#7](https://github.com/mahito1594/sf-pkgen-rs/pull/7))

## [0.2.0](https://github.com/mahito1594/sf-pkgen-rs/compare/v0.1.0...v0.2.0) - 2026-03-01

### Fixed

- sort component list in right pane alphabetically ([#5](https://github.com/mahito1594/sf-pkgen-rs/pull/5))

## [0.1.0](https://github.com/mahito1594/sf-pkgen-rs/releases/tag/v0.1.0) - 2026-02-23

### Other

- add CI/CD (auto test and publishing) ([#1](https://github.com/mahito1594/sf-pkgen-rs/pull/1))

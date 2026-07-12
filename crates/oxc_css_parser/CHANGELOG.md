# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.7](https://github.com/oxc-project/oxc-css-parser/compare/oxc-css-parser-v0.0.6...oxc-css-parser-v0.0.7) - 2026-07-12

### Added

- *(parser)* accept functional pseudo-pages (@page::slot()) ([#121](https://github.com/oxc-project/oxc-css-parser/pull/121))
- *(parser)* allow multiple idents in ::part() ([#119](https://github.com/oxc-project/oxc-css-parser/pull/119))
- *(parser)* support scroll-driven keyframe timeline-range selectors ([#118](https://github.com/oxc-project/oxc-css-parser/pull/118))
- *(parser)* accept empty @scope () roots and limits ([#117](https://github.com/oxc-project/oxc-css-parser/pull/117))
- *(parser)* support name-only and general-enclosed @container queries ([#116](https://github.com/oxc-project/oxc-css-parser/pull/116))

### Fixed

- *(parser)* don't report an error for @color-profile device-cmyk ([#120](https://github.com/oxc-project/oxc-css-parser/pull/120))

### Other

- replace guarded single-arm matches with let chains ([#113](https://github.com/oxc-project/oxc-css-parser/pull/113))
- remove redundant clone on Copy type Span ([#112](https://github.com/oxc-project/oxc-css-parser/pull/112))
- set MSRV to 1.95.0 ([#111](https://github.com/oxc-project/oxc-css-parser/pull/111))

## [0.0.6](https://github.com/oxc-project/oxc-css-parser/compare/oxc-css-parser-v0.0.5...oxc-css-parser-v0.0.6) - 2026-07-08

### Added

- *(parser)* report spec parse errors via recoverable_errors ([#94](https://github.com/oxc-project/oxc-css-parser/pull/94))

### Other

- mention raffia fork ([#106](https://github.com/oxc-project/oxc-css-parser/pull/106))
- remove redundant clones on Copy types ([#100](https://github.com/oxc-project/oxc-css-parser/pull/100))
- *(parser)* compact CSS token data ([#98](https://github.com/oxc-project/oxc-css-parser/pull/98))
- import ast_generated as a module instead of include! ([#97](https://github.com/oxc-project/oxc-css-parser/pull/97))
- place span first in ast structs ([#96](https://github.com/oxc-project/oxc-css-parser/pull/96))
- tokenize over bytes instead of CharIndices ([#95](https://github.com/oxc-project/oxc-css-parser/pull/95))
- remove changelog
- clean up codebase and fix svmin/svmax classification ([#89](https://github.com/oxc-project/oxc-css-parser/pull/89))
- normalize README sponsor section

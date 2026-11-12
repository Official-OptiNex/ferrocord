# Roadmap

ferrocord is being built ahead of the native client's public release. Items move to ✅ when they are
merged and tested.

## Phase 0: Foundations ✅

- ✅ IDA-style signature scanner with wildcards
- ✅ Signature files with offsets, uniqueness and ambiguity checks
- ✅ Hook manager (install / enable / disable / remove)
- ✅ Sandboxed WASM plugin host with fuel limits
- ✅ Plugin ABI v1 and manifest format
- ✅ Linux loader (`LD_PRELOAD`) and `ferrocord` CLI
- ✅ Example plugin in Rust

## Phase 1: Platforms

- ⬜ Windows: find `.text` via PE headers, load the loader with a proxy DLL
- ⬜ macOS: find `__TEXT,__text` via dyld, handle code signing
- ⬜ Release binaries for every platform from CI

## Phase 2: First contact with the native client

- ⬜ Map the client's main modules and its GPUI views
- ⬜ First signature set (`ready`, `message_create`, `message_delete`)
- ⬜ Turn hooks into plugin events
- ⬜ Signature update workflow for new client builds

## Phase 3: Plugin SDK

- ⬜ `ferrocord-sdk` crate so plugins avoid hand-written `extern` functions
- ⬜ Plugin settings stored by the host
- ⬜ Permission system (plugins request capabilities in their manifest)
- ⬜ Host calls back into the client (send message, open view, …)

## Phase 4: Look and feel

- ⬜ Theme support through GPUI's theme system
- ⬜ In-client plugin and theme manager
- ⬜ Plugin and theme repository

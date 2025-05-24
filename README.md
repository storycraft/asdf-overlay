[![Npm][npm-badge]][npm-url]
[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]

[npm-badge]: https://img.shields.io/npm/v/asdf-overlay-node.svg
[npm-url]: https://www.npmjs.com/package/asdf-overlay-node
[crates-badge]: https://img.shields.io/crates/v/asdf-overlay-client.svg
[crates-url]: https://crates.io/crates/asdf-overlay-client
[docs-badge]: https://docs.rs/asdf-overlay-client/badge.svg     
[docs-url]: https://docs.rs/asdf-overlay-client

# Asdf Overlay
Blazingly fast™ & Easy to use Windows Overlay library

Asdf Overlay provides an easy to use interface to draw on top of window framebuffer by hooking rendering backends

GPU shared texture was used to avoid framebuffer copy via CPU.
As a result, Asdf Overlay is capable of rendering overlay with any size without performance loss.

![Screenshot](assets/example.png)

## Supported API
* [x] OpenGL
* [x] DX9
* [ ] DX10
* [x] DX11
* [x] DX12
* [ ] Vulkan

## Features
1. Supports multiple backends
2. Blazingly fast™
3. GPU accelerated shared overlay surface
4. Input capture control

## Used by
[alspotron-url]: https://github.com/organization/alspotron
[tosu-url]: https://github.com/tosuapp/tosu

| | | |
| :-----: | ----- | ----- |
| [<img src="https://github.com/organization/alspotron/assets/16558115/447a957e-faf2-4759-8884-5d7b04fb1fbb" height="48">][alspotron-url] | [Alspotron][alspotron-url] | Ingame lyrics overlay
| [<img src="https://avatars.githubusercontent.com/u/184138403?s=48" height="48">][tosu-url] | [Tosu][tosu-url] | Ingame overlay

## Pre-requirement
1. node, pnpm package manager
2. nightly rustc, cargo, msvc(x64, x86, arm64)
3. Install x86_64-pc-windows-msvc, i686-pc-windows-msvc, aarch64-pc-windows-msvc rustc targets

### Installing node dependencies
```bash
pnpm install
```

### Build
```bash
pnpm build
```

> [!WARNING]
> DLL and the client must be built using same rust compiler or it will misbehaviour

## Example
Examples are located in `examples` directory.

### Node
Run
```bash
pnpm build && pnpm --filter ingame-browser start <process_name>
```
Pressing `Left Shift + A` will show ingame browser overlay and input will be redirected to browser window. Pressing again will close it.

https://github.com/user-attachments/assets/d7f0db58-cb11-437f-9990-50d095c7c575

### Rust
1. Run
```bash
cargo build && cargo run -p noise-rectangle <process_name>
```
Glitching squares appear and disappear on target process

https://github.com/user-attachments/assets/069d1cc1-f95d-4a44-899c-7f538c0f5a69

2. Run
```bash
cargo build && cargo run -p input-capture <process_name>
```
It will listen and block inputs from target process until process exit

## Debugging
Run with debug build.
Use external debug log viewer (ex: `DebugView`) to see tracing log of injected process

## License
This project is dual licensed under MIT or Apache-2.0 License

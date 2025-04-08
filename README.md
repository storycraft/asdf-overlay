# Asdf Overlay
Electron overlay solution for Windows

![Screenshot](assets/example.png)

## Supported API
* [x] OpenGL
* [ ] DX9
* [ ] DX10
* [x] DX11
* [ ] DX12
* [ ] Vulkan

## Pre-requirement
1. node, pnpm package manager
2. rust, cargo, msvc(x64, x86, arm64)
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
> DLL and the client must be built using same rust compiler or it may misbehaviour

## Example
Run
```bash
cargo build
cargo run --example example <process_name>
```
Glitching squares appear and disappear on target process

## License
This project is dual licensed under MIT or Apache-2.0 License

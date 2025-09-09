[![Npm][npm-badge]][npm-url]
[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]

[npm-badge]: https://img.shields.io/npm/v/@asdf-overlay/core.svg
[npm-url]: https://www.npmjs.com/package/@asdf-overlay/core
[crates-badge]: https://img.shields.io/crates/v/asdf-overlay.svg
[crates-url]: https://crates.io/crates/asdf-overlay
[docs-badge]: https://docs.rs/asdf-overlay/badge.svg     
[docs-url]: https://docs.rs/asdf-overlay

# Asdf Overlay
Blazingly fast™ Windows Overlay library

[Documentation](https://storycraft.github.io/asdf-overlay/)

## Used by
[lyrs-url]: https://github.com/organization/lyrs
[tosu-url]: https://github.com/tosuapp/tosu

| | | |
| :-----: | ----- | ----- |
| [![Lyrs logo](.github/images/lyrs-logo.png)][lyrs-url] | [Lyrs][lyrs-url] | Ingame lyrics overlay
| [![Tosu logo](.github/images/tosu-logo.png)][tosu-url] | [Tosu][tosu-url] | Ingame overlay

## Sponsorship
[sign-path-io-url]: https://signpath.io/
[sign-path-foundation-url]: https://signpath.org/

| | |
| :-----: | ----- |
| [![SignPath logo](.github/images/signpath-logo.png)][sign-path-io-url] | Free code signing provided by [SignPath.io][sign-path-io-url], certificate by [SignPath Foundation][sign-path-foundation-url] |

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
pnpm build && cargo run -p noise-rectangle <pid>
```
Glitching squares appear and disappear on target process

https://github.com/user-attachments/assets/069d1cc1-f95d-4a44-899c-7f538c0f5a69

2. Run
```bash
pnpm build && cargo run -p input-capture <pid>
```
It will listen and block inputs from target process until process exit

## License
This project is dual licensed under MIT or Apache-2.0 License

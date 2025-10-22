## Attaching to target process
To control overlay, you first need to attach overlay dll to target process.
The `@asdf-overlay/core` package provides a function for attaching overlay and initialize IPC connection.

Following code connects overlay to target process.
```typescript
import { defaultDllDir, Overlay } from '@asdf-overlay/core';

const overlay = await Overlay.attach(
  defaultDllDir().replace('app.asar', 'app.asar.unpacked'),
  /* target process id */ 12345,
  /* optional timeout in ms */ 5000,
);
```
Some caveats included below:
1. `@asdf-overlay/core` package includes overlay dll files for x64, ia32 and arm64 architectures.
   Asdf overlay will choose appropriate one based on the target process architecture.
   The `defaultDllDir` function returns path to the directory containing these dll files.
2. Onced injected, the dll will not be unloaded until the target process exits and maybe reused later if another connection is established.
3. If optional timeout is not provided, it will wait indefinitely.
4. The `Overlay.attach` function returns an `Overlay` instance upon successful attachment and can be used to control the overlay.
5. On Electron, `@asdf-overlay/core` must be specified as external to work correctly.
6. On Electron, `defaultDllDir` function may return path inside `app.asar` archive.
   In such cases, you need to replace `app.asar` with `app.asar.unpacked` to access the dll files.

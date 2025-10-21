## Layout and Surface connection
This section explains how to layout overlay surface and connect overlay surface to target window in Electron.

## Layout
After obtaining window id, you can set overlay layout using various options.

TBA

## Surface connection
After obtaining main window information, you can connect overlay surface to show overlay.
By using `@asdf-overlay/electron` package, you can easily connect Electron window as overlay surface.

```typescript
import { Overlay } from '@asdf-overlay/core';
import { ElectronOverlaySurface } from '@asdf-overlay/electron';

const overlay: Overlay = /* Attached Overlay instance */;
const id: number = /* Id of target window */;
const gpuLuid: GpuLuid = /* GPU LUID of target window */;

const window = new BrowserWindow({
  webPreferences: {
    offscreen: {
      useSharedTexture: true,
    },
  },
});

const surface = ElectronOverlaySurface.connect({ id, overlay }, gpuLuid, mainWindow.webContents);

// Disconnecting surface
await surface.disconnect();
```
Caveats include:
1. Make sure to enable `offscreen.useSharedTexture` in `webPreferences` when creating `BrowserWindow`.
   Without this option, overlay will be presented using CPU which causes high latency and low performance.
2. You need to provide correct `GpuLuid` when connecting surface.
   On hybrid GPU systems like laptops with integrated and discrete GPUs, providing wrong `GpuLuid` will result in no overlay shown.
3. While overlay position can be changed, size is fixed to connected surface texture.
   To change size of surface, change the size of browser.
   `ElectronOverlaySurface` instance takes care of resizing overlay surface automatically.
4. To disconnect surface, call `disconnect` method of `ElectronOverlaySurface` instance.
   This will disconnect and release surface resources.

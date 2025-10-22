## Layout and Surface connection
This section explains how to layout overlay surface and connect overlay surface to target window in Electron.

## Layout
After obtaining window id, you can set overlay layout using various options.

There are two types of specifying length: absolute length in pixels and relative length in percentage.

Relative length is specified as a number between `0.0` and `1.0`, representing percentage of target window size.

Layout can be done using `setPosition`, `setAnchor` and `setMargin` methods of `Overlay` instance.
```typescript
import { Overlay, percent, length } from '@asdf-overlay/core';

const overlay: Overlay = /* Attached Overlay instance */;
const id: number = /* Id of target window */;

// Set position to center of target window
void overlay.setPosition(id, percent(0.5), percent(0.5));
// Set anchor to center of overlay surface
void overlay.setAnchor(id, percent(0.5), percent(0.5));
// Set margin to 10 pixels from each side
void overlay.setMargin(id, length(10), length(10), length(10), length(10));
```
* **Position**: Specifies the position of overlay surface relative to target window's client area.
  The position is determined by the anchor point of overlay surface.
* **Anchor**: Specifies the anchor point of overlay surface.
   The anchor point is a point on the overlay surface that will be aligned to the position in the target window.
* **Margin**: Specifies the margin between overlay surface and target window's client area.
  Margin is specified for each side: top, right, bottom and left.

## Surface connection
After obtaining main window information, you can connect overlay surface to show overlay.
By using `@asdf-overlay/electron` package, you can easily connect Electron window as overlay surface.

Code example is shown below.
```typescript
import { Overlay } from '@asdf-overlay/core';
import { ElectronOverlaySurface, type OverlayWindow } from '@asdf-overlay/electron';

const overlay: Overlay = /* Attached Overlay instance */;
const windowId: number = /* Id of target window */;
const gpuLuid: GpuLuid = /* GPU LUID of target window */;
const window: OverlayWindow = { id: windowId, overlay };

const window = new BrowserWindow({
  webPreferences: {
    offscreen: {
      useSharedTexture: true,
    },
  },
});

const surface = ElectronOverlaySurface.connect(window, gpuLuid, mainWindow.webContents);

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

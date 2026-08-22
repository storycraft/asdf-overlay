import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay, type KeyInputState, type SurfaceInfo } from '@asdf-overlay/core';
import find from 'find-process';
import { type OverlaySurface, type OverlayWindow } from '@asdf-overlay/electron';
import { ElectronOverlaySurface } from '@asdf-overlay/electron/surface';
import { ElectronOverlayInput } from '@asdf-overlay/electron/input';

async function createOverlayWindow(pid: number) {
  const overlay = await Overlay.attach(
    defaultDllDir().replace('app.asar', 'app.asar.unpacked'),
    pid,
  );

  overlay.event.on('tracing_event', (metadata, message) => {
    console.info({ metadata, message });
  });

  // Create the browser window.
  const mainWindow = new BrowserWindow({
    webPreferences: {
      offscreen: {
        useSharedTexture: true,
      },
    },
  });

  overlay.event.on(
    'window_added',
    (id) => {
      // always listen keyboard events
      overlay.listenInput(id, false, true);
    }
  );

  // NOTE: Some apps decide to recreate whole surface when resizing.
  // In actual use, you have to listen for `surface_destroyed` and listen for next `surface_added` events to handle this case.
  let [windowId, [surfaceId, surfaceInfo]] = await Promise.all([
    new Promise<number>(resolve => overlay.event.once(
      'window_added',
      (id, _width, _height) => {
        console.debug('window found id:', id);
        resolve(id);
      }),
    ),
    new Promise<[bigint, SurfaceInfo]>(resolve => overlay.event.once(
      'surface_added',
      (id, _width, _height, info) => {
        console.debug('surface found id:', id, 'info:', info);
        resolve([id, info]);
      }),
    )
  ]);

  // If bound window is found, use it instead of the first window found.
  if (surfaceInfo.ty.windowId) {
    console.debug('surface window found id:', surfaceInfo.ty.windowId);
    windowId = surfaceInfo.ty.windowId;
  }

  const window: OverlayWindow = { id: windowId, overlay };
  const surface: OverlaySurface = { id: surfaceId, overlay, info: surfaceInfo };

  let electronSurface: ElectronOverlaySurface | null = null;

  let overlayInput: ElectronOverlayInput | null = null;
  let block = false;
  let shiftState: KeyInputState = 'Released';
  let aState: KeyInputState = 'Released';
  overlay.event.on('window_keyboard_input', (_, input) => {
    keybind: if (input.type === 'Key') {
      const key = input.key;
      if (key.code === 0x10 && !key.extended) {
        shiftState = input.state;
      } else if (key.code === 0x41) {
        aState = input.state;
      } else {
        break keybind;
      }

      // when Left Shift + A is pressed. show window and start blocking.
      if (shiftState === aState && shiftState === 'Pressed') {
        block = !block;

        if (block) {
          overlayInput = ElectronOverlayInput.connect(window, mainWindow.webContents);
          electronSurface = ElectronOverlaySurface.connect(surface, mainWindow.webContents);

          // do full repaint
          mainWindow.webContents.startPainting();
          mainWindow.webContents.invalidate();
          mainWindow.focusOnWebView();

          // Open the DevTools.
          mainWindow.webContents.openDevTools();
        }

        // block all inputs reaching window and listen
        void overlay.blockInput(block);
        return;
      }
    }
  });

  // always listen for `input_blocking_ended` because user can cancel blocking
  overlay.event.on('input_blocking_ended', () => {
    block = false;
    mainWindow.webContents.stopPainting();
    mainWindow.blurWebView();
    void electronSurface?.disconnect().then(() => {
      electronSurface = null;
    });
    void overlayInput?.disconnect().then(() => {
      overlayInput = null;
    });
  });

  mainWindow.webContents.stopPainting();
  await mainWindow.loadURL('https://electronjs.org');
}

async function main() {
  await app.whenReady();

  const name = process.argv[2];
  if (!name) {
    throw new Error('Please provide process name to attach overlay');
  }

  const list = await find('name', name, true);
  if (list.length === 0) {
    throw new Error(`Couldn't find a process named ${name}`);
  }

  await createOverlayWindow(list[0].pid);
}

main().catch((e: unknown) => {
  app.quit();
  throw e;
});

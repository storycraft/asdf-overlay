import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay, type GpuLuid, type KeyInputState } from '@asdf-overlay/core';
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
    console.info(metadata, message);
  });

  // Create the browser window.
  const mainWindow = new BrowserWindow({
    webPreferences: {
      offscreen: {
        useSharedTexture: true,
      },
    },
  });

  const [windowId, [surfaceId, luid]] = await Promise.all([
    new Promise<number>(resolve => overlay.event.once(
      'window_added',
      (id, _width, _height) => {
        resolve(id);
      }),
    ),
    new Promise<[bigint, GpuLuid]>(resolve => overlay.event.once(
      'surface_added',
      (id, _width, _height, luid) => {
        resolve([id, luid]);
      }),
    )
  ]);

  const window: OverlayWindow = { id: windowId, overlay };
  const surface: OverlaySurface = { id: surfaceId, overlay };

  let electronSurface: ElectronOverlaySurface | null = null;

  // always listen keyboard events
  await overlay.listenInput(windowId, false, true);

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
          electronSurface = ElectronOverlaySurface.connect(surface, luid, mainWindow.webContents);

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

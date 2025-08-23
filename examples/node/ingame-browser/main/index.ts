import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay, percent } from '@asdf-overlay/core';
import { InputState } from '@asdf-overlay/core/input';
import find from 'find-process';
import { type OverlayWindow } from '@asdf-overlay/electron';
import { ElectronOverlaySurface } from '@asdf-overlay/electron/surface';
import { ElectronOverlayInput } from '@asdf-overlay/electron/input';

async function createOverlayWindow(pid: number) {
  const overlay = await Overlay.attach(
    defaultDllDir().replace('app.asar', 'app.asar.unpacked'),
    pid,
  );

  // Create the browser window.
  const mainWindow = new BrowserWindow({
    webPreferences: {
      offscreen: {
        useSharedTexture: true,
      },
    },
  });

  const id = await new Promise<number>(resolve => overlay.event.once('added', resolve));
  const window: OverlayWindow = { id, overlay };

  // centre layout
  void overlay.setPosition(id, percent(0.5), percent(0.5));
  void overlay.setAnchor(id, percent(0.5), percent(0.5));

  ElectronOverlaySurface.connect(window, mainWindow.webContents);

  // always listen keyboard events
  await overlay.listenInput(id, false, true);

  let overlayInput: ElectronOverlayInput | null = null;
  let block = false;
  let shiftState: InputState = 'Released';
  let aState: InputState = 'Released';
  overlay.event.on('keyboard_input', (_, input) => {
    keybind: if (input.kind === 'Key') {
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
          // do full repaint
          mainWindow.webContents.startPainting();
          mainWindow.webContents.invalidate();
          mainWindow.focusOnWebView();

          // Open the DevTools.
          mainWindow.webContents.openDevTools();
          overlayInput = ElectronOverlayInput.connect(window, mainWindow.webContents);
        }

        // block all inputs reaching window and listen
        void overlay.blockInput(id, block);
        return;
      }
    }
  });

  // always listen for `input_blocking_ended` because user can cancel blocking
  overlay.event.on('input_blocking_ended', () => {
    block = false;
    mainWindow.webContents.stopPainting();
    mainWindow.blurWebView();
    void overlay.clearSurface(id);
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

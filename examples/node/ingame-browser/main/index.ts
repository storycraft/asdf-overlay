import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay } from 'asdf-overlay-node';
import { InputState } from 'asdf-overlay-node/input';
import find from 'find-process';
import { toCursor, toKeyboardInputEvent, toMouseEvent } from './input';

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

  mainWindow.webContents.on('paint', async (e) => {
    if (!e.texture) {
      return;
    }

    const info = e.texture.textureInfo;
    // captureUpdateRect contains more accurate dirty rect, fallback to contentRect if it doesn't exist.
    const rect = info.metadata.captureUpdateRect ?? info.contentRect;

    try {
      // update only changed part
      await overlay.updateShtex(
        info.codedSize.width,
        info.codedSize.height,
        e.texture.textureInfo.sharedTextureHandle,
        {
          dstX: rect.x,
          dstY: rect.y,
          src: rect
        },
      );
    } finally {
      e.texture.release();
    }
  });

  const hwnd = await new Promise<number>((resolve) => overlay.event.once('added', resolve));

  // always listen keyboard events
  await overlay.listenInput(hwnd, false, true);

  overlay.event.on('cursor_input', (_, input) => {
    const event = toMouseEvent(input);
    if (event) {
      mainWindow.webContents.sendInputEvent(event);
    }
  });

  mainWindow.webContents.on('cursor-changed', (_, type) => {
    overlay.setBlockingCursor(hwnd, toCursor(type));
  });

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
          mainWindow.webContents.invalidate();
          mainWindow.webContents.startPainting();
          mainWindow.focusOnWebView();

          // Open the DevTools.
          mainWindow.webContents.openDevTools();
        }

        // block all inputs reaching window and listen
        overlay.blockInput(hwnd, block);
        return;
      }
    }

    if (!block) {
      return;
    }

    const event = toKeyboardInputEvent(input);
    if (event) {
      mainWindow.webContents.sendInputEvent(event);
    }
  });

  // always listen for `input_blocking_ended` because user can cancel blocking
  overlay.event.on('input_blocking_ended', () => {
    block = false;
    mainWindow.webContents.stopPainting();
    mainWindow.blurWebView();
    overlay.clearSurface();
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

main().catch((e) => {
  app.quit();
  throw e;
});

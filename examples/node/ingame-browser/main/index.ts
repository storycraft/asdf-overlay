import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay } from 'asdf-overlay-node';
import { key } from 'asdf-overlay-node/util';
import find from 'find-process';
import { toCursor, toKeyboardInputEvent, toMouseEvent } from './input';

async function createOverlayWindow(pid: number) {
  const overlay = await Overlay.attach(
    'electron-overlay',
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

  overlay.event.on('cursor_input', (_, input) => {
    const event = toMouseEvent(input);
    if (event) {
      mainWindow.webContents.sendInputEvent(event);
    }
  });

  mainWindow.webContents.on('cursor-changed', (_, type) => {
    overlay.setCaptureCursor(hwnd, toCursor(type));
  });

  overlay.event.on('keyboard_input', (_, input) => {
    const event = toKeyboardInputEvent(input);
    if (event) {
      mainWindow.webContents.sendInputEvent(event);
    }
  });

  overlay.event.on('input_capture_start', () => {
    // do full repaint
    mainWindow.webContents.invalidate();
    mainWindow.webContents.startPainting();
    mainWindow.focusOnWebView();

    // Open the DevTools.
    mainWindow.webContents.openDevTools();
  });

  overlay.event.on('input_capture_end', () => {
    mainWindow.webContents.stopPainting();
    mainWindow.blurWebView();
    overlay.clearSurface();
  });

  await overlay.setInputCaptureKeybind(hwnd, [key(0x10), key(0x41)]);
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

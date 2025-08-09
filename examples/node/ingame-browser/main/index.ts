import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay, percent } from 'asdf-overlay-node';
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

  mainWindow.webContents.on('paint', (e) => {
    void (async (e) => {
      if (!e.texture) {
        return;
      }

      const info = e.texture.textureInfo;
      // captureUpdateRect contains more accurate dirty rect, fallback to contentRect if it doesn't exist.
      const rect = info.metadata.captureUpdateRect ?? info.contentRect;

      try {
        // update only changed part
        await overlay.updateShtex(
          id,
          info.codedSize.width,
          info.codedSize.height,
          e.texture.textureInfo.sharedTextureHandle,
          {
            dstX: rect.x,
            dstY: rect.y,
            src: rect,
          },
        );
      } finally {
        e.texture.release();
      }
    })(e);
  });

  const id = await new Promise<number>(resolve => overlay.event.once('added', resolve));

  // centre layout
  void overlay.setPosition(id, percent(0.5), percent(0.5));
  void overlay.setAnchor(id, percent(0.5), percent(0.5));

  // always listen keyboard events
  await overlay.listenInput(id, false, true);

  overlay.event.on('cursor_input', (_, input) => {
    const event = toMouseEvent(input);
    if (event) {
      mainWindow.webContents.sendInputEvent(event);
    }
  });

  mainWindow.webContents.on('cursor-changed', (_, type) => {
    void overlay.setBlockingCursor(id, toCursor(type));
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
        void overlay.blockInput(id, block);
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
    void overlay.clearSurface(id);
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

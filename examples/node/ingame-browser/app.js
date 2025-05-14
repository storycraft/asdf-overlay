// Modules to control application life and create native browser window
import { app, BrowserWindow } from 'electron';
import { defaultDllDir, Overlay } from 'asdf-overlay-node';
import { key } from 'asdf-overlay-node/util';
import find from 'find-process';
import keyboardKey from 'keyboard-key';

async function createOverlayWindow(pid) {
  const overlay = await Overlay.attach(
    'electron-overlay',
    defaultDllDir().replace('app.asar', 'app.asar.unpacked'),
    pid,
  );

  // Create the browser window.
  const mainWindow = new BrowserWindow({
    width: 800,
    height: 600,
    webPreferences: {
      offscreen: {
        useSharedTexture: true,
      },
    },
    show: false,
  });
  mainWindow.webContents.on('paint', (e) => {
    (async () => {
      if (!e.texture) {
        return;
      }

      try {
        await overlay.updateShtex(e.texture.textureInfo.sharedTextureHandle);
      } finally {
        e.texture.release();
      }
    })();
  });
  const hwnd = await new Promise((resolve) => overlay.event.once('added', resolve));

  await overlay.setInputCaptureKeybind(hwnd, [key(0x10), key(0x41)]);

  overlay.event.on('cursor_input', (hwnd, input) => {
    if (input.kind === 'Enter') {
      mainWindow.webContents.sendInputEvent({
        type: 'mouseEnter',
        x: input.x,
        y: input.y,
      });
    } else if (input.kind === 'Leave') {
      mainWindow.webContents.sendInputEvent({
        type: 'mouseLeave',
        x: input.x,
        y: input.y,
      });
    } else if (input.kind === 'Move') {
      mainWindow.webContents.sendInputEvent({
        type: 'mouseMove',
        x: input.x,
        y: input.y,
      });
    } else if (input.kind === 'Scroll') {
      mainWindow.webContents.sendInputEvent({
        type: 'mouseWheel',
        deltaX: input.axis === 'X' ? input.delta : 0,
        deltaY: input.axis === 'Y' ? input.delta : 0,
        x: input.x,
        y: input.y,
      });
    } else if (input.kind === 'Action') {
      mainWindow.webContents.sendInputEvent({
        type: input.state === 'Pressed' ? 'mouseDown' : 'mouseUp',
        button: input.action === 'Left' ? 'left' : input.action === 'Middle' ? 'middle' : 'right',
        clickCount: 1,
        x: input.x,
        y: input.y,
      });
    }
  });

  overlay.event.on('keyboard_input', (_, input) => {
    if (input.kind === 'Key') {
      const keyCode = keyboardKey.getKey(input.key.code);

      if (!keyCode) {
        return;
      }

      if (input.state === 'Pressed') {
        mainWindow.webContents.sendInputEvent({
          type: 'keyDown',
          keyCode: keyboardKey.getKey(input.key.code),
        });
      } else {
        mainWindow.webContents.sendInputEvent({
          type: 'KeyUp',
          keyCode: keyboardKey.getKey(input.key.code),
        });
      }
    } else {
      mainWindow.webContents.sendInputEvent({
        type: 'char',
        keyCode: input.ch,
      });
    }
  });

  overlay.event.on('input_capture_start', () => {
    mainWindow.show();
    // do full repaint
    mainWindow.webContents.invalidate();

    // Open the DevTools.
    mainWindow.webContents.openDevTools();
  });

  overlay.event.on('input_capture_end', () => {
    mainWindow.hide();
    overlay.clearSurface();
  });

  mainWindow.hide();
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

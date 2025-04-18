// Modules to control application life and create native browser window
import { app, BrowserWindow } from 'electron';
import path from 'node:path';
import { defaultDllDir, Overlay } from 'asdf-overlay-node';
import find from 'find-process';

async function createOverlayWindow(pid) {
  // Create the browser window.
  const mainWindow = new BrowserWindow({
    width: 600,
    height: 400,
    webPreferences: {
      offscreen: {
        useSharedTexture: true,
      },
    },
  });

  const overlay = await Overlay.attach(
    'electron-overlay',
    defaultDllDir().replace('app.asar', 'app.asar.unpacked'),
    pid,
  );

  mainWindow.webContents.on('paint', (e) => {
    if (!e.texture) {
      return;
    }

    (async () => {
      try {
        await overlay.updateShtex(e.texture.textureInfo.sharedTextureHandle);
      } finally {
        e.texture.release();
      }
    })();
  });

  // Open the DevTools.
  mainWindow.webContents.openDevTools();

  await mainWindow.loadURL('https://electronjs.org');
}

async function main() {
  await app.whenReady();

  app.on('activate', function () {
    // On macOS it's common to re-create a window in the app when the
    // dock icon is clicked and there are no other windows open.
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  });

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

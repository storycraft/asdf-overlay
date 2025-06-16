import path from 'node:path';
import { arch } from 'node:process';

import { Addon } from './addon.js';
import { fileURLToPath } from 'node:url';
import { EventEmitter } from 'node:events';
import { CursorInput, KeyboardInput } from './input.js';
import { PercentLength, CopyRect, Key, Cursor } from './types.js';

export * from './types.js';
export * from './util.js';

const addon = loadAddon();

function loadAddon(): Addon {
  const nodeModule = { exports: {} };

  let name: string;
  switch (arch) {
    case 'arm64': {
      name = '../addon-aarch64.node';
      break;
    }
    case 'x64': {
      name = '../addon-x64.node';
      break;
    }

    default: throw new Error(`Unsupported arch: ${arch}`);
  }
  process.dlopen(
    nodeModule,
    path.resolve(
      path.dirname(fileURLToPath(new URL(import.meta.url))),
      name,
    ),
  );

  return nodeModule.exports as Addon;
}

const idSym: unique symbol = Symbol("id");

export type OverlayEventEmitter = EventEmitter<{
  'added': [hwnd: number, width: number, height: number],
  'resized': [hwnd: number, width: number, height: number],
  'cursor_input': [hwnd: number, input: CursorInput],
  'keyboard_input': [hwnd: number, input: KeyboardInput],
  'input_blocking_ended': [hwnd: number],
  'destroyed': [hwnd: number],
  'error': [err: unknown],
  'disconnected': [],
}>;

export class Overlay {
  readonly event: OverlayEventEmitter = new EventEmitter();
  readonly [idSym]: number;

  private constructor(id: number) {
    this[idSym] = id;

    void (async () => {
      // wait until next tick so no events are lost
      await new Promise<void>(resolve => process.nextTick(resolve));

      try {
        while (await addon.overlayCallNextEvent(id, this.event, this.event.emit)) { }
      } catch (err) {
        if (this.event.listenerCount('error') != 0) {
          this.event.emit('error', err);
        } else {
          throw err;
        }
      } finally {
        this.destroy();
      }
    })();
  }

  /**
   * Update overlay position relative to window
   * @param hwnd target window hwnd
   * @param x x position
   * @param y y position
   */
  async setPosition(hwnd: number, x: PercentLength, y: PercentLength) {
    await addon.overlaySetPosition(this[idSym], hwnd, x, y);
  }

  /**
   * Update overlay anchor
   * @param hwnd target window hwnd
   * @param x x anchor
   * @param y y anchor
   */
  async setAnchor(hwnd: number, x: PercentLength, y: PercentLength) {
    await addon.overlaySetAnchor(this[idSym], hwnd, x, y);
  }

  /**
   * Update overlay margin
   * @param hwnd target window hwnd
   * @param top top margin
   * @param right right margin
   * @param bottom bottom margin
   * @param left left margin
   */
  async setMargin(
    hwnd: number,
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength,
  ) {
    await addon.overlaySetMargin(this[idSym], hwnd, top, right, bottom, left);
  }

  /**
   * Listen to window input without blocking
   * @param hwnd target window hwnd
   * @param cursor listen cursor input or not
   * @param keyboard listen keyboard input or not
   */
  async listenInput(
    hwnd: number,
    cursor: boolean,
    keyboard: boolean,
  ) {
    await addon.overlayListenInput(this[idSym], hwnd, cursor, keyboard);
  }

  /**
   * Block window input and listen them
   * @param hwnd target window hwnd
   * @param block set true to block input, false to release
   */
  async blockInput(
    hwnd: number,
    block: boolean,
  ) {
    await addon.overlayBlockInput(this[idSym], hwnd, block);
  }

  /**
   * Set cursor while in input blocking mode
   * @param hwnd target window hwnd
   * @param cursor cursor to set. Do not supply this value to hide cursor.
   */
  async setBlockingCursor(
    hwnd: number,
    cursor?: Cursor,
  ) {
    await addon.overlaySetBlockingCursor(this[idSym], hwnd, cursor);
  }

  /**
   * Update overlay using bitmap buffer. The size of overlay is `width x (data.byteLength / 4 / width)`
   * @param hwnd target window hwnd
   * @param width width of the bitmap
   * @param data bgra formatted bitmap
   */
  async updateBitmap(hwnd: number, width: number, data: Buffer) {
    await addon.overlayUpdateBitmap(this[idSym], hwnd, width, data);
  }

  /**
   * Update overlay using D3D11 shared texture.
   * @param hwnd target window hwnd
   * @param width width of the surface
   * @param height height of the surface
   * @param handle NT Handle of shared D3D11 Texture
   * @param rect Area to update
   */
  async updateShtex(hwnd: number, width: number, height: number, handle: Buffer, rect?: CopyRect) {
    await addon.overlayUpdateShtex(this[idSym], hwnd, width, height, handle, rect);
  }

  /**
   * Clear overlay
   * @param hwnd target window hwnd
   */
  async clearSurface(hwnd: number) {
    await addon.overlayClearSurface(this[idSym], hwnd);
  }

  /**
   * Destroy overlay
   */
  destroy() {
    addon.overlayDestroy(this[idSym]);
    this.event.emit('disconnected');
  }

  /**
   * Attach overlay to target process
   * 
   * Name must be unique or it will fail if there is a connection with same name
   * @param dllDir path to dlls
   * @param pid target process pid
   * @param timeout Timeout for injection, in milliseconds. Will wait indefinitely if not provided.
   * @returns new {@link Overlay} object
   */
  static async attach(dllDir: string, pid: number, timeout?: number): Promise<Overlay> {
    return new Overlay(await addon.attach(dllDir, pid, timeout));
  }
}

/**
 * Default dll directory path
 */
export function defaultDllDir(): string {
  return path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    '../',
  );
}

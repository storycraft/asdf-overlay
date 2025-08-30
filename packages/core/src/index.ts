import path from 'node:path';
import { arch } from 'node:process';

import { Addon } from './addon.js';
import { fileURLToPath } from 'node:url';
import { EventEmitter } from 'node:events';
import { CursorInput, KeyboardInput } from './input.js';
import { PercentLength, CopyRect, Cursor } from './types.js';

export * from './types.js';
export * from './util.js';

/**
 * Global node addon instance
 */
const addon = loadAddon();

/**
 * Load node addon depending on system architecture.
 */
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

/**
 * Unique symbol for accessing internal id
 */
const idSym: unique symbol = Symbol('id');

export type OverlayEventEmitter = EventEmitter<{
  /**
   * A window has been added.
   */
  added: [id: number, width: number, height: number],

  /**
   * A window has been resized.
   */
  resized: [id: number, width: number, height: number],

  /**
   * Cursor input from a window.
   */
  cursor_input: [id: number, input: CursorInput],

  /**
   * Keyboard input from a window.
   */
  keyboard_input: [id: number, input: KeyboardInput],

  /**
   * Input blocking to a window is interrupted and turned off.
   */
  input_blocking_ended: [id: number],

  /**
   * Window is destroyed.
   */
  destroyed: [id: number],

  /**
   * An error has occured on ipc connection.
   */
  error: [err: unknown],

  /**
   * Ipc disconnected.
   */
  disconnected: [],
}>;

export class Overlay {
  readonly event: OverlayEventEmitter = new EventEmitter();
  readonly [idSym]: number;

  private constructor(id: number) {
    this[idSym] = id;

    void (async () => {
      // wait until next tick so no events are lost
      await new Promise<void>((resolve) => {
        process.nextTick(resolve);
      });

      try {
        for (; ;) {
          const hasNext = await addon.overlayCallNextEvent(
            id,
            this.event,
            (name, ...args) => this.event.emit(name, ...args),
          );

          if (!hasNext) break;
        }
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
   * @param id target window id
   * @param x x position
   * @param y y position
   */
  async setPosition(id: number, x: PercentLength, y: PercentLength) {
    await addon.overlaySetPosition(this[idSym], id, x, y);
  }

  /**
   * Update overlay anchor
   * @param id target window id
   * @param x x anchor
   * @param y y anchor
   */
  async setAnchor(id: number, x: PercentLength, y: PercentLength) {
    await addon.overlaySetAnchor(this[idSym], id, x, y);
  }

  /**
   * Update overlay margin
   * @param id target window id
   * @param top top margin
   * @param right right margin
   * @param bottom bottom margin
   * @param left left margin
   */
  async setMargin(
    id: number,
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength,
  ) {
    await addon.overlaySetMargin(this[idSym], id, top, right, bottom, left);
  }

  /**
   * Listen to window input without blocking
   * @param id target window id
   * @param cursor listen cursor input or not
   * @param keyboard listen keyboard input or not
   */
  async listenInput(
    id: number,
    cursor: boolean,
    keyboard: boolean,
  ) {
    await addon.overlayListenInput(this[idSym], id, cursor, keyboard);
  }

  /**
   * Block window input and listen them
   * @param id target window id
   * @param block set true to block input, false to release
   */
  async blockInput(
    id: number,
    block: boolean,
  ) {
    await addon.overlayBlockInput(this[idSym], id, block);
  }

  /**
   * Set cursor while in input blocking mode
   * @param id target window id
   * @param cursor cursor to set. Do not supply this value to hide cursor.
   */
  async setBlockingCursor(
    id: number,
    cursor?: Cursor,
  ) {
    await addon.overlaySetBlockingCursor(this[idSym], id, cursor);
  }

  /**
   * Update overlay using bitmap buffer. The size of overlay is `width x (data.byteLength / 4 / width)`
   * @param id target window id
   * @param width width of the bitmap
   * @param data bgra formatted bitmap
   */
  async updateBitmap(id: number, width: number, data: Buffer) {
    await addon.overlayUpdateBitmap(this[idSym], id, width, data);
  }

  /**
   * Update overlay using D3D11 shared texture.
   * @param id target window id
   * @param width width of the surface
   * @param height height of the surface
   * @param handle NT Handle of shared D3D11 Texture
   * @param rect Area to update
   */
  async updateShtex(id: number, width: number, height: number, handle: Buffer, rect?: CopyRect) {
    await addon.overlayUpdateShtex(this[idSym], id, width, height, handle, rect);
  }

  /**
   * Clear overlay
   * @param id target window id
   */
  async clearSurface(id: number) {
    await addon.overlayClearSurface(this[idSym], id);
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

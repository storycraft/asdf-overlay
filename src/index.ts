import path from 'node:path';
import { arch } from 'node:process';

import { Addon } from './addon.js';
import { fileURLToPath } from 'node:url';
import { EventEmitter } from 'node:events';

export * from './util.js';

export type PercentLength = {
  ty: 'percent' | 'length',
  value: number,
}

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
  'resized': [hwnd: number, width: number, height: number],
  'destroyed': [hwnd: number],
  'disconnected': [],
}>;

export class Overlay {
  readonly event: OverlayEventEmitter = new EventEmitter();
  readonly [idSym]: number;

  private constructor(id: number) {
    this[idSym] = id;

    void (async () => {
      // wait until next tick so no events are lost
      await new Promise(process.nextTick);

      try {
        for (; ;) {
          await addon.overlayNextEvent(id);
        }
      } catch (e) {
        this.event.emit('disconnected');
      }
    })();
  }

  /**
   * Update overlay position relative to window
   * @param x x position
   * @param y y position
   */
  async setPosition(x: PercentLength, y: PercentLength) {
    await addon.overlaySetPosition(this[idSym], x, y);
  }

  /**
   * Update overlay anchor
   * @param x x anchor
   * @param y y anchor
   */
  async setAnchor(x: PercentLength, y: PercentLength) {
    await addon.overlaySetAnchor(this[idSym], x, y);
  }

  /**
   * Update overlay margin
   * @param top top margin
   * @param right right margin
   * @param bottom bottom margin
   * @param left left margin
   */
  async setMargin(
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength,
  ) {
    await addon.overlaySetMargin(this[idSym], top, right, bottom, left);
  }

  /**
   * Update overlay using bitmap buffer. The size of overlay is `width x (data.byteLength / 4 / width)`
   * @param width width of the bitmap
   * @param data bgra formatted bitmap
   */
  async updateBitmap(width: number, data: Buffer) {
    await addon.overlayUpdateBitmap(this[idSym], width, data);
  }

  /**
   * Update overlay using D3D11 shared texture.
   * @param handle NT Handle of shared D3D11 Texture
   */
  async updateShtex(handle: Buffer) {
    await addon.overlayUpdateShtex(this[idSym], handle);
  }

  /**
   * Clear overlay
   */
  async clearSurface() {
    await addon.overlayClearSurface(this[idSym]);
  }

  /**
   * Destroy overlay
   */
  destroy() {
    addon.overlayDestroy(this[idSym]);
  }

  /**
   * Attach overlay to target process
   * 
   * Name must be unique or it will fail if there is a connection with same name
   * @param name name of ipc pipe and overlay thread
   * @param dllDir path to dlls
   * @param pid target process pid
   * @param timeout Timeout for injection, in milliseconds. Will wait indefinitely if not provided.
   * @returns new {@link Overlay} object
   */
  static async attach(name: string, dllDir: string, pid: number, timeout?: number): Promise<Overlay> {
    return new Overlay(await addon.attach(name, dllDir, pid, timeout));
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

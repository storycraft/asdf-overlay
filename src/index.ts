import path from 'node:path';

const addon: {
  attach(dllDir: string, pid: number, timeout?: number): Promise<number>,

  overlaySetPosition(id: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetAnchor(id: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetMargin(
    id: number,
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength
  ): Promise<void>,

  overlayUpdateBitmap(id: number, width: number, data: Buffer): Promise<void>,

  overlayDestroy(id: number): void,
} = require('../index.node');

const idSym: unique symbol = Symbol("id");

export type PercentLength = {
  ty: 'percent' | 'length',
  value: number,
}

export function percent(value: number): PercentLength {
  return {
    ty: 'percent',
    value,
  };
}

export function length(value: number): PercentLength {
  return {
    ty: 'length',
    value,
  };
}

export class Overlay {
  readonly [idSym]: number;

  private constructor(id: number) {
    this[idSym] = id;
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
   * Destroy overlay
   */
  destroy() {
    addon.overlayDestroy(this[idSym]);
  }

  /**
   * Attach overlay to target process
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
  return path.resolve(__dirname, '..');
}

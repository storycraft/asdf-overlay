import { OverlayEventEmitter } from './index.js';
import { CopyRect, Cursor, Key, PercentLength } from './types.js';

export type Addon = {
  attach(name: string, dllDir: string, pid: number, timeout?: number): Promise<number>,

  overlaySetPosition(id: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetAnchor(id: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetMargin(
    id: number,
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength,
  ): Promise<void>,
  overlaySetInputCaptureKeybind(
    id: number,
    hwnd: number,
    keybind: [Key?, Key?, Key?, Key?],
  ): Promise<void>,
  overlaySetCaptureCursor(
    id: number,
    hwnd: number,
    cursor?: Cursor,
  ): Promise<void>,

  overlayGetSize(id: number, hwnd: number): Promise<[width: number, height: number] | null>,

  overlayUpdateBitmap(id: number, width: number, data: Buffer): Promise<void>,
  overlayUpdateShtex(id: number, width: number, height: number, handle: Buffer, rect?: CopyRect): Promise<void>,
  overlayClearSurface(id: number): Promise<void>,

  overlayCallNextEvent(
    id: number,
    emitter: OverlayEventEmitter,
    emit: OverlayEventEmitter['emit'],
  ): Promise<boolean>,

  overlayDestroy(id: number): void,
};

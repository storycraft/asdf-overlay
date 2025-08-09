import { OverlayEventEmitter } from './index.js';
import { CopyRect, Cursor, PercentLength } from './types.js';

export type Addon = {
  attach(dllDir: string, pid: number, timeout?: number): Promise<number>,

  overlaySetPosition(id: number, winId: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetAnchor(id: number, winId: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetMargin(
    id: number,
    winId: number,
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength,
  ): Promise<void>,

  overlayListenInput(
    id: number,
    winId: number,
    cursor: boolean,
    keyboard: boolean,
  ): Promise<void>,

  overlayBlockInput(
    id: number,
    winId: number,
    block: boolean,
  ): Promise<void>,
  overlaySetBlockingCursor(
    id: number,
    winId: number,
    cursor?: Cursor,
  ): Promise<void>,

  overlayUpdateBitmap(id: number, winId: number, width: number, data: Buffer): Promise<void>,
  overlayUpdateShtex(id: number, winId: number, width: number, height: number, handle: Buffer, rect?: CopyRect): Promise<void>,
  overlayClearSurface(id: number, winId: number): Promise<void>,

  overlayCallNextEvent(
    id: number,
    emitter: OverlayEventEmitter,
    emit: OverlayEventEmitter['emit'],
  ): Promise<boolean>,

  overlayDestroy(id: number): void,
};

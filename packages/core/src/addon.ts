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

  overlayCallNextEvent(
    id: number,
    emitter: OverlayEventEmitter,
    emit: OverlayEventEmitter['emit'],
  ): Promise<boolean>,

  overlayDestroy(id: number): void,

  surfaceCreate(luid: unknown): number,
  surfaceClear(id: number): void,
  surfaceUpdateBitmap(id: number, width: number, data: Buffer): void,
  surfaceUpdateShtex(id: number, width: number, height: number, handle: Buffer, rect?: CopyRect): void,
  surfaceDestroy(id: number): void,
};

import { OverlayEventEmitter } from './index.js';
import { CopyRect, Cursor, PercentLength, type GpuLuid, type UpdateSharedHandle } from './types.js';

export type Addon = {
  attach(dllDir: string, pid: number, timeout?: number): Promise<unknown>,

  overlaySetPosition(id: unknown, winId: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetAnchor(id: unknown, winId: number, x: PercentLength, y: PercentLength): Promise<void>,
  overlaySetMargin(
    id: unknown,
    winId: number,
    top: PercentLength,
    right: PercentLength,
    bottom: PercentLength,
    left: PercentLength,
  ): Promise<void>,

  overlayUpdateHandle(
    id: unknown,
    winId: number,
    update: UpdateSharedHandle,
  ): Promise<void>,

  overlayListenInput(
    id: unknown,
    winId: number,
    cursor: boolean,
    keyboard: boolean,
  ): Promise<void>,

  overlayBlockInput(
    id: unknown,
    winId: number,
    block: boolean,
  ): Promise<void>,
  overlaySetBlockingCursor(
    id: unknown,
    winId: number,
    cursor?: Cursor,
  ): Promise<void>,

  overlayCallNextEvent(
    id: unknown,
    emitter: OverlayEventEmitter,
    emit: OverlayEventEmitter['emit'],
  ): Promise<boolean>,

  overlayDestroy(id: unknown): void,

  surfaceCreate(luid: GpuLuid): unknown,
  surfaceClear(id: unknown): void,
  surfaceUpdateBitmap(id: unknown, width: number, data: Buffer): UpdateSharedHandle | null,
  surfaceUpdateShtex(id: unknown, width: number, height: number, handle: Buffer, rect?: CopyRect): UpdateSharedHandle | null,
};

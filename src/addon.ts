import { PercentLength } from './index.js';

export type Addon = {
    attach(name: string, dllDir: string, pid: number, timeout?: number): Promise<number>,

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
    overlayUpdateShtex(id: number, handle: Buffer): Promise<void>,
    overlayClearSurface(id: number): Promise<void>,

    overlayNextEvent(id: number): Promise<void>,

    overlayDestroy(id: number): void,
};

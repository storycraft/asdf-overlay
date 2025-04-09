export type Addon = {
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
};

export type PercentLength = {
    ty: 'percent' | 'length',
    value: number,
}

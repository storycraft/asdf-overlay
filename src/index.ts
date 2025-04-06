import path from 'node:path';

const addon: {
    attach(dllDir: string, pid: number, timeout?: number): Promise<number>,
    overlayUpdateBitmap(id: number, width: number, data: Buffer): Promise<void>,
    overlayReposition(id: number, x: number, y: number): Promise<void>,
    overlayClose(id: number): Promise<boolean>,
} = require('../index.node');

const idSym: unique symbol = Symbol("id");

export class Overlay {
    readonly [idSym]: number;

    private constructor(id: number) {
        this[idSym] = id;
    }

    /**
     * Update overlay position relative to window
     * @param x x position. 0 is left
     * @param y y position. 0 is top
     */
    async reposition(x: number, y: number) {
        await addon.overlayReposition(this[idSym], x, y);
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
     * Close overlay
     */
    async close() {
        await addon.overlayClose(this[idSym]);
    }

    /**
     * Attach overlay to target process
     * @param pid target process pid
     * @param timeout Timeout for injection, in milliseconds. Will wait indefinitely if not provided.
     * @returns new {@link Overlay} object
     */
    static async attach(pid: number, timeout?: number): Promise<Overlay> {
        return new Overlay(await addon.attach(path.resolve(__dirname, '..'), pid, timeout));
    }
}
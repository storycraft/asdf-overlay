// The Rust addon.
import * as addon from './load.cjs';

declare module './load.cjs' {
    function attach(name: string): Promise<number>;
    function overlayUpdateBitmap(id: number, width: number, data: ArrayBuffer): Promise<void>;
    function overlayReposition(id: number, x: number, y: number): Promise<void>;
    function overlayClose(id: number): Promise<boolean>;
}

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
    async updateBitmap(width: number, data: ArrayBuffer) {
        await addon.overlayUpdateBitmap(this[idSym], width, data);
    }

    /**
     * Attach overlay to target process
     * @param name process name
     * @returns new {@link Overlay} object
     */
    static async attach(name: string): Promise<Overlay> {
        return new Overlay(await addon.attach(name));
    }
}
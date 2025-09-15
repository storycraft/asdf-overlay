import { EventEmitter } from 'node:events';
import { CursorInput, KeyboardInput } from './input.js';
import { PercentLength, CopyRect, Cursor } from './types.js';
export * from './types.js';
export * from './util.js';
declare const idSym: unique symbol;
export type OverlayEventEmitter = EventEmitter<{
    added: [id: number, width: number, height: number];
    resized: [id: number, width: number, height: number];
    cursor_input: [id: number, input: CursorInput];
    keyboard_input: [id: number, input: KeyboardInput];
    input_blocking_ended: [id: number];
    destroyed: [id: number];
    error: [err: unknown];
    disconnected: [];
}>;
export declare class Overlay {
    readonly event: OverlayEventEmitter;
    readonly [idSym]: number;
    private constructor();
    /**
     * Update overlay position relative to window
     * @param id target window id
     * @param x x position
     * @param y y position
     */
    setPosition(id: number, x: PercentLength, y: PercentLength): Promise<void>;
    /**
     * Update overlay anchor
     * @param id target window id
     * @param x x anchor
     * @param y y anchor
     */
    setAnchor(id: number, x: PercentLength, y: PercentLength): Promise<void>;
    /**
     * Update overlay margin
     * @param id target window id
     * @param top top margin
     * @param right right margin
     * @param bottom bottom margin
     * @param left left margin
     */
    setMargin(id: number, top: PercentLength, right: PercentLength, bottom: PercentLength, left: PercentLength): Promise<void>;
    /**
     * Listen to window input without blocking
     * @param id target window id
     * @param cursor listen cursor input or not
     * @param keyboard listen keyboard input or not
     */
    listenInput(id: number, cursor: boolean, keyboard: boolean): Promise<void>;
    /**
     * Block window input and listen them
     * @param id target window id
     * @param block set true to block input, false to release
     */
    blockInput(id: number, block: boolean): Promise<void>;
    /**
     * Set cursor while in input blocking mode
     * @param id target window id
     * @param cursor cursor to set. Do not supply this value to hide cursor.
     */
    setBlockingCursor(id: number, cursor?: Cursor): Promise<void>;
    /**
     * Update overlay using bitmap buffer. The size of overlay is `width x (data.byteLength / 4 / width)`
     * @param id target window id
     * @param width width of the bitmap
     * @param data bgra formatted bitmap
     */
    updateBitmap(id: number, width: number, data: Buffer): Promise<void>;
    /**
     * Update overlay using D3D11 shared texture.
     * @param id target window id
     * @param width width of the surface
     * @param height height of the surface
     * @param handle NT Handle of shared D3D11 Texture
     * @param rect Area to update
     */
    updateShtex(id: number, width: number, height: number, handle: Buffer, rect?: CopyRect): Promise<void>;
    /**
     * Clear overlay
     * @param id target window id
     */
    clearSurface(id: number): Promise<void>;
    /**
     * Destroy overlay
     */
    destroy(): void;
    /**
     * Attach overlay to target process
     *
     * Name must be unique or it will fail if there is a connection with same name
     * @param dllDir path to dlls
     * @param pid target process pid
     * @param timeout Timeout for injection, in milliseconds. Will wait indefinitely if not provided.
     * @returns new {@link Overlay} object
     */
    static attach(dllDir: string, pid: number, timeout?: number): Promise<Overlay>;
}
/**
 * Default dll directory path
 */
export declare function defaultDllDir(): string;

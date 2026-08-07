import { EventEmitter } from 'node:events';

export declare type OverlayEventEmitter = EventEmitter<{
  /**
  * A window has been added.
  */
  added: [id: number, width: number, height: number, luid: GpuLuid],

  /**
   * A window has been resized.
   */
  resized: [id: number, width: number, height: number],

  /**
   * Cursor input from a window.
   */
  cursor_input: [id: number, input: CursorInput],

  /**
   * Keyboard input from a window.
   */
  keyboard_input: [id: number, input: KeyboardInput],

  /**
   * Input blocking to a window is interrupted and turned off.
   */
  input_blocking_ended: [id: number],

  /**
   * Window is destroyed.
   */
  destroyed: [id: number],

  /**
   * An error has occured on ipc connection.
   */
  error: [err: unknown],

  /**
   * Ipc disconnected.
   */
  disconnected: [],
}>;

import { EventEmitter } from 'node:events';

export declare type OverlayEventEmitter = EventEmitter<{
  /**
  * A window has been added.
  */
  window_added: [id: number, width: number, height: number],

  /**
   * A window has been resized.
   */
  window_resized: [id: number, width: number, height: number],

  /**
   * Cursor input from a window.
   */
  window_cursor_input: [id: number, input: CursorInput],

  /**
   * Keyboard input from a window.
   */
  window_keyboard_input: [id: number, input: KeyboardInput],

  /**
   * Window is destroyed.
   */
  window_destroyed: [id: number],

  /**
   * A surface has been added.
   */
  surface_added: [id: bigint, width: number, height: number, luid: GpuLuid],

  /**
   * A surface has been resized.
   */
  surface_resized: [id: bigint, width: number, height: number],

  /**
   * A surface has been destroyed.
   */
  surface_destroyed: [id: bigint],

  /**
   * Input blocking is interrupted and turned off.
   */
  input_blocking_ended: [id: number],

  /**
   * A log message from the overlay system.
   */
  log: [level: LogLevel, message: string],

  /**
   * An error has occured on ipc connection.
   */
  error: [err: unknown],

  /**
   * Ipc disconnected.
   */
  disconnected: [],
}>;

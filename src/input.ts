import { Key } from './index.js';

export type CursorInput = {
  clientX: number,
  clientY: number,
  windowX: number,
  windowY: number,
} & ({
  kind: 'Enter'
} | {
  kind: 'Leave'
} | {
  kind: 'Action',

  state: InputState,
  action: CursorAction,
} | {
  kind: 'Move',
} | {
  kind: 'Scroll',

  axis: ScrollAxis,

  /**
   * Scroll tick delta
   */
  delta: number,
});

export type KeyboardInput = {
  kind: 'Key',
  key: Key,

  /**
   * Key input state
   */
  state: InputState,
} | {
  kind: 'Char',
  ch: string,
};

export type InputState = 'Pressed' | 'Released';
export type CursorAction = 'Left' | 'Right' | 'Middle' | 'Back' | 'Forward';
export type ScrollAxis = 'X' | 'Y';
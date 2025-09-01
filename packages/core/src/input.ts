import { Key } from './index.js';

/**
 * Describe a cursor input.
 */
export type CursorInput = {
  /**
   * X position relative to overlay surface.
   */
  clientX: number,

  /**
   * Y position relative to overlay surface.
   */
  clientY: number,

  /**
   * X position relative to window.
   */
  windowX: number,

  /**
   * Y position relative to window.
   */
  windowY: number,
} & ({
  /**
   * Cursor has entered to a window.
   */
  kind: 'Enter',
} | {
  /**
   * Cursor left a window.
   */
  kind: 'Leave',
} | {
  /**
   * Cursor button is pressed.
   */
  kind: 'Action',

  /**
   * Cursor button input state.
   */
  state: InputState,

  /**
   * True if `state` is `Pressed` and the button is secondly pressed within system double click time.
   */
  doubleClick: boolean,

  /**
   * Pressed cursor button.
   */
  action: CursorAction,
} | {
  /**
   * Cursor moved.
   */
  kind: 'Move',
} | {
  /**
   * Cursor scrolled.
   */
  kind: 'Scroll',

  /**
   * Scroll axis.
   */
  axis: ScrollAxis,

  /**
   * Scroll tick delta.
   */
  delta: number,
});

/**
 * Describe a keyboard input.
 */
export type KeyboardInput = {
  /**
   * A key is pressed or released.
   */
  kind: 'Key',

  /**
   * Keyboard key without considering keyboard layout.
   */
  key: Key,

  /**
   * Keyboard key input state.
   */
  state: InputState,
} | {
  /**
   * A character input due to a key press without involving IME.
   */
  kind: 'Char',

  /**
   * Input character (1 character).
   */
  ch: string,
} | {
  /**
   * IME related event.
   */
  kind: 'Ime',

  /**
   * An IME event.
   */
  ime: Ime,
};

/**
 * Describe a IME event.
 */
export type Ime = {
  /**
   * IME is enabled due to window focus or etc.
   */
  kind: 'Enabled',

  /**
   * Initial IME language in ETF language tag(BCP 47) format.
   */
  lang: string,

  /**
   * Initial IME conversion mode.
   */
  conversion: ImeConversion,
} | {
  /**
   * IME language is changed.
   */
  kind: 'Changed',

  /**
   * Changed IME language.
   */
  lang: string,
} | {
  /**
   * IME conversion mode is changed.
   */
  kind: 'ChangedConversion',

  /**
   * Changed IME conversion mode.
   */
  conversion: ImeConversion,
} | {
  /**
   * IME candidate is added/updated.
   *
   * The sent candidates are only valid until another `CandidateChanged` or `CandidateClosed` event.
   */
  kind: 'CandidateChanged',
  list: ImeCandidateList,
} | {
  /**
   * IME candidate window is closed.
   */
  kind: 'CandidateClosed',
} | {
  /**
   * IME is composing text.
   */
  kind: 'Compose',

  /**
   * Composing text.
   */
  text: string,

  /**
   * Current caret index in composing text.
   */
  caret: number,
} | {
  /**
   * IME commits text.
   *
   * This event should clear prior composing text.
   */
  kind: 'Commit',

  /**
   * Composed text.
   */
  text: string,
} | {
  /**
   * IME is disabled due to losing focus or etc.
   */
  kind: 'Disabled',
};

/**
 * IME candidate list.
 */
export type ImeCandidateList = {
  /**
   * Start index of current page.
   */
  pageStartIndex: number,

  /**
   * Count of candidate item per page.
   */
  pageSize: number,

  /**
   * Currently selected candidate index.
   */
  selectedIndex: number,

  /**
   * Candidate list.
   */
  candidates: string[],
};

/**
 * IME conversion bit flags.
 *
 * There can be multiple flag set.
 */
export enum ImeConversion {
  None = 0,

  /**
   * IME converts to native langauge.
   */
  Native = 1,

  /**
   * IME composes in full-width characters.
   */
  Fullshape = 2,

  /**
   * Conversion is disabled.
   */
  NoConversion = 4,

  /**
   * Converting to hanja.
   */
  HanjaConvert = 8,

  /**
   * Converting to katakana.
   */
  Katakana = 16,
}

/**
 * Key input state.
 */
export type InputState = 'Pressed' | 'Released';

/**
 * Cursor buttons.
 */
export type CursorAction = 'Left' | 'Right' | 'Middle' | 'Back' | 'Forward';

/**
 * Cursor scroll axis.
 */
export type ScrollAxis = 'X' | 'Y';

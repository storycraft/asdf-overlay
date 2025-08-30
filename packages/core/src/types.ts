/**
 * A length which can be expressed in absolute or relative percent.
 */
export type PercentLength = {
  ty: 'percent' | 'length',
  value: number,
};

/**
 * Describe a rectangle when copying from source to destination.
 */
export type CopyRect = {
  /**
   * Destination X position.
   */
  dstX: number,

  /**
   * Destination Y position.
   */
  dstY: number,

  /**
   * Source rectangle.
   */
  src: Rect,
};

/**
 * Describe a Reactangle.
 */
export type Rect = {
  /**
   * X position.
   */
  x: number,

  /**
   * Y position.
   */
  y: number,

  /**
   * Width of the Rectangle.
   */
  width: number,

  /**
   * Height of the Rectangle.
   */
  height: number,
};

/**
 * A keyboard key.
 */
export type Key = {
  /**
   * Windows virtual key code.
   */
  code: number,

  /**
   * Extended flag.
   *
   * True for right key variant (e.g. Right shift), or Numpad variant (e.g. NumPad 1)
   */
  extended: boolean,
};

/**
 * Describe a cursor type.
 */
export enum Cursor {
  Default = 0,
  Help,
  Pointer,
  Progress,
  Wait,
  Cell,
  Crosshair,
  Text,
  VerticalText,
  Alias,
  Copy,
  Move,
  NotAllowed,
  Grab,
  Grabbing,
  ColResize,
  RowResize,
  EastWestResize,
  NorthSouthResize,
  NorthEastSouthWestResize,
  NorthWestSouthEastResize,
  ZoomIn,
  ZoomOut,

  // Windows additional cursors
  UpArrow,
  Pin,
  Person,
  Pen,
  Cd,

  // Panning cursors
  PanMiddle,
  PanMiddleHorizontal,
  PanMiddleVertical,
  PanEast,
  PanNorth,
  PanNorthEast,
  PanNorthWest,
  PanSouth,
  PanSouthEast,
  PanSouthWest,
  PanWest,
};

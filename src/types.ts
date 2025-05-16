export type PercentLength = {
  ty: 'percent' | 'length',
  value: number,
};

export type CopyRect = {
  dstX: number,
  dstY: number,
  src: Rect,
}

export type Rect = {
  x: number,
  y: number,
  width: number,
  height: number,
};

export type Key = {
  /**
   * Windows virtual key code
   */
  code: number,

  /**
   * Extended flag
   */
  extended: boolean,
};

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

    // panning
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
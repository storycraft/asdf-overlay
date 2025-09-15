export type PercentLength = {
    ty: 'percent' | 'length';
    value: number;
};
export type CopyRect = {
    dstX: number;
    dstY: number;
    src: Rect;
};
export type Rect = {
    x: number;
    y: number;
    width: number;
    height: number;
};
export type Key = {
    /**
     * Windows virtual key code
     */
    code: number;
    /**
     * Extended flag
     */
    extended: boolean;
};
export declare enum Cursor {
    Default = 0,
    Help = 1,
    Pointer = 2,
    Progress = 3,
    Wait = 4,
    Cell = 5,
    Crosshair = 6,
    Text = 7,
    VerticalText = 8,
    Alias = 9,
    Copy = 10,
    Move = 11,
    NotAllowed = 12,
    Grab = 13,
    Grabbing = 14,
    ColResize = 15,
    RowResize = 16,
    EastWestResize = 17,
    NorthSouthResize = 18,
    NorthEastSouthWestResize = 19,
    NorthWestSouthEastResize = 20,
    ZoomIn = 21,
    ZoomOut = 22,
    UpArrow = 23,
    Pin = 24,
    Person = 25,
    Pen = 26,
    Cd = 27,
    PanMiddle = 28,
    PanMiddleHorizontal = 29,
    PanMiddleVertical = 30,
    PanEast = 31,
    PanNorth = 32,
    PanNorthEast = 33,
    PanNorthWest = 34,
    PanSouth = 35,
    PanSouthEast = 36,
    PanSouthWest = 37,
    PanWest = 38
}

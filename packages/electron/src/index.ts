import type { Overlay } from '@asdf-overlay/core';

export * from './input/index.js';
export * from './surface.js';

export type OverlayWindow = {
  overlay: Overlay,
  id: number,
};

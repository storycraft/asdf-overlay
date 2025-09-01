import type { Overlay } from '@asdf-overlay/core';

/**
 * Describe a window in `Overlay`.
 */
export type OverlayWindow = {
  /**
   * Associated `Overlay` instance.
   */
  overlay: Overlay,

  /**
   * Window id.
   */
  id: number,
};

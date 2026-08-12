import type { Overlay, SurfaceInfo } from '@asdf-overlay/core';

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

/**
 * Describe a surface in `Overlay`.
 */
export type OverlaySurface = {
  /**
   * Associated `Overlay` instance.
   */
  overlay: Overlay,

  /**
   * Surface id.
   */
  id: bigint,

  /**
   * Surface info.
   */
  info: SurfaceInfo,
};

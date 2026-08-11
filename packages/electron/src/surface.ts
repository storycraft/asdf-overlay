import type { NativeImage, OffscreenSharedTexture, WebContents, WebContentsPaintEventParams } from 'electron';
import EventEmitter from 'node:events';
import { OverlaySurface as CoreOverlaySurface, type GpuLuid } from '@asdf-overlay/core';
import type { OverlaySurface } from './index.js';

type Emitter = EventEmitter<{
  /**
   * An error has been occured while copying to overlay surface.
   */
  error: [e: unknown],
}>;

/**
 * Connection from a Electron offscreen window to a overlay surface.
 */
export class ElectronOverlaySurface {
  /**
   * Events during paints.
   */
  readonly events: Emitter = new EventEmitter();

  private handler: (
    e: Electron.Event<WebContentsPaintEventParams>,
    dirtyRect: Electron.Rectangle,
    image: NativeImage,
  ) => void;

  private readonly inner: CoreOverlaySurface;

  private constructor(
    private readonly surface: OverlaySurface,
    luid: GpuLuid,
    private readonly contents: WebContents,
  ) {
    this.inner = new CoreOverlaySurface(luid);

    this.handler = (e, rect, image) => {
      try {
        const update = e.texture ? this.paintAccelerated(e.texture) : this.paintSoftware(rect, image);

        if (update) {
          this.surface.overlay.updateHandle(this.surface.id, update)
            .catch((e: unknown) => this.events.emit('error', e));
        }
      } catch (err) {
        this.events.emit('error', err);
      }
    };

    contents.on('paint', this.handler);
    contents.invalidate();
  }

  /**
   * Connect Electron `WebContents` surface to target overlay window.
   */
  static connect(
    surface: OverlaySurface,
    luid: GpuLuid,
    contents: WebContents,
  ): ElectronOverlaySurface {
    return new ElectronOverlaySurface({ ...surface }, luid, contents);
  }

  /**
   * Disconnect surface from Electron window and clear overlay surface.
   */
  async disconnect() {
    this.contents.off('paint', this.handler);
    await this.surface.overlay.updateHandle(this.surface.id, { type: 'None' });
  }

  /**
   * Copy overlay texture in gpu accelerated shared texture mode.
   */
  private paintAccelerated(texture: OffscreenSharedTexture) {
    const info = texture.textureInfo;

    try {
      // TODO:: cross platform handle
      if (info.widgetType !== 'frame' || !info.handle.ntHandle) {
        return null;
      }
      const rect = info.metadata.captureUpdateRect ?? info.contentRect;

      // update only changed part
      return this.inner.updateNtShtex(
        info.codedSize.width,
        info.codedSize.height,
        info.handle.ntHandle,
        {
          dstX: rect.x,
          dstY: rect.y,
          src: rect,
        },
      );
    } finally {
      texture.release();
    }
  }

  /**
   * Copy overlay texture from bitmap surface.
   */
  private paintSoftware(
    _dirtyRect: Electron.Rectangle,
    image: NativeImage,
  ) {
    const size = image.getSize();
    // offscreenTexture undefined if image is empty, handle the case
    if (size.width === 0 || size.height === 0) {
      return null;
    }

    // TODO:: update only changed part
    return this.inner.updateBitmap(
      image.getSize().width,
      image.toBitmap(),
    );
  }
}

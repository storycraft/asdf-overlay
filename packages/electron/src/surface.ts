import type { NativeImage, TextureInfo, WebContents, WebContentsPaintEventParams } from 'electron';
import type { OverlayWindow } from './index.js';
import EventEmitter from 'node:events';
import { OverlaySurface, type GpuLuid } from '@asdf-overlay/core';

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

  private readonly surface: OverlaySurface;

  private constructor(
    private readonly window: OverlayWindow,
    luid: GpuLuid,
    private readonly contents: WebContents,
  ) {
    this.surface = OverlaySurface.create(luid);

    this.handler = (e, rect, image) => {
      const offscreenTexture = e.texture;

      if (offscreenTexture) {
        void this.paintAccelerated(offscreenTexture.textureInfo).finally(() => {
          offscreenTexture.release();
        });
      } else {
        const size = image.getSize();
        // offscreenTexture undefined if image is empty, handle the case
        if (size.width === 0 || size.height === 0) {
          return;
        }

        void this.paintSoftware(rect, image);
      }
    };

    contents.on('paint', this.handler);
    contents.invalidate();
  }

  /**
   * Connect Electron `WebContents` surface to target overlay window.
   */
  static connect(
    window: OverlayWindow,
    luid: GpuLuid,
    contents: WebContents,
  ): ElectronOverlaySurface {
    return new ElectronOverlaySurface({ ...window }, luid, contents);
  }

  /**
   * Disconnect surface from Electron window and clear overlay surface.
   */
  async disconnect() {
    this.contents.off('paint', this.handler);
    await this.window.overlay.updateHandle(this.window.id, {});
    this.surface.destroy();
  }

  /**
   * Copy overlay texture in gpu accelerated shared texture mode.
   */
  private async paintAccelerated(texture: TextureInfo) {
    // TODO:: cross platform handle
    if (texture.widgetType !== 'frame' || !texture.handle.ntHandle) {
      return;
    }
    const rect = texture.metadata.captureUpdateRect ?? texture.contentRect;

    // update only changed part
    try {
      const update = this.surface.updateShtex(
        texture.codedSize.width,
        texture.codedSize.height,
        texture.handle.ntHandle,
        {
          dstX: rect.x,
          dstY: rect.y,
          src: rect,
        },
      );

      if (update) {
        await this.window.overlay.updateHandle(this.window.id, update);
      }
    } catch (e) {
      this.emitError(e);
    }
  }

  /**
   * Copy overlay texture from bitmap surface.
   */
  private async paintSoftware(
    _dirtyRect: Electron.Rectangle,
    image: NativeImage,
  ) {
    // TODO:: update only changed part
    try {
      const update = this.surface.updateBitmap(
        image.getSize().width,
        image.toBitmap(),
      );

      if (update) {
        await this.window.overlay.updateHandle(this.window.id, update);
      }
    } catch (e) {
      this.emitError(e);
    }
  }

  private emitError(e: unknown) {
    if (this.events.listenerCount('error') !== 0) {
      this.events.emit('error', e);
      return;
    }

    throw e;
  }
}

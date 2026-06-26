import type { NativeImage, OffscreenSharedTexture, WebContents, WebContentsPaintEventParams } from 'electron';
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
      const update = e.texture ? this.paintAccelerated(e.texture) : this.paintSoftware(rect, image);
      if (update) {
        this.window.overlay.updateHandle(this.window.id, update).catch((e: unknown) => {
          this.emitError(e);
        });
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
      return this.surface.updateShtex(
        info.codedSize.width,
        info.codedSize.height,
        info.handle.ntHandle,
        {
          dstX: rect.x,
          dstY: rect.y,
          src: rect,
        },
      );
    } catch (e) {
      this.emitError(e);
    } finally {
      texture.release();
    }

    return null;
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
    try {
      return this.surface.updateBitmap(
        image.getSize().width,
        image.toBitmap(),
      );
    } catch (e) {
      this.emitError(e);
    }

    return null;
  }

  private emitError(e: unknown) {
    if (this.events.listenerCount('error') !== 0) {
      this.events.emit('error', e);
      return;
    }

    throw e;
  }
}

import type { NativeImage, TextureInfo, WebContents, WebContentsPaintEventParams } from 'electron';
import type { OverlayWindow } from './index.js';
import EventEmitter from 'node:events';

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

  private constructor(
    private readonly window: OverlayWindow,
    private readonly contents: WebContents,
  ) {
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
    contents: WebContents,
  ): ElectronOverlaySurface {
    return new ElectronOverlaySurface({ ...window }, contents);
  }

  /**
   * Disconnect surface from Electron window and clear overlay surface.
   */
  async disconnect() {
    this.contents.off('paint', this.handler);
    await this.window.overlay.clearSurface(this.window.id);
  }

  /**
   * Copy overlay texture in gpu accelerated shared texture mode.
   */
  private async paintAccelerated(texture: TextureInfo) {
    const rect = texture.metadata.captureUpdateRect ?? texture.contentRect;

    // update only changed part
    try {
      await this.window.overlay.updateShtex(
        this.window.id,
        texture.codedSize.width,
        texture.codedSize.height,
        texture.sharedTextureHandle,
        {
          dstX: rect.x,
          dstY: rect.y,
          src: rect,
        },
      );
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
      await this.window.overlay.updateBitmap(
        this.window.id,
        image.getSize().width,
        image.toBitmap(),
      );
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

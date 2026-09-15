import type { NativeImage, OffscreenSharedTexture, WebContents, WebContentsPaintEventParams } from 'electron';
import EventEmitter from 'node:events';
import { OverlaySurface as CoreOverlaySurface } from '@asdf-overlay/core';
import type { OverlaySurface } from './index.js';

type Emitter = EventEmitter<{
  /**
   * An error has been occured while copying to overlay surface.
   */
  error: [e: unknown],
}>;

type PaintSubscriber = (
  texture: OffscreenSharedTexture | undefined,
  dirtyRect: Electron.Rectangle,
  image: NativeImage,
) => void;

const PUMPS = new WeakMap<WebContents, PaintPump>();

/**
 * The single `paint` listener shared by every surface connected to one `WebContents`.
 *
 * A paint hands out one `OffscreenSharedTexture` that has to be released exactly once,
 * and only a limited number of them can exist at a time. With a listener per surface
 * the first one to run releases the texture while the others still have to copy from
 * it, so every surface but one fails with `opening NT shared texture`. Fan a frame out
 * to all of them instead, and release once they are done with it.
 */
class PaintPump {
  private readonly subscribers = new Set<PaintSubscriber>();

  private readonly handler: (
    e: Electron.Event<WebContentsPaintEventParams>,
    dirtyRect: Electron.Rectangle,
    image: NativeImage,
  ) => void;

  private constructor(private readonly contents: WebContents) {
    this.handler = (e, rect, image) => {
      try {
        for (const subscriber of this.subscribers) {
          subscriber(e.texture, rect, image);
        }
      } finally {
        e.texture?.release();
      }
    };

    contents.on('paint', this.handler);
  }

  /**
   * Receive every frame of `contents` until the returned function is called.
   */
  static subscribe(contents: WebContents, subscriber: PaintSubscriber): () => void {
    let pump = PUMPS.get(contents);
    if (!pump) {
      pump = new PaintPump(contents);
      PUMPS.set(contents, pump);
    }

    const owner = pump;
    owner.subscribers.add(subscriber);
    return () => owner.unsubscribe(subscriber);
  }

  private unsubscribe(subscriber: PaintSubscriber) {
    if (!this.subscribers.delete(subscriber) || this.subscribers.size > 0) {
      return;
    }

    this.contents.off('paint', this.handler);
    PUMPS.delete(this.contents);
  }
}

/**
 * Connection from a Electron offscreen window to a overlay surface.
 */
export class ElectronOverlaySurface {
  /**
   * Events during paints.
   */
  readonly events: Emitter = new EventEmitter();

  private readonly unsubscribe: () => void;

  private readonly inner: CoreOverlaySurface;

  private constructor(
    private readonly surface: OverlaySurface,
    contents: WebContents,
  ) {
    this.inner = new CoreOverlaySurface(surface.info.gpuId);

    this.unsubscribe = PaintPump.subscribe(contents, (texture, rect, image) => {
      try {
        const update = texture ? this.paintAccelerated(texture) : this.paintSoftware(rect, image);

        if (update) {
          this.surface.overlay.updateHandle(this.surface.id, update)
            .catch((e: unknown) => this.events.emit('error', e));
        }
      } catch (err) {
        this.events.emit('error', err);
      }
    });

    contents.invalidate();
  }

  /**
   * Connect Electron `WebContents` surface to target overlay window.
   */
  static connect(
    surface: OverlaySurface,
    contents: WebContents,
  ): ElectronOverlaySurface {
    return new ElectronOverlaySurface({ ...surface }, contents);
  }

  /**
   * Disconnect surface from Electron window and clear overlay surface.
   */
  async disconnect() {
    this.unsubscribe();
    await this.surface.overlay.updateHandle(this.surface.id, { type: 'None' });
  }

  /**
   * Copy overlay texture in gpu accelerated shared texture mode.
   */
  private paintAccelerated(texture: OffscreenSharedTexture) {
    const info = texture.textureInfo;

    // TODO:: cross platform handle
    if (info.widgetType !== 'frame' || !info.handle.ntHandle) {
      return null;
    }
    const rect = info.metadata.captureUpdateRect ?? info.contentRect;

    // update only changed part
    // NOTE: the texture is released by `PaintPump`, once every surface has copied it.
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

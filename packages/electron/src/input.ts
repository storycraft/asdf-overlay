import { type MouseInputEvent, type MouseWheelInputEvent, type WebContents } from 'electron';
import type { OverlayWindow } from './index.js';
import type { CursorInput, CursorInputKind, KeyboardInput } from '@asdf-overlay/core';
import { mapCssCursor, mapKeycode } from './input/conv.js';
import { Cursor } from '@asdf-overlay/core';

/**
 * Connection from a overlay window to a Electron window.
 */
export class ElectronOverlayInput {
  private readonly cursorInputHandler: (id: number, input: CursorInput) => void;
  private readonly keyboardInputHandler: (id: number, input: KeyboardInput) => void;

  private readonly cursorChangedHandler: (e: Electron.Event, type: string) => void;

  private constructor(
    private readonly window: OverlayWindow,
    private readonly contents: WebContents,
  ) {
    this.window = { ...window };

    this.window.overlay.event.on(
      'cursor_input',
      this.cursorInputHandler = (id, input) => {
        if (id !== window.id) {
          return;
        }

        this.sendCursorInput(input);
      },
    );
    this.window.overlay.event.on(
      'keyboard_input',
      this.keyboardInputHandler = (id, input) => {
        if (id !== window.id) {
          return;
        }

        this.sendKeyboardInput(input);
      },
    );
    this.contents.on(
      'cursor-changed',
      this.cursorChangedHandler = (_, type) => {
        void this.window.overlay.setBlockingCursor(mapCssCursor(type));
      },
    );
  }

  /**
   * Connect overlay inputs to a Electron `WebContents`.
   */
  static connect(window: OverlayWindow, contents: WebContents): ElectronOverlayInput {
    return new ElectronOverlayInput({ ...window }, contents);
  }

  /**
   * Disconnect overlay inputs.
   */
  async disconnect() {
    this.window.overlay.event.off('cursor_input', this.cursorInputHandler);
    this.window.overlay.event.off('keyboard_input', this.keyboardInputHandler);
    this.contents.off('cursor-changed', this.cursorChangedHandler);

    try {
      await this.window.overlay.setBlockingCursor(Cursor.Default);
    } catch {
      //
    }
  }

  private readonly clickCounts: number[] = [];
  private processCursorAction(
    input_kind: CursorInputKind & { type: 'Action' },
    x: number,
    y: number,
    globalX: number,
    globalY: number,
    movementX: number,
    movementY: number,
  ) {
    let button: MouseInputEvent['button'];
    switch (input_kind.action) {
      case 'Left': {
        button = 'left';
        break;
      }
      case 'Middle': {
        button = 'middle';
        break;
      }
      case 'Right': {
        button = 'right';
        break;
      }
      case 'Forward': {
        this.contents.navigationHistory.goForward();
        return;
      }
      case 'Back': {
        this.contents.navigationHistory.goBack();
        return;
      }
    }

    if (input_kind.state.type === 'Pressed') {
      const clickCount = 1 + ~~input_kind.state.doubleClick;
      this.clickCounts.push(clickCount);
      this.contents.sendInputEvent({
        type: 'mouseDown',
        button,
        clickCount,
        x,
        y,
        globalX,
        globalY,
        movementX,
        movementY,
        modifiers: this.modifiers,
      });
    } else {
      const clickCount = this.clickCounts.pop() ?? 1;
      this.contents.sendInputEvent({
        type: 'mouseUp',
        button,
        clickCount,
        x,
        y,
        globalX,
        globalY,
        movementX,
        movementY,
        modifiers: this.modifiers,
      });
    }
  }

  private readonly lastWindowCursor = {
    x: 0,
    y: 0,
  };

  sendCursorInput(input: CursorInput) {
    const x = input.x;
    const y = input.y;
    const globalX = input.x;
    const globalY = input.y;

    const movementX = globalX - this.lastWindowCursor.x;
    const movementY = globalY - this.lastWindowCursor.y;

    switch (input.kind.type) {
      case 'Enter': {
        this.contents.sendInputEvent({
          type: 'mouseEnter',
          x,
          y,
          globalX,
          globalY,
          movementX,
          movementY,
          modifiers: this.modifiers,
        });
        break;
      }

      case 'Leave': {
        this.contents.sendInputEvent({
          type: 'mouseLeave',
          x,
          y,
          globalX,
          globalY,
          movementX,
          movementY,
          modifiers: this.modifiers,
        });
        break;
      }

      case 'Move': {
        this.contents.sendInputEvent({
          type: 'mouseMove',
          x,
          y,
          globalX,
          globalY,
          movementX,
          movementY,
          modifiers: this.modifiers,
        });
        break;
      }

      case 'Scroll': {
        let scroll: MouseWheelInputEvent;
        if (input.kind.axis === 'Y') {
          scroll = {
            type: 'mouseWheel',
            deltaY: input.kind.delta,
            x,
            y,
            globalX,
            globalY,
            movementX,
            movementY,
            modifiers: this.modifiers,
          };
        } else {
          scroll = {
            type: 'mouseWheel',
            deltaX: input.kind.delta,
            x,
            y,
            globalX,
            globalY,
            movementX,
            movementY,
            modifiers: this.modifiers,
          };
        }
        this.contents.sendInputEvent(scroll);
        break;
      }

      case 'Action': {
        this.processCursorAction(
          input.kind,
          x,
          y,
          globalX,
          globalY,
          movementX,
          movementY,
        );
        break;
      }
    }

    this.lastWindowCursor.x = globalX;
    this.lastWindowCursor.y = globalY;
  }

  private readonly modifiersMap = {
    shift: false,
    ctrl: false,
    alt: false,
    super: false,
    meta: false,
  };

  private modifiers: ('shift' | 'ctrl' | 'alt' | 'meta' | 'cmd')[] = [];
  private updateModifiers(key: string, downState: boolean) {
    switch (key) {
      case 'Control': {
        this.modifiersMap.ctrl = downState;
        break;
      }

      case 'Shift': {
        this.modifiersMap.shift = downState;
        break;
      }

      case 'Super': {
        this.modifiersMap.super = downState;
        break;
      }

      case 'Meta': {
        this.modifiersMap.meta = downState;
        break;
      }

      case 'Alt': {
        this.modifiersMap.alt = downState;
        break;
      }

      default: {
        return;
      }
    }

    this.modifiers = [];
    if (this.modifiersMap.shift) {
      this.modifiers.push('shift');
    }

    if (this.modifiersMap.ctrl) {
      this.modifiers.push('ctrl');
    }

    if (this.modifiersMap.alt) {
      this.modifiers.push('alt');
    }

    if (this.modifiersMap.meta) {
      this.modifiers.push('meta');
    }

    if (this.modifiersMap.super) {
      this.modifiers.push('cmd');
    }
  }

  sendKeyboardInput(input: KeyboardInput) {
    switch (input.type) {
      case 'Key': {
        const keyCode = mapKeycode(input.key.code);
        if (!keyCode) {
          return;
        }

        const pressed = input.state === 'Pressed';
        this.updateModifiers(keyCode, pressed);
        this.contents.sendInputEvent({
          type: pressed ? 'keyDown' : 'keyUp',
          keyCode,
          modifiers: this.modifiers,
        });
        return;
      }

      case 'Char': {
        this.contents.sendInputEvent({
          type: 'char',
          keyCode: input.ch,
          modifiers: this.modifiers,
        });
        return;
      }

      case 'Ime': {
        this.processIme(input);
        return;
      }
    }
  }

  private processIme(input: KeyboardInput & { type: 'Ime' }) {
    if (input.ime.type !== 'Commit') {
      return;
    }

    for (const ch of input.ime.text) {
      this.contents.sendInputEvent({
        type: 'char',
        keyCode: ch,
        modifiers: this.modifiers,
      });
    }
  }
}

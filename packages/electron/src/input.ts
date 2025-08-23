import type { MouseInputEvent, MouseWheelInputEvent, WebContents } from 'electron';
import type { OverlayWindow } from './index.js';
import type { CursorInput, KeyboardInput } from '@asdf-overlay/core/input';
import { mapCssCursor, mapKeycode } from './input/conv.js';
import { Cursor } from '@asdf-overlay/core';

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
        void this.window.overlay.setBlockingCursor(this.window.id, mapCssCursor(type));
      },
    );
  }

  static connect(window: OverlayWindow, contents: WebContents): ElectronOverlayInput {
    return new ElectronOverlayInput({ ...window }, contents);
  }

  async disconnect() {
    this.window.overlay.event.off('cursor_input', this.cursorInputHandler);
    this.window.overlay.event.off('keyboard_input', this.keyboardInputHandler);
    this.contents.off('cursor-changed', this.cursorChangedHandler);

    try {
      await this.window.overlay.setBlockingCursor(this.window.id, Cursor.Default);
    } catch {
      //
    }
  }

  private readonly clickCounts: number[] = [];
  private processCursorAction(
    input: CursorInput & { kind: 'Action', },
    movementX: number,
    movementY: number,
  ) {
    let button: MouseInputEvent['button'];
    switch (input.action) {
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

    if (input.state === 'Pressed') {
      const clickCount = 1 + ~~input.doubleClick;
      this.clickCounts.push(clickCount);
      this.contents.sendInputEvent({
        type: 'mouseDown',
        button,
        clickCount,
        x: input.clientX,
        y: input.clientY,
        globalX: input.windowX,
        globalY: input.windowY,
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
        x: input.clientX,
        y: input.clientY,
        globalX: input.windowX,
        globalY: input.windowY,
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
    const movementX = input.windowX - this.lastWindowCursor.x;
    const movementY = input.windowY - this.lastWindowCursor.y;

    switch (input.kind) {
      case 'Enter': {
        this.contents.sendInputEvent({
          type: 'mouseEnter',
          x: input.clientX,
          y: input.clientY,
          globalX: input.windowX,
          globalY: input.windowY,
          movementX,
          movementY,
          modifiers: this.modifiers,
        });
        break;
      }

      case 'Leave': {
        this.contents.sendInputEvent({
          type: 'mouseLeave',
          x: input.clientX,
          y: input.clientY,
          globalX: input.windowX,
          globalY: input.windowY,
          movementX,
          movementY,
          modifiers: this.modifiers,
        });
        break;
      }

      case 'Move': {
        this.contents.sendInputEvent({
          type: 'mouseMove',
          x: input.clientX,
          y: input.clientY,
          globalX: input.windowX,
          globalY: input.windowY,
          movementX,
          movementY,
          modifiers: this.modifiers,
        });
        break;
      }

      case 'Scroll': {
        let scroll: MouseWheelInputEvent;
        if (input.axis === 'Y') {
          scroll = {
            type: 'mouseWheel',
            deltaY: input.delta,
            x: input.clientX,
            y: input.clientY,
            globalX: input.windowX,
            globalY: input.windowY,
            movementX,
            movementY,
            modifiers: this.modifiers,
          };
        } else {
          scroll = {
            type: 'mouseWheel',
            deltaX: input.delta,
            x: input.clientX,
            y: input.clientY,
            globalX: input.windowX,
            globalY: input.windowY,
            movementX,
            movementY,
            modifiers: this.modifiers,
          };
        }
        this.contents.sendInputEvent(scroll);
        break;
      }

      case 'Action': {
        this.processCursorAction(input, movementX, movementY);
        break;
      }
    }

    this.lastWindowCursor.x = input.windowX;
    this.lastWindowCursor.y = input.windowY;
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
    switch (input.kind) {
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

  private processIme(input: KeyboardInput & { kind: 'Ime', }) {
    if (input.ime.kind !== 'Commit') {
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

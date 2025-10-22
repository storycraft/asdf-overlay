# Input Control
This section provides a guide on how to listen to input events and control input passthrough to overlay window.

## Listening to input events
To listen to input events going to the overlay window, you can use `listenInput` method from the `Overlay` instance.
This method allows you to listen cursor and keyboard events using event listeners.

Example:
```typescript
import { Overlay } from '@asdf-overlay/core';

const overlay: Overlay = /* Attached Overlay instance */;
const windowId: number = /* Target window ID */;

await overlay.listenInput(
  windowId,
  true, /* listen to cursor events */
  true, /* listen to keyboard events */
);

// Register listener for cursor events
overlay.on('cursor_input', (windowId, event) => {
  // Cursor event listener 
});

// Register listener for keyboard events
overlay.on('keyboard_input', (windowId, event) => {
  // Keyboard event listener 
});
```
Caveats:
1. Raw input will not be captured as you can register platform specific raw input listeners.
2. Listening to unnecessary input events may cause performance issues.
3. IME input events are provided as best-effort basis and may not work as expected in all cases.

## Controlling input passthrough
Sometimes you may want to control whether input events are passed through to the underlying window or not.
For example, you may want to show interactive overlay UI and you don't want input events to reach the underlying window.

By using `blockInput` method from the `Overlay` instance, you can block or unblock input events reaching the underlying window.

Example:
```typescript
import { Overlay } from '@asdf-overlay/core';

const overlay: Overlay = /* Attached Overlay instance */;
const windowId: number = /* Target window ID */;

await overlay.blockInput(
  windowId,
  block, // true to block input, false to unblock input
);

overlay.event.on('input_blocking_ended', () => {
  // Event listener called when input blocking ends
});
```
Caveats:
1. All input events will be captured, regardless of whether you are listening to them or not.
2. Raw input will be blocked.
3. User can interrupt input blocking by pressing `Alt + F4` shortcut.
   Always listen to `input_blocking_ended` event to handle such cases.

## Electron input redirection
When using `@asdf-overlay/electron` package, utility for input redirection is provided.
By using `ElectronOverlayInput`, you can easily redirect captured input events to Electron's `BrowserWindow`.

Example:
```typescript
import { Overlay } from '@asdf-overlay/core';
import { ElectronOverlayInput, type OverlayWindow } from '@asdf-overlay/electron';
import { BrowserWindow } from 'electron';

const overlay: Overlay = /* Attached Overlay instance */;
const windowId: number = /* Target window ID */;
const browserWindow: BrowserWindow = /* Target Electron BrowserWindow */;
const window: OverlayWindow = { id: windowId, overlay };

// Connect overlay input to Electron BrowserWindow.
const overlayInput = ElectronOverlayInput.connect(window, browserWindow.webContents);

// Control which input events to listen and redirect.
overlay.listenInput(windowId, true, true);

// Disconnect when no longer needed.
await overlayInput.disconnect();
```
Caveats:
1. Due to limitations of Electron, some input events may not be redirected properly.
2. If `browserWindow` is not focused, some input events may not be redirected properly.


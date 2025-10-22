# Obtaining Window Information
Before showing overlay, you need to specify which window to show overlay.
In most case, you need to obtain main window information of target application.

This heuristic code can be used to obtain main window information.
```typescript
const [id, luid] = await new Promise<[number, GpuLuid]>(resolve => overlay.event.once(
  'added',
  (id, _width, _height, luid) => {
    resolve([id, luid]);
  }),
);
```
It will return first found window ID and its associated GPU LUID.

## Closing
After opening a connection to Asdf Overlay, it is important to close the connection properly to free up resources.
The `Overlay` instance provides a method to close the connection and unload the overlay dll from the target process.

Following code demonstrates how to close the overlay connection.
```typescript
import { Overlay } from '@asdf-overlay/core';

const overlay: Overlay = /* previously attached Overlay instance */;
overlay.destroy();
```
Some caveats included below:
1. After calling the `destroy` method, the overlay setup in the target process will be reset, and the resources will be released.
2. You cannot use the `Overlay` instance after calling this method.
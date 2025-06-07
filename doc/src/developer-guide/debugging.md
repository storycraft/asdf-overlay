# Debugging
By default, release build will not emit any logs due to performance overhead.
To debug overlay DLL, you need to build with debug profile. 

Build overlay DLL with debug profile, following command can be used
```bash
pnpm build-dll
``` 

After building, replace overlay DLL.
Overlay DLL with debug profile will emit tracing to Debug Output Window.
Use external debug log viewer (ex: [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)) to see debug output.

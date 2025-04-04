// This module loads the platform-specific build of the addon on
// the current system. The supported platforms are registered in
// the `platforms` object below, whose entries can be managed by
// by the Neon CLI:
//
//   https://www.npmjs.com/package/@neon-rs/cli

export const lib = require('@neon-rs/load').proxy({
  platforms: {
    'win32-x64-msvc': () => require('@asdf-overlay-node/win32-x64-msvc'),
    'win32-ia32-msvc': () => require('@asdf-overlay-node/win32-ia32-msvc'),
  },
  debug: () => require('../index.node')
});

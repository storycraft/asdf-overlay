import { defineConfig } from 'electron-vite';

export default defineConfig({
  main: {
    build: {
      lib: {
        entry: './main/index.ts',
      },
      rollupOptions: {
        external: [
          'asdf-overlay-node',
        ],
      },
    }
  }
});

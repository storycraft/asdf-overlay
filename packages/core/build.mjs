// @ts-check
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { generateTypeDef, writeJsBinding } from '@napi-rs/cli';

import pkg from './package.json' with { type: 'json' };
import { spawn } from 'node:child_process';

const typeDefDir = join(import.meta.dirname, '..', '..', 'target', 'napi-rs', 'asdf-overlay-node');

// 1. Build native addons
await exec(
  'cargo',
  ['xtask', 'build-node', '--', ...process.argv.slice(2)],
  {}
);

// 2. Clean up the type definition directory before building
await rm(typeDefDir, { recursive: true, force: true });
await mkdir(typeDefDir, { recursive: true });

// 3. Generate intermediate type definition files
await exec(
  'cargo',
  ['check', '--target', pkg.napi.targets[0], ...process.argv.slice(2)],
  {
    NAPI_TYPE_DEF_TMP_FOLDER: typeDefDir
  }
);

// 4. Generate dts
const { dts, exports } = await generateTypeDef({
  typeDefDir,
  constEnum: false,
  dtsHeaderFile: './types.d.ts',
  cwd: import.meta.dirname,
});
await writeFile(join(import.meta.dirname, 'index.d.ts'), dts);

// 5. Write JS binding
await writeJsBinding({
  jsBinding: 'index.js',
  platform: true,
  binaryName: pkg.napi.binaryName,
  packageName: pkg.name,
  version: pkg.version,
  outputDir: import.meta.dirname,
  idents: exports,
});

/**
 * @param {string} command 
 * @param {string[]} args
 * @param {Record<string, string>} env
 */
async function exec(
  command,
  args,
  env,
) {
  const childProcess = spawn(
    command,
    args,
    {
      stdio: 'inherit',
      env: {
        ...process.env,
        ...env,
      }
    }
  );

  await new Promise((resolve, reject) => {
    childProcess.on('error', reject);
    childProcess.on('close', (code) => {
      if (code === 0) {
        resolve(true)
      } else {
        reject(new Error(`Process failed with code: ${code}`));
      }
    });
  });
}
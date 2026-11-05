import { resolve } from 'node:path';
import { defineConfig } from 'electron-vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { localeDevelopmentPlugin } from './scripts/locale-dev-plugin.mjs';
import { rustDevelopmentPlugins } from './scripts/rust-dev-plugin.mjs';
export default defineConfig(({ command, mode }) => {
  const channelDefinitions = {
    'import.meta.env.VIGILSHELL_DEV_PACKAGE_BUILD': JSON.stringify(
      mode === 'development-package' ? '1' : '0',
    ),
    'import.meta.env.VIGILSHELL_DEVELOPMENT_BUILD': JSON.stringify(command === 'serve' ? '1' : '0'),
  };
  const rustDevelopment =
    command === 'serve' && process.env.VIGILSHELL_DEV_APP === '1'
      ? rustDevelopmentPlugins(resolve('.'))
      : undefined;
  return {
    main: {
      define: {
        ...channelDefinitions,
        'process.env.VIGILSHELL_DEV_PACKAGE_BUILD':
          channelDefinitions['import.meta.env.VIGILSHELL_DEV_PACKAGE_BUILD'],
        'process.env.VIGILSHELL_DEVELOPMENT_BUILD':
          channelDefinitions['import.meta.env.VIGILSHELL_DEVELOPMENT_BUILD'],
      },
      plugins: rustDevelopment ? [rustDevelopment.main] : [],
      build: { externalizeDeps: true },
    },
    preload: {
      build: {
        externalizeDeps: true,
        rollupOptions: {
          output: { format: 'cjs', entryFileNames: '[name].cjs' },
        },
      },
    },
    renderer: {
      define: channelDefinitions,
      server: { host: '127.0.0.1' },
      resolve: { alias: { '@': resolve('src/renderer') } },
      plugins: [
        ...(command === 'serve' ? [localeDevelopmentPlugin(resolve('.'))] : []),
        ...(rustDevelopment ? [rustDevelopment.renderer] : []),
        react(),
        tailwindcss(),
      ],
      build: {
        assetsInlineLimit: 0,
        minify: true,
        reportCompressedSize: false,
        rollupOptions: { input: resolve('src/renderer/index.html') },
      },
    },
  };
});

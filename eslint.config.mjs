import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import { defineConfig } from 'eslint/config';

const mainBrowserGlobals = [
  ...new Set([
    ...Object.keys(globals.browser).filter((name) => !(name in globals.node)),
    'navigator',
    'localStorage',
    'sessionStorage',
  ]),
];

export default defineConfig(
  {
    ignores: [
      'node_modules/**',
      '.cache/**',
      '.specs/**/dashboard/**',
      'target/**',
      'out/**',
      'dist/**',
      'build/**',
      'release/**',
      'apps/desktop/out/**',
      'apps/desktop/node_modules/**',
      '.agents/**',
      'src/contracts/generated/**',
      'src/shared/i18n/generated/**',
      'test-results/**',
      'playwright-report/**',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: [
      'src/main/**/*.ts',
      'src/preload/**/*.ts',
      'scripts/**/*.mjs',
      '*.config.{ts,mjs}',
      'tests/**/*.{ts,tsx,mjs}',
      'src/**/*.test.{ts,tsx}',
    ],
    languageOptions: { globals: globals.node },
  },
  {
    files: ['src/renderer/**/*.{ts,tsx}', 'src/preload/**/*.ts', 'tests/**/*.{ts,tsx,mjs}'],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ['src/main/**/*.ts'],
    rules: {
      'no-restricted-globals': [
        'error',
        {
          globals: mainBrowserGlobals.map((name) => ({
            name,
            message: 'Main 的 DOM 类型仅用于 Electron 声明；浏览器运行时操作属于 Renderer。',
          })),
          checkGlobalObject: true,
          globalObjects: ['globalThis', 'global'],
        },
      ],
    },
  },
  {
    files: ['src/renderer/**/*.{ts,tsx}'],
    // 只注册现用规则，避免把插件嵌套的旧式预设当作 ESLint 10 配置输入。
    plugins: { 'react-hooks': { meta: reactHooks.meta, rules: reactHooks.rules } },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'error',
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['electron', 'node:*', '**/main/**', '**/preload/**'],
              message: 'Renderer 只能通过 DesktopApi 使用桌面能力。',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['src/main/**/*.ts', 'src/preload/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            { group: ['react', 'react-dom', '**/renderer/**'], message: '桌面层不依赖 Renderer。' },
          ],
        },
      ],
    },
  },
);

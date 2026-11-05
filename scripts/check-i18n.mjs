import { readFileSync } from 'node:fs';
const en = JSON.parse(readFileSync('messages/en.json', 'utf8'));
const zh = JSON.parse(readFileSync('messages/zh-CN.json', 'utf8'));
if (JSON.stringify(Object.keys(en).sort()) !== JSON.stringify(Object.keys(zh).sort()))
  throw Error('Locale keys differ');
for (const key of Object.keys(en)) {
  const parameters = (value) =>
    [...value.matchAll(/\{([^}]+)\}/g)]
      .map((m) => m[1])
      .sort()
      .join(',');
  if (!zh[key] || parameters(en[key]) !== parameters(zh[key]))
    throw Error('Invalid translation: ' + key);
}
console.log(`Locales: ${Object.keys(en).length} matching messages`);

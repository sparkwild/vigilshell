import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';

const dataDirectory = process.argv[process.argv.indexOf('--data-dir') + 1];
const scenario = JSON.parse(readFileSync(join(dataDirectory, 'scenario.json'), 'utf8'));
const statePath = join(dataDirectory, 'state.json');
let preferences = {
  systemNotificationMode: 'unfocused',
  attentionRequiredNotificationEnabled: true,
  executionIssueNotificationEnabled: true,
  autoCheckUpdates: true,
  themeMode: 'system',
  localeMode: 'system',
  revision: 0,
  terminalFontSize: 12,
  terminalLineHeight: 14,
  terminalScrollback: 10000,
  terminalCursorStyle: 'block',
  terminalCursorBlink: true,
  connectionOpenBehavior: 'ask',
  filesShowHidden: true,
  filesShowTree: true,
  filesColumns: ['name', 'size', 'kind', 'modified', 'permissions', 'owner'],
  downloadMode: 'ask',
  downloadDirectory: '',
  monitorDisplay: 'bar',
  monitorSampleSeconds: 5,
  monitorFullSeconds: 60,
  terminalCopyOnSelect: false,
  terminalThemeMode: 'inherit',
  terminalFontFamily: 'jetbrains-mono',
  sidebarHoverPreview: true,
  filesAutoOpen: true,
  remoteDirectoryMode: 'initial',
  fileEditorApplication: '',
};
try {
  preferences = JSON.parse(readFileSync(statePath, 'utf8'));
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
writeFileSync(join(dataDirectory, 'pid'), String(process.pid));
const input = createInterface({ input: process.stdin });

function respond(request, result) {
  const response = { jsonrpc: '2.0', id: request.id, result };
  if (scenario.mode === 'malformed-json') {
    process.stdout.write('{broken-json}\n');
  } else if (scenario.mode === 'invalid-schema') {
    process.stdout.write(`${JSON.stringify({ ...response, extra: 'not in schema' })}\n`);
  } else if (scenario.mode === 'wrong-request') {
    process.stdout.write(`${JSON.stringify({ ...response, id: 'unknown-request' })}\n`);
  } else if (scenario.mode === 'oversized-frame') {
    process.stdout.write(`${'x'.repeat(65_537)}\n`);
  } else if (scenario.mode === 'oversized-partial') {
    process.stdout.write('x'.repeat(65_537));
  } else if (scenario.mode === 'split-utf8') {
    const frame = Buffer.from(`${JSON.stringify(response)}\n`);
    const boundary = frame.indexOf(Buffer.from('初')) + 1;
    process.stdout.write(frame.subarray(0, boundary));
    setImmediate(() => process.stdout.write(frame.subarray(boundary)));
  } else {
    process.stdout.write(`${JSON.stringify(response)}\n`);
  }
}

input.on('line', (line) => {
  const request = JSON.parse(line);
  appendFileSync(join(dataDirectory, 'requests.jsonl'), `${line}\n`);
  if (request.method === 'runtime.hello') {
    respond(request, {
      kind: 'ready',
      application: 'vigilshell',
      runtimeId: 'runtime-初始',
      contractVersion:
        scenario.mode === 'unsupported-version' ? 3 : scenario.desktopProtocolVersion,
    });
  } else if (
    ['unlock-delayed', 'negotiation-delayed'].includes(scenario.mode) &&
    request.method === 'workspace.negotiate'
  ) {
    const send = () =>
      respond(request, {
        kind: 'workspace',
        result: {
          kind: 'ready',
          version: scenario.workspaceProtocolVersion,
          connectionAccess: { phase: 'unlocking', revision: 1, code: null },
        },
      });
    if (scenario.mode === 'negotiation-delayed') setTimeout(send, 650);
    else send();
  } else if (scenario.mode === 'unlock-delayed' && request.method === 'connections.unlock') {
    respond(request, {
      kind: 'workspace',
      result: { kind: 'connectionAccess', state: { phase: 'unlocking', revision: 1, code: null } },
    });
    setTimeout(
      () =>
        process.stdout.write(
          JSON.stringify({
            jsonrpc: '2.0',
            method: 'workspace.changed',
            params: {
              runtimeId: 'runtime-初始',
              owner: request.owner,
              event: {
                kind: 'connectionAccess',
                state: { phase: 'ready', revision: 2, code: null },
              },
            },
          }) + '\n',
        ),
      60,
    );
  } else if (request.method === 'agent.request' && scenario.mode === 'large-agent') {
    const large = 'x'.repeat(70000);
    respond(request, {
      kind: 'workspace',
      result: {
        kind: 'agent',
        result: {
          kind: 'output',
          output: {
            screen: null,
            chunks: [{ sequence: 1, stream: 'stdout', text: large }],
            nextCursor: 1,
            incomplete: false,
          },
        },
      },
    });
    process.stdout.write(
      JSON.stringify({
        jsonrpc: '2.0',
        method: 'agent.changed',
        params: {
          runtimeId: 'runtime-初始',
          event: {
            kind: 'context',
            context: {
              id: 's',
              entryId: 'e',
              entryName: 'Fixture',
              connectionId: 'c',
              connectionName: 'Fixture',
              connectionRevision: 1,
              phase: 'ready',
              cwd: '/',
              environment: { fixture: large },
              revision: 0,
              createdAt: 1,
              error: null,
              support: null,
            },
          },
        },
      }) + '\n',
    );
  } else if (request.method === 'preferences.update') {
    preferences = {
      ...preferences,
      ...request.params.patch,
      revision: preferences.revision + 1,
    };
    writeFileSync(statePath, JSON.stringify(preferences));
    if (scenario.mode === 'changed-instance')
      process.stdout.write(
        JSON.stringify({
          jsonrpc: '2.0',
          method: 'preferences.changed',
          params: { runtimeId: 'other', preferences },
        }) + '\n',
      );
    if (scenario.mode !== 'timeout-after-commit') {
      respond(request, { kind: 'updated', outcome: 'applied', preferences });
    }
  } else if (request.method === 'preferences.get') {
    respond(request, { kind: 'preferences', preferences });
  } else if (request.method === 'runtime.shutdown' && scenario.mode !== 'ignore-shutdown') {
    respond(request, { kind: 'stopped' });
    input.close();
    process.stdin.destroy();
  }
});
input.on('close', () => process.stdin.destroy());

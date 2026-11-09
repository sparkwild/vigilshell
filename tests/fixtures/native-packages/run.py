"""Use only named no-mount disposable containers, never saved connections or Dev data."""
import argparse
import json
import os
import pathlib
import subprocess


def exchange(helper, line):
    helper.stdin.write(line)
    helper.stdin.flush()
    response = helper.stdout.readline()
    assert response, 'host bridge closed'
    return response


def verify_backend(helper, expected):
    task = json.loads(exchange(helper, json.dumps({'operation': {'kind': 'packages_list'}}) + '\n'))
    assert isinstance(task, dict), 'invalid package backend probe reply'
    report = task.get('nativeReport') or {}
    completed = {'kind': 'completed', 'exitCode': 0}
    assert (
        isinstance(report, dict)
        and (task.get('state') or {}).get('outcome') == completed
        and report.get('outcome') == completed
        and not task.get('error')
        and not report.get('error')
    ), 'package backend probe did not complete successfully'
    assert isinstance(report.get('json'), str), 'package backend probe result missing'
    result = json.loads(report['json'])
    actual = result.get('backend') if isinstance(result, dict) else None
    assert actual == expected, f'Expected native backend {expected}, got {actual}'
    print(f'PASS native backend {actual} before scenario startup', flush=True)


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('backend', choices=['apt', 'dnf4', 'dnf5'])
    parser.add_argument('scenario')
    parser.add_argument('--container', required=True)
    args = parser.parse_args(argv)
    root = pathlib.Path(__file__).resolve().parents[3]
    name = args.container
    info = json.loads(subprocess.check_output(['docker', 'inspect', name]))[0]
    assert info['Config']['Labels'].get('vigilshell.acceptance') == 'native-packages' and not info['Mounts']
    binding = info['NetworkSettings']['Ports']['22/tcp'][0]
    assert binding['HostIp'] == '127.0.0.1'
    env = {
        **os.environ,
        'VIGILSHELL_TEST_NATIVE_PORT': binding['HostPort'],
        'VIGILSHELL_TEST_NATIVE_USER': 'fixture' if args.scenario == 'permission' else 'root',
    }
    helper = subprocess.Popen(
        [str(root / 'target/debug/examples/native_fixture')],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env,
    )
    guest = None
    try:
        # 场景模块在导入时就可能修改来源或构建软件包，须先核对实际选中的后端。
        verify_backend(helper, args.backend)
        subprocess.run(['docker', 'exec', name, 'mkdir', '-p', '/native-fixtures'], check=True)
        for path in pathlib.Path(__file__).parent.glob('*.py'):
            if path.name != 'run.py':
                subprocess.run(['docker', 'cp', str(path), name + ':/native-fixtures/' + path.name], check=True)
        guest = subprocess.Popen(
            ['docker', 'exec', '-i', name, 'python3', '-u', '/native-fixtures/' + args.scenario + '.py'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        for line in guest.stdout:
            if line.startswith('VIGIL_REQUEST '):
                request_line = line[len('VIGIL_REQUEST '):]
                response = exchange(helper, request_line)
                value = json.loads(response)
                request = json.loads(request_line)
                print(request.get('operation', {}).get('kind', 'environment' if request.get('environment') else 'unknown'), value.get('error'), value.get('state'), flush=True)
                guest.stdin.write(response)
                guest.stdin.flush()
            else:
                print(line, end='', flush=True)
        assert guest.wait() == 0, 'guest scenario failed'
    finally:
        if guest is not None and guest.poll() is None:
            guest.terminate()
            guest.wait()
        helper.stdin.close()
        assert helper.wait(timeout=30) == 0, 'host bridge failed'


if __name__ == '__main__':
    main()

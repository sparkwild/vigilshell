"""Exercise fixture requests and startup order without Docker or application data."""
import contextlib
import importlib.util
import io
import json
import pathlib
import unittest
from unittest.mock import Mock, patch


ROOT = pathlib.Path(__file__).resolve().parents[2]


def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


packages = load('package_protocol', 'tests/fixtures/native-packages/protocol.py')
services = load('service_protocol', 'tests/fixtures/native-services/protocol.py')
runner = load('package_runner', 'tests/fixtures/native-packages/run.py')


def receipt(backend='apt', outcome=None, error=None):
    outcome = outcome or {'kind': 'completed', 'exitCode': 0}
    return {
        'id': 'task',
        'state': {'outcome': outcome},
        'error': error,
        'nativeReport': {
            'outcome': outcome,
            'error': error,
            'json': json.dumps({'backend': backend}),
        },
    }


class OperationTests(unittest.TestCase):
    def test_task_timeout_is_sent_before_finish_without_changing_the_receipt(self):
        original = receipt()
        output = io.StringIO()
        with patch.object(packages.sys, 'stdin', io.StringIO(json.dumps(original) + '\n')):
            with contextlib.redirect_stdout(output):
                operation = packages.Operation({'kind': 'packages_list'}, timeout=8)
                self.assertEqual(len(output.getvalue().splitlines()), 1)
                self.assertIs(operation.finish(), operation)
        request = json.loads(output.getvalue().removeprefix('VIGIL_REQUEST '))
        self.assertEqual(request, {'operation': {'kind': 'packages_list'}, 'timeout': 8})
        self.assertEqual(operation.task, original)
        self.assertEqual(operation.result(), {'backend': 'apt'})

    def test_failed_and_unknown_reports_stay_failed_without_resending(self):
        for outcome, error in [
            ({'kind': 'completed', 'exitCode': 1}, 'native_package_operation_failed'),
            ({'kind': 'unknown', 'reason': 'timedOut'}, None),
            ({'kind': 'unknown', 'reason': 'disconnected'}, 'native_result_unknown'),
        ]:
            with self.subTest(outcome=outcome):
                original = receipt(outcome=outcome, error=error)
                output = io.StringIO()
                with patch.object(packages.sys, 'stdin', io.StringIO(json.dumps(original) + '\n')):
                    with contextlib.redirect_stdout(output):
                        operation = packages.Operation({'kind': 'packages_install'})
                        with self.assertRaises(AssertionError):
                            operation.finish()
                        operation.finish(False)
                self.assertEqual(len(output.getvalue().splitlines()), 1)
                self.assertEqual(operation.task, original)

    def test_service_request_acceptance_keeps_its_distinct_success_rule(self):
        accepted = receipt(outcome={'kind': 'requestAccepted'})
        for module, success in [(packages, False), (services, True)]:
            with self.subTest(module=module.__name__):
                with patch.object(module.sys, 'stdin', io.StringIO(json.dumps(accepted) + '\n')):
                    with contextlib.redirect_stdout(io.StringIO()):
                        module.Operation({'kind': 'services_control'}).finish(success)


class RunnerTests(unittest.TestCase):
    def run_fixture(self, expected, reply, scenario='smoke', scenario_reply=None, guest_error=None):
        response = reply if isinstance(reply, str) else json.dumps(reply) + '\n'
        responses = response + json.dumps(scenario_reply or receipt()) + '\n' if response else ''
        helper = Mock(stdin=Mock(), stdout=io.StringIO(responses))
        helper.wait.return_value = 0
        guest = Mock(stdin=Mock(), stdout=io.StringIO(
            'VIGIL_REQUEST {"operation":{"kind":"packages_list"},"timeout":8}\nPASS scenario\n'))
        guest.wait.return_value = 0
        guest.poll.return_value = 0
        info = {
            'Config': {'Labels': {'vigilshell.acceptance': 'native-packages'}},
            'Mounts': [],
            'NetworkSettings': {'Ports': {'22/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '2222'}]}},
        }

        def start(command, **options):
            if command[0] == 'docker':
                self.assertGreater(helper.stdout.tell(), 0)
                if guest_error:
                    raise guest_error
                return guest
            return helper

        with patch.object(runner.subprocess, 'check_output', return_value=json.dumps([info]).encode()):
            with patch.object(runner.subprocess, 'Popen', side_effect=start) as spawn:
                with patch.object(runner.subprocess, 'run') as mutate:
                    with contextlib.redirect_stdout(io.StringIO()):
                        error = None
                        try:
                            runner.main([expected, scenario, '--container', 'named-fixture'])
                        except Exception as caught:
                            error = caught
        return error, helper, guest, spawn, mutate

    def test_matching_backends_start_the_scenario_and_preserve_request_options(self):
        for backend in ['apt', 'dnf4', 'dnf5']:
            with self.subTest(backend=backend):
                error, helper, guest, spawn, mutate = self.run_fixture(backend, receipt(backend))
                self.assertIsNone(error)
                self.assertEqual(spawn.call_count, 2)
                self.assertTrue(mutate.called)
                requests = [json.loads(call.args[0]) for call in helper.stdin.write.call_args_list]
                self.assertEqual(requests, [
                    {'operation': {'kind': 'packages_list'}},
                    {'operation': {'kind': 'packages_list'}, 'timeout': 8},
                ])
                guest.stdin.write.assert_called_once()
                helper.stdin.close.assert_called_once()
                helper.wait.assert_called_once_with(timeout=30)

    def test_installed_dnf5_does_not_mask_an_actually_selected_dnf4(self):
        reply = receipt('dnf4')
        reply['nativeReport']['json'] = json.dumps({
            'backend': 'dnf4',
            'packageManagers': [{'kind': 'dnf4', 'available': True}, {'kind': 'dnf5', 'available': True}],
        })
        error, helper, _, spawn, mutate = self.run_fixture('dnf5', reply)
        self.assertIsInstance(error, AssertionError)
        self.assertIn('Expected native backend dnf5, got dnf4', str(error))
        self.assertEqual(spawn.call_count, 1)
        mutate.assert_not_called()
        helper.stdin.close.assert_called_once()
        helper.wait.assert_called_once_with(timeout=30)

    def test_missing_failed_unknown_and_malformed_probes_stop_before_guest_changes(self):
        missing = receipt()
        missing['nativeReport']['json'] = '{}'
        unknown_task = receipt()
        unknown_task['state']['outcome'] = {'kind': 'unknown', 'reason': 'timedOut'}
        for reply in [
            receipt(None), missing, unknown_task,
            receipt(error='native_package_query_failed'),
            receipt(outcome={'kind': 'unknown', 'reason': 'disconnected'}),
            '{broken-json}\n', '',
        ]:
            with self.subTest(reply=reply):
                error, helper, _, spawn, mutate = self.run_fixture('apt', reply)
                self.assertIsInstance(error, (AssertionError, json.JSONDecodeError))
                self.assertEqual(spawn.call_count, 1)
                mutate.assert_not_called()
                helper.stdin.close.assert_called_once()
                helper.wait.assert_called_once_with(timeout=30)

    def test_deliberately_missing_manager_reply_is_relayed_after_initial_probe(self):
        reply = receipt(None)
        error, helper, guest, _, _ = self.run_fixture('apt', receipt('apt'), 'query_without_manager', reply)
        self.assertIsNone(error)
        self.assertEqual(json.loads(guest.stdin.write.call_args.args[0]), reply)
        self.assertEqual(len(helper.stdin.write.call_args_list), 2)

    def test_failed_guest_start_still_closes_the_initialized_bridge(self):
        failure = OSError('guest could not start')
        error, helper, _, _, _ = self.run_fixture('apt', receipt('apt'), guest_error=failure)
        self.assertIs(error, failure)
        helper.stdin.close.assert_called_once()
        helper.wait.assert_called_once_with(timeout=30)

    def test_unprivileged_scenario_uses_the_same_account_for_probe_and_execution(self):
        error, _, _, spawn, _ = self.run_fixture('apt', receipt('apt'), 'permission')
        self.assertIsNone(error)
        self.assertEqual(spawn.call_args_list[0].kwargs['env']['VIGILSHELL_TEST_NATIVE_USER'], 'fixture')


if __name__ == '__main__':
    unittest.main()

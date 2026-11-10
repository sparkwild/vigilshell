import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location('licenses', pathlib.Path(__file__).parents[2] / 'scripts/licenses.py')
licenses = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licenses)


class NpmLicenseTests(unittest.TestCase):
    def test_runtime_and_bundled_roots_follow_nested_optional_and_peer_dependencies(self):
        packages = {
            '': {'dependencies': {'runtime': '1'}, 'devDependencies': {'ui': '1', 'builder': '1'}},
            'node_modules/runtime': {'dependencies': {'shared': '1'}},
            'node_modules/shared': {},
            'node_modules/ui': {
                'dev': True,
                'dependencies': {'shared': '2'},
                'optionalDependencies': {'absent': '1', 'present': '1'},
                'peerDependencies': {'peer': '1', 'optional-peer': '1'},
                'peerDependenciesMeta': {'optional-peer': {'optional': True}},
            },
            'node_modules/ui/node_modules/shared': {'dev': True},
            'node_modules/present': {'dev': True},
            'node_modules/peer': {'dev': True, 'dependencies': {'ui': '1'}},
            'node_modules/builder': {'dev': True},
        }
        self.assertEqual(licenses.npm_license_paths(packages, ['ui']), {
            'node_modules/runtime', 'node_modules/shared', 'node_modules/ui',
            'node_modules/ui/node_modules/shared', 'node_modules/present', 'node_modules/peer',
        })

    def test_missing_required_dependency_fails(self):
        with self.assertRaisesRegex(RuntimeError, 'Missing locked dependency'):
            licenses.npm_license_paths({'': {'dependencies': {'missing': '1'}}}, [])

    def test_stylesheet_license_does_not_include_unshipped_cli_dependencies(self):
        packages = {
            '': {'devDependencies': {'styles': '1'}},
            'node_modules/styles': {'dev': True, 'dependencies': {'cli': '1'}},
            'node_modules/cli': {'dev': True},
        }
        self.assertEqual(licenses.npm_license_paths(packages, [], ['styles']), {'node_modules/styles'})

    def test_undeclared_bundled_root_fails(self):
        with self.assertRaisesRegex(RuntimeError, 'Undeclared bundled dependency'):
            licenses.npm_license_paths({'': {}, 'node_modules/phantom': {}}, ['phantom'])


if __name__ == '__main__':
    unittest.main()

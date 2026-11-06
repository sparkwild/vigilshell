import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch
spec = importlib.util.spec_from_file_location('signing', Path(__file__).parents[2]/'scripts/configure-release-signing.py')
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)
class StorageTests(unittest.TestCase):
    def response(self, message, code=1):
        return subprocess.CompletedProcess([], code, json.dumps({'message':message}).encode(), b'')
    def test_environment_supported(self):
        with patch.object(signing.subprocess,'run',return_value=self.response('',0)):
            self.assertIsNotNone(signing.api('environment','PUT',{}))
    def test_explicit_plan_restriction_falls_back(self):
        with patch.object(signing.subprocess,'run',return_value=self.response('Upgrade to GitHub Pro or make this repository public to enable this feature.')):
            self.assertIsNone(signing.api('environment','PUT',{}))
    def test_other_failures_never_downgrade(self):
        for message in ['Not Found','Resource not accessible by integration','Bad credentials','API rate limit exceeded']:
            with self.subTest(message=message),patch.object(signing.subprocess,'run',return_value=self.response(message)):
                with self.assertRaises(RuntimeError): signing.api('environment','PUT',{})
    def test_get_failure_never_downgrades(self):
        with patch.object(signing.subprocess,'run',return_value=self.response('Upgrade your plan to enable environments')):
            with self.assertRaises(RuntimeError): signing.api('environment')
if __name__=='__main__':unittest.main()

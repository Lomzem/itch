"""Exercise the credential policy through its command-line input boundary."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import yaml

CHECKER = Path(__file__).resolve().parents[1] / 'check-workflow-checkouts.py'


class CheckoutPolicy(unittest.TestCase):
    def test_credentials_policy_is_independent_of_upload_order_and_ignores(self):
        upload = {'uses': 'actions/upload-artifact@v7', 'with': {'path': '.'}}
        for value in [None, True, '${{ false }}', 'off', 0, False, 'false']:
            for after_upload in [False, True]:
                with self.subTest(value=value, after_upload=after_upload), tempfile.TemporaryDirectory() as directory:
                    checkout = {'uses': 'actions/checkout@v7'}
                    if value is not None:
                        checkout['with'] = {'persist-credentials': value}
                    steps = [upload, checkout] if after_upload else [checkout, upload]
                    workflow = {'name': 'policy-fixture', 'on': 'push', 'jobs': {
                        'build': {'runs-on': 'ubuntu-24.04', 'steps': steps}}}
                    path = Path(directory) / 'policy.yml'
                    path.write_text('# zizmor: ignore[artipacked]\n' + yaml.safe_dump(workflow), encoding='utf-8')
                    result = subprocess.run([sys.executable, str(CHECKER), directory], capture_output=True, text=True)
                    safe = value is False or value == 'false'
                    self.assertEqual(result.returncode, 0 if safe else 1, result.stderr)
                    if not safe:
                        self.assertIn('actions/checkout requires literal persist-credentials: false', result.stderr)


if __name__ == '__main__':
    unittest.main()

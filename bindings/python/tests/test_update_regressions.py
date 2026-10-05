"""Regression checks for installed native transport and failed exports."""
from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from nirs4all_core._conformal import CalibratedWorkflow, export_calibrated
from nirs4all_core._tuning import TuningResult, generate


class CalibratedExportTest(unittest.TestCase):
    def test_missing_source_does_not_reserve_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / 'export.n4a'
            with self.assertRaises(FileNotFoundError):
                export_calibrated(CalibratedWorkflow(root / 'missing.n4a', {}), target)
            self.assertFalse(target.exists())

    def test_copy_failure_removes_only_our_partial_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source.n4a'
            source.write_bytes(b'source')
            target = root / 'export.n4a'
            def failed_copy(source, output):
                output.write(b'partial')
                raise OSError('copy failed')
            with patch('nirs4all_core._conformal.shutil.copyfileobj', failed_copy):
                with self.assertRaisesRegex(OSError, 'copy failed'):
                    export_calibrated(CalibratedWorkflow(source, {}), target)
            self.assertFalse(target.exists())
            target.write_bytes(b'existing')
            with self.assertRaises(FileExistsError):
                export_calibrated(CalibratedWorkflow(source, {}), target)
            self.assertEqual(target.read_bytes(), b'existing')


@unittest.skipUnless(importlib.util.find_spec('nirs4all_core._native'), 'installed native extension required')
class InstalledNativeTest(unittest.TestCase):
    def test_generate_uses_shipped_extension_without_cli(self):
        with patch.dict(os.environ, {'PATH': ''}):
            os.environ.pop('NIRS4ALL_CORE_CLI', None)
            variants = generate({'components': [1, 2]})
        self.assertEqual(len(variants), 2)

    def test_failed_tuning_export_does_not_reserve_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / 'export'
            model = TuningResult(root / 'missing.n4a', {}, {})
            with self.assertRaises(FileNotFoundError):
                model.export(target)
            self.assertFalse(target.exists())

    @unittest.skipUnless(os.environ.get('NIRS4ALL_TUNING_DATASET') and os.environ.get('N4M_LIBRARY_PATH'),
                         'native dataset and Methods runtime required')
    def test_tuned_winner_calibrates_and_replays_frozen_uncertainty(self):
        from nirs4all_core import tune, calibrate, predict_calibrated, robustness
        training = json.loads(Path(os.environ['NIRS4ALL_TUNING_DATASET']).read_text())
        heldout = json.loads(json.dumps(training))
        ids = ['cal.' + sample for sample in training['dataset']['sample_ids']]
        heldout['origin_ids'] = ids
        heldout['dataset']['sample_ids'] = ids
        heldout['dataset']['sources'][0]['sample_ids'] = ids
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            model = tune(training, trials=1, archive=root / 'model.n4a')
            calibrated = calibrate(model.archive, heldout, coverages=[0.8], archive=root / 'calibrated.n4a')
            x = training['dataset']['sources'][0]['array']['values']
            fresh = ['fresh.' + sample for sample in training['dataset']['sample_ids']]
            predicted = predict_calibrated(calibrated, x, sample_ids=fresh)
            self.assertEqual(predicted['interval_block']['sample_ids'], fresh)
            report = robustness(model.archive, x, training['dataset']['y']['values'], sample_ids=fresh,
                                scenarios=[{'id': 'observed', 'kind': 'observed', 'severity': 0, 'seed': 0}])
            self.assertTrue(report)

"""Complete native fit, schema checks and target-free cold-state transport."""
from __future__ import annotations

import importlib.util
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path
from unittest.mock import patch

import numpy as np
from nirs4all_core import Dataset, MultimodalPredictor


@unittest.skipUnless(importlib.util.find_spec('nirs4all_io') and importlib.util.find_spec('n4m'), 'optional IO/Methods dependencies required')
class PublicMultimodalTest(unittest.TestCase):
    def setUp(self) -> None:
        rng = np.random.default_rng(419)
        def source(n):
            return {'nir': rng.normal(size=(n, 5)), 'image': rng.normal(size=(n, 2, 2, 3)),
                    'series': rng.normal(size=(n, 4, 2)), 'metadata': [[i / 3, ['é', '猫'][i % 2]] for i in range(n)]}
        options = {'feature_names': {'metadata': ['measurement', 'category']},
                   'axis_units': {'nir': {'wavelength': 'nm'}},
                   'axis_coordinates': {'nir': {'wavelength': [900, 1000, 1100, 1200, 1300]}}}
        self.train = Dataset.from_sources(source(12), sample_ids=[f'train.{i}' for i in range(12)],
                                           y=rng.normal(size=12), target_names=['protein'], **options)
        values = source(3); values['metadata'][0][1] = '未知'
        self.predict = Dataset.from_sources(values, sample_ids=['new.c', 'new.a', 'new.b'],
                                             partitions=['predict'] * 3, target_names=['protein'], **options)
        self.recipe = {'schema_version': 1, 'fusion': 'early', 'source_order': ['nir', 'image', 'series', 'metadata'],
                       'encoders': {'nir': {'kind': 'standard_scaler', 'with_mean': True, 'with_std': True},
                                    'image': {'kind': 'tensor_pca', 'n_components': 2, 'whiten': False, 'random_state': 17},
                                    'series': {'kind': 'tensor_pca', 'n_components': 2, 'whiten': False, 'random_state': 17},
                                    'metadata': {'kind': 'column_transformer', 'numeric_columns': [0], 'categorical_columns': [1],
                                                 'with_mean': True, 'with_std': True, 'handle_unknown': 'ignore', 'sparse_output': False, 'drop': None}},
                       'source_weights': {'nir': 1., 'image': .5, 'series': 1., 'metadata': 1.},
                       'model': {'method_id': 'models.regularized.ridge', 'params': {'alpha': .3, 'center_x': True, 'center_y': True, 'scale_x': False}}}

    def test_cold_replay_preserves_prediction_ids_target_name_and_snapshot(self) -> None:
        with MultimodalPredictor.fit(self.recipe, self.train) as model:
            expected = model.predict(self.predict)
            record = model.to_dict()
            attempted_mutation = model.recipe; attempted_mutation['model']['params']['alpha'] = 100
            self.assertEqual(model.recipe['model']['params']['alpha'], .3)
        with patch('n4m.MultimodalPipeline.fit', side_effect=AssertionError('cold replay must never FIT')), MultimodalPredictor.load(record) as replay:
            actual = replay.predict(self.predict)
            self.assertEqual(actual['sample_ids'], ['new.c', 'new.a', 'new.b'])
            self.assertEqual(actual['target_names'], ['protein'])
            np.testing.assert_array_equal(actual['values'], expected['values'])
            wrong = deepcopy(self.predict.to_dict()); wrong['dataset']['sources'][0]['axis_units']['wavelength'] = 'cm-1'
            with self.assertRaisesRegex(ValueError, 'schema differs'):
                replay.predict(wrong)
        record['state'][60] ^= 1
        with self.assertRaises(RuntimeError):
            MultimodalPredictor.load(record)

    def test_export_does_not_overwrite_and_predict_refuses_targets(self) -> None:
        with MultimodalPredictor.fit(self.recipe, self.train) as model, tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / 'predictor.json'; model.export(destination)
            with self.assertRaises(FileExistsError): model.export(destination)
            with self.assertRaisesRegex(ValueError, 'target-free'): model.predict(self.train)
            with self.assertRaisesRegex(ValueError, 'observed numeric target'): MultimodalPredictor.fit(self.recipe, self.predict)


if __name__ == '__main__':
    unittest.main()

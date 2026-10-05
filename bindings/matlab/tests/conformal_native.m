% Actual native shared model calibration, keyed intervals and seeded replay.
fixture = getenv('NIRS4ALL_BLOCK5_FIXTURE');
assert(~isempty(fixture), 'NIRS4ALL_BLOCK5_FIXTURE is required for runtime qualification');
root = fileparts(fileparts(mfilename('fullpath'))); addpath(root);
directory = tempname(); mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's')); %#ok<NASGU>
data = nirs4all.dataset(fullfile(fixture, 'dataset-calibration.json'), getenv('NIRS4ALL_CORE_CLI'));
input = jsondecode(fileread(fullfile(fixture, 'independent.json')));
reference = jsondecode(fileread(fullfile(fixture, 'shared-prediction.json')));
model = nirs4all.calibrate(fullfile(fixture, 'shared-model.n4a'), data, ...
    'coverages', [0.8 0.9], 'archive', fullfile(directory, 'matlab-calibrated.n4a'));
prediction = nirs4all.predictCalibrated(model, input.x, input.sample_ids);
assert(isequal(prediction.sample_ids, reference.sample_ids));
for coverage = 1:numel(reference.interval_block.intervals)
    actual = prediction.interval_block.intervals(coverage);
    expected = reference.interval_block.intervals(coverage);
    assert(actual.coverage == expected.coverage);
    for row = 1:numel(expected.cells)
        assert(strcmp(actual.cells(row).status, expected.cells(row).status));
        if strcmp(actual.cells(row).status, 'finite')
            assert(abs(actual.cells(row).lower - expected.cells(row).lower) < 1e-12);
            assert(abs(actual.cells(row).upper - expected.cells(row).upper) < 1e-12);
        end
    end
end
metrics = nirs4all.conformalMetrics(model, prediction, input.y, input.sample_ids);
assert(numel(metrics.coverages) == 2);
relocated = nirs4all.exportCalibrated(model, fullfile(directory, 'relocated.n4a'));
delete(model.archive);
loaded = nirs4all.loadCalibrated(relocated);
replay = nirs4all.predictCalibrated(loaded, input.x, input.sample_ids);
assert(isequal(replay.interval_block.intervals, prediction.interval_block.intervals));
singleton = nirs4all.predictCalibrated(loaded, input.x(1,:), input.sample_ids(1));
singletonMetrics = nirs4all.conformalMetrics(loaded, singleton, input.y(1), input.sample_ids(1));
assert(numel(singleton.sample_ids)==1 && numel(singletonMetrics.coverages)==2);
% Preserve native owner IO JSON, including nulls, through the typed Dataset.
structured = nirs4all.dataset(fullfile(fixture,'dataset-predict.json'), getenv('NIRS4ALL_CORE_CLI'));
matched = nirs4all.predictCalibrated(loaded, structured);
assert(isequal(matched.sample_ids, replay.sample_ids));
assert(isequal(size(matched.point_prediction.values),size(replay.point_prediction.values)));
assert(max(abs(matched.point_prediction.values(:)-replay.point_prediction.values(:))) < 1e-12);
report = nirs4all.robustness(loaded, input.x, input.y, input.sample_ids);
structuredReport = nirs4all.robustness(loaded, structured, input.y);
singletonReport = nirs4all.robustness(loaded, input.x(1,:), input.y(1), input.sample_ids(1));
assert(isequal(size(structuredReport.scenarios(1).point_predictions),size(report.scenarios(1).point_predictions)));
assert(max(abs(structuredReport.scenarios(1).point_predictions(:)-report.scenarios(1).point_predictions(:))) < 1e-12);
assert(numel(singletonReport.scenarios(1).point_predictions)==1);
expected = jsondecode(fileread(fullfile(fixture, 'shared-robustness.json')));
assert(strcmp(report.mode, 'clean_frozen'));
assert(max(abs(report.scenarios(2).point_predictions(:) - expected.scenarios(2).point_predictions(:))) < 1e-12);
fprintf('native MATLAB/Octave conformal and seeded robustness cross-language gate passed\n');

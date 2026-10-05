fixture = getenv('NIRS4ALL_EXPERIMENT_FIXTURE');
assert(~isempty(fixture), 'Set NIRS4ALL_EXPERIMENT_FIXTURE to native-written experiment');
addpath(fileparts(fileparts(mfilename('fullpath'))));
view = nirs4all.resultView(fixture);
assert(strcmp(view.validation_level, 'native_results_and_model_prediction_closure'));
assert(~isempty(view.model_archive));
native = jsondecode(fileread(fullfile(fixture, 'result_view.json')));
assert(isequaln(nirs4all.resultPredictions(view), native.predictions));
scores = nirs4all.resultCompare(view);
assert(isequaln(scores(1).metrics, native.score_set.reports(1).metrics));
rows = nirs4all.resultPredictions(view);
rows(1).y_pred(1) = 999;
assert(isequaln(nirs4all.resultPredictions(view), native.predictions));
failed = false;
try, nirs4all.resultPredictions(view, 'absent'); catch, failed = true; end
assert(failed);
mixedReports = jsondecode('[{"variant_id":"variant:one","partition":"validation"},{"partition":"final"}]');
mixedView = struct('score_set', struct('reports', {mixedReports}), ...
    'variant_ids', {{'variant:one'}}, 'winner_variant_id', 'variant:one');
compared = nirs4all.resultCompare(mixedView);
assert(iscell(compared) && numel(compared) == 2);
assert(compared{1}.is_winner && ~compared{2}.is_winner);
filtered = nirs4all.resultCompare(mixedView, 'variant:one', 'validation');
assert(iscell(filtered) && numel(filtered) == 1 && filtered{1}.is_winner);
fprintf('Native Octave result view: PASS\n');

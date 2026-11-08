% Canonical V1 .n4a operations through the native Core CLI (n4a-* commands).
%
% Real CLI, real owners, real Methods library; nothing is mocked. The profile
% witness is standard_scale + ridge with two ordered targets (protein,
% moisture); it is not a universal-support claim.
%
% Environment:
%   NIRS4ALL_CORE_CLI            nirs4all-core-archive that provides n4a-*
%   N4M_LIBRARY_PATH             Methods library file
%   N4M_EXPECTED_LIBRARY_SHA256  independently attested SHA-256 of that file
%   NIRS4ALL_N4A_FIXTURE_DIR     directory with train.json, train-v2.json,
%                                future.json, future-permuted.json, recipe.json
%                                (written by the Rust test with
%                                N4A_CLI_WRITE_FIXTURES=<dir>)
%   NIRS4ALL_REQUIRE_CORE_N4A=1  turn missing prerequisites into a failure
%
% Without prerequisites the script prints an explicit SKIP line and asserts
% nothing. It prints OCTAVE_N4A_CANONICAL_PASS only after the assertions ran.
root = fileparts(fileparts(mfilename('fullpath'))); addpath(root);

% --- Memory budget: refusals and forwarding through every n4a-* transport ---
% A stub CLI echoes its arguments; no native executable, library or fixture is used.
stubDir = tempname(); mkdir(stubDir); stubCleanup = onCleanup(@() rmdir(stubDir, 's')); %#ok<NASGU>
stubFile = fullfile(stubDir, 'input.json'); f = fopen(stubFile, 'w'); fprintf(f, 'x'); fclose(f);
refused = false;
try, nirs4all.n4aInspect(stubFile, 'memoryBudgetBytes', 1.5);
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aArgument'); end
assert(refused, 'a fractional budget must be refused before any CLI is resolved');
refused = false;
try, nirs4all.n4aProduce(stubFile, stubFile, fullfile(stubDir, 'a.n4a'), 'methodsLibrary', 'l', ...
        'methodsLibrarySha256', '00', 'memoryBudgetBytes', true);
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aArgument'); end
assert(refused, 'a logical budget must be refused');
if ~ispc
    stub = fullfile(stubDir, 'stub-cli.sh'); f = fopen(stub, 'w');
    fprintf(f, '%s', ['#!/bin/sh' newline 'out=""; prev=""' newline ...
        'for a in "$@"; do if [ "$prev" = "--output" ]; then out="$a"; fi; prev="$a"; done' newline ...
        'printf ''{"argv":"%s","sample_ids":["a"],"target_names":["t"],"target_units":[null],"values":[[1.5]],"source_id":"s","partition":"p","input_mode":"m","archive_sha256":"x","replay":{},"receipt":{},"training_outcome":{},"training_source_id":"s","source_archive_sha256":"y","readability":{"container_sha256":"z"},"executability":"not_evaluated"}'' "$*" > "$out"' newline]);
    fclose(f); assert(system(['chmod +x ''' stub '''']) == 0);
    flag = '--memory-budget-bytes 18446744073709551615'; big = intmax('uint64');
    stubModel = struct('archive', stubFile, 'archiveSha256', 'z', 'cli', stub);
    stubRuntime = {'methodsLibrary', 'l', 'methodsLibrarySha256', '00'};
    got = nirs4all.n4aInspect(stubFile, 'cli', stub, 'memoryBudgetBytes', big);
    assert(~isempty(strfind(got.nativeJson, flag)));
    got = nirs4all.n4aProduce(stubFile, stubFile, fullfile(stubDir, 'p.n4a'), stubRuntime{:}, 'cli', stub, 'memoryBudgetBytes', big);
    assert(~isempty(strfind(got.nativeJson, flag)));
    got = nirs4all.n4aPredict(stubModel, stubRuntime{:}, 'memoryBudgetBytes', big);
    assert(~isempty(strfind(got.nativeJson, flag)));
    got = nirs4all.n4aRetrain(stubModel, stubFile, fullfile(stubDir, 'r.n4a'), stubRuntime{:}, 'memoryBudgetBytes', big);
    assert(~isempty(strfind(got.nativeJson, flag)));
    got = nirs4all.n4aInspect(stubFile, 'cli', stub);
    assert(isempty(strfind(got.nativeJson, '--memory-budget')), 'a missing budget sends no flag');
end

cli = getenv('NIRS4ALL_CORE_CLI'); library = getenv('N4M_LIBRARY_PATH');
sha = getenv('N4M_EXPECTED_LIBRARY_SHA256'); fixtures = getenv('NIRS4ALL_N4A_FIXTURE_DIR');
required = strcmp(getenv('NIRS4ALL_REQUIRE_CORE_N4A'), '1');
names = {'NIRS4ALL_CORE_CLI', 'N4M_LIBRARY_PATH', 'N4M_EXPECTED_LIBRARY_SHA256', 'NIRS4ALL_N4A_FIXTURE_DIR'};
values = {cli, library, sha, fixtures};
missing = names(cellfun(@isempty, values));
if ~isempty(missing)
    if required
        error('n4a_canonical:Prerequisites', 'Missing required environment: %s', strjoin(missing, ', '));
    end
    fprintf('SKIP n4a_canonical: %s not set; no native assertion ran\n', strjoin(missing, ', '));
    return;
end
runtime = {'cli', cli, 'methodsLibrary', library, 'methodsLibrarySha256', sha};
directory = tempname(); mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's')); %#ok<NASGU>

% --- Argument errors and refusals that need no Methods runtime -------------
refused = false;
try, nirs4all.n4aProduce(struct('x', 1), 'recipe', fullfile(directory, 'a.n4a'), runtime{:});
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aRecord'); end
assert(refused, 'a struct record must be refused: re-encoding changes record bytes');
garbage = fullfile(directory, 'garbage.n4a');
f = fopen(garbage, 'w'); fprintf(f, 'not a container'); fclose(f);
refused = false;
try, nirs4all.n4aInspect(garbage, 'cli', cli);
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aRefusal'); end
assert(refused, 'a non-container must be refused with the native message');

% --- Produce once, seal the owner bytes --------------------------------------
train = fullfile(fixtures, 'train.json'); trainV2 = fullfile(fixtures, 'train-v2.json');
future = fullfile(fixtures, 'future.json'); permuted = fullfile(fixtures, 'future-permuted.json');
archive = fullfile(directory, 'model.n4a');
model = nirs4all.n4aProduce(train, fullfile(fixtures, 'recipe.json'), archive, runtime{:}, ...
    'embedTrainingRecord', true, 'runId', 'run:octave:n4a:produce', 'containerId', 'octave-n4a-produce');
assert(exist(archive, 'file') == 2);
assert(~isempty(regexp(model.archiveSha256, '^[0-9a-f]{64}$', 'once')));
if exist('OCTAVE_VERSION', 'builtin') ~= 0
    assert(strcmp(hash('sha256', fileread(archive)), model.archiveSha256));
end
sealed = fileread(archive);
refused = false;
try, nirs4all.n4aProduce(train, fullfile(fixtures, 'recipe.json'), archive, runtime{:});
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aRefusal'); end
assert(refused && isequal(sealed, fileread(archive)), 'produce must never replace an existing archive');

% --- Inspect: readable, executability not evaluated --------------------------
info = nirs4all.n4aInspect(archive, 'cli', cli, 'expectedSha256', model.archiveSha256);
assert(strcmp(info.executability, 'not_evaluated'));
assert(strcmp(info.readability.storage, 'verified'));
assert(strcmp(info.archiveSha256, model.archiveSha256));
assert(strcmp(info.readability.manifest.schema, 'nirs4all.n4a.container.v1'));
assert(strcmp(info.readability.training_view.status, 'readable'));

% --- Predict: physical row order, target axes, no refit ----------------------
futureRecord = jsondecode(fileread(future)); permutedRecord = jsondecode(fileread(permuted));
p1 = nirs4all.n4aPredict(model, 'record', future, 'partition', 'predict', runtime{:});
p2 = nirs4all.n4aPredict(model, 'record', permuted, 'partition', 'predict', runtime{:});
assert(isequal(sealed, fileread(archive)), 'predict must not change the archive');
assert(isequal(p1.sampleIds(:), futureRecord.dataset.sample_ids(:)));
assert(isequal(p2.sampleIds(:), permutedRecord.dataset.sample_ids(:)));
assert(isequal(p1.targetNames(:), {'protein'; 'moisture'}));
assert(isequal(size(p1.values), [5 2]) && isequal(size(p2.values), [5 2]));
assert(all(isfinite(p1.values(:))) && all(isfinite(p2.values(:))));
assert(strcmp(p1.partition, 'predict') && strcmp(p1.sourceId, 'spectra'));
assert(strcmp(p1.inputMode, 'external'));
assert(strcmp(p2.replay.phase, 'PREDICT'));
assert(strcmp(p2.receipt.authenticity, 'not_established'));
[found, at] = ismember(p2.sampleIds, p1.sampleIds); assert(all(found));
assert(max(max(abs(p2.values - p1.values(at, :)))) < 1e-9, 'each sample must keep its prediction');
assert(max(max(abs(p2.values - p1.values))) > 1e-6, 'the permutation must be visible positionally');
block = p2.replay.outputs(1).predictions(1);
[found, at] = ismember(p2.sampleIds, block.sample_ids); assert(all(found));
assert(isequal(p2.values, block.values(at, :)), 'values must be the replay rows aligned by sample ID');
assert(~isequal(p2.sampleIds(:), block.sample_ids(:)), 'the owner replay order must differ from physical order here');

% Stored in-sample prediction: each column is closer to its own target.
stored = nirs4all.n4aPredict(model, runtime{:});
assert(strcmp(stored.inputMode, 'stored') && strcmp(stored.partition, 'train'));
trainRecord = jsondecode(fileread(train)); y = trainRecord.dataset.y.values;
assert(isequal(stored.sampleIds(:), trainRecord.dataset.sample_ids(:)));
for c = 1:2
    own = mean(abs(stored.values(:, c) - y(:, c)));
    other = mean(abs(stored.values(:, c) - y(:, 3 - c)));
    assert(own < other, 'target axes appear swapped');
end

% --- Refusals: native messages, no fallback, archive unchanged ---------------
bad = model; bad.archiveSha256 = repmat('0', 1, 64);
refused = false;
try, nirs4all.n4aPredict(bad, 'record', future, runtime{:});
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aRefusal') && ~isempty(strfind(err.message, 'archive bytes changed')); end
assert(refused, 'a wrong archive SHA must be refused');
refused = false;
try, nirs4all.n4aPredict(model, 'record', future, 'cli', cli, 'methodsLibrary', library, ...
        'methodsLibrarySha256', repmat('0', 1, 64));
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aRefusal'); end
assert(refused, 'a wrong Methods SHA must be refused');
refused = false;
try, nirs4all.n4aPredict(model, 'record', future, 'entrypoint', 'calibrate-external', runtime{:});
catch err, refused = strcmp(err.identifier, 'nirs4all:N4aRefusal'); end
assert(refused, 'the owner must refuse an operation that is not a prediction');
assert(isequal(sealed, fileread(archive)));

% --- Retrain through the owner API (stored recipe and source ID, no host loop)
% Owner errors propagate; there is no skip once the prerequisites are present.
retrained = fullfile(directory, 'retrained.n4a');
fresh = nirs4all.n4aRetrain(model, trainV2, retrained, runtime{:}, ...
    'embedTrainingRecord', true, 'runId', 'run:octave:n4a:retrain', 'containerId', 'octave-n4a-retrain');
assert(exist(retrained, 'file') == 2);
assert(isequal(sealed, fileread(archive)), 'the source archive must stay intact');
assert(~strcmp(fresh.archiveSha256, model.archiveSha256));
assert(strcmp(fresh.sourceArchiveSha256, model.archiveSha256));
after = nirs4all.n4aPredict(fresh, 'record', future, 'partition', 'predict', runtime{:});
assert(isequal(after.sampleIds(:), p1.sampleIds(:)));
assert(max(max(abs(after.values - p1.values))) > 1e-6, 'new targets must change the predictions');
fprintf('OCTAVE_N4A_CANONICAL_PASS produce=1 inspect=1 predict_external_permuted=1 predict_stored=1 refusals=1 retrain=1\n');

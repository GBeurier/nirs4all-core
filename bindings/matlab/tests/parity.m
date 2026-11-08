function parity()
scriptDir = fileparts(mfilename('fullpath'));
root = fileparts(fileparts(fileparts(scriptDir)));
addpath(fullfile(scriptDir, '..'));

methodsPath = getenv('NIRS4ALL_METHODS_MATLAB_PATH');
if isempty(methodsPath)
    candidate = fullfile(root, '..', 'nirs4all-methods', 'bindings', 'matlab');
    if exist(candidate, 'dir') == 7
        methodsPath = candidate;
    end
end
if ~isempty(methodsPath) && exist(methodsPath, 'dir') == 7
    addpath(methodsPath);
end

strict = strcmp(getenv('NIRS4ALL_CORE_REQUIRE_METHODS_PARITY'), '1');
% The migrated native Core facade is required. MEX availability is not a gate.
probe = ['{"schema_version":1,"definition":{"pipeline":' ...
    '[{"model":{"class":"n4m.PLSRegression","params":{"n_components":1}}}]}}'];
admission = nirs4all.workflowCli('', 'portable-pipeline-plan', probe, struct());
assert(strcmp(admission.profile, 'portable_linear_regression.v1'));
assert(strcmp(admission.mode, 'training'));

oraclePath = getenv('NIRS4ALL_CORE_PARITY_ORACLE');
if isempty(oraclePath)
    oraclePath = fullfile(scriptDir, 'parity', 'expected', 'portable_python_oracle.json');
    if exist(oraclePath,'file') ~= 2
        oraclePath = fullfile(root, 'tests', 'parity', 'expected', 'portable_python_oracle.json');
    end
end
fixtureRoot = getenv('NIRS4ALL_CORE_PARITY_FIXTURES');
if isempty(fixtureRoot)
    fixtureRoot = fullfile(scriptDir, 'parity', 'fixtures');
    if exist(fixtureRoot,'dir') ~= 7
        fixtureRoot = fullfile(root, 'tests', 'parity', 'fixtures');
    end
end

if exist(oraclePath, 'file') ~= 2
    if strict
        error('nirs4all:testFailed', 'portable parity oracle not found: %s', oraclePath);
    end
    disp('Skipping MATLAB/Octave execution parity: oracle is not available');
    return
end

oracle = readJson(oraclePath);
dataset = struct( ...
    'X', oracle.dataset.X, ...
    'y', oracle.dataset.y, ...
    'rows', oracle.dataset.rows, ...
    'cols', oracle.dataset.cols);
cases = asCell(oracle.cases);
tolerances = oracle.metadata.tolerances;
assert(numel(cases) >= 4);

modeFixture = struct();
modeFixture.name = 'savgol_mode_contract';
modeFixture.pipeline = { ...
    struct( ...
        'class', 'nirs4all.operators.transforms.SavitzkyGolay', ...
        'params', struct('window_length', 11, 'mode', 'constant', 'cval', 7.25)), ...
    struct( ...
        'model', struct( ...
            'class', 'sklearn.cross_decomposition.PLSRegression', ...
            'params', struct('n_components', 2))) ...
};
modeResult = nirs4all.runPortablePipeline(modeFixture, dataset);
modeParams = modeResult.preprocessing{1}.params;
assert(modeParams.window_length == 11 && modeParams.polyorder == 3 && modeParams.deriv == 0);
assert(strcmp(modeParams.mode, 'constant') && modeParams.cval == 7.25);

for idx = 1:numel(cases)
    expected = cases{idx};
    fixture = fullfile(fixtureRoot, [expected.name '.json']);
    assert(exist(fixture, 'file') == 2);
    actual = nirs4all.runPortablePipeline(fixture, dataset);
    if ~strcmp(actual.split.kind, 'all')
        alternate = nirs4all.loadPipelineDefinition(fixture);
        alternate = alternate.original_root;
        steps = asCell(alternate.pipeline);
        steps = steps(:);
        alternate.pipeline = [steps(2:end-1); steps(1); steps(end)];
        reordered = nirs4all.runPortablePipeline(alternate, dataset);
        for field = {'split', 'preprocessing', 'variants', 'selected', 'targets', 'evaluation', 'identities'}
            assert(isequal(reordered.(field{1}), actual.(field{1})));
        end
    end
    assert(~actual.evaluation.independent_test);
    if strcmp(actual.split.kind, 'all')
        assert(strcmp(actual.evaluation.scope, 'training'));
    else
        assert(strcmp(actual.evaluation.scope, 'selection_validation'));
    end

    assert(strcmp(actual.split.kind, expected.split.kind));
    assert(isequal(double(actual.split.trainIndices(:)), double(expected.split.trainIndices(:))));
    assert(isequal(double(actual.split.testIndices(:)), double(expected.split.testIndices(:))));
    assert(strcmp(actual.profile, 'portable_linear_regression.v1'));
    assert(strcmp(actual.owner_profile, 'pipeline_steps.v1'));
    ids = actual.identities.sample_ids;
    assert(isequal(actual.identities.training_sample_ids(:), ids(double(actual.split.trainIndices(:)) + 1)));
    assert(isequal(actual.identities.evaluation_sample_ids(:), ids(double(actual.split.testIndices(:)) + 1)));
    assert(~isempty(fieldnames(actual.owner.decision)));
    assert(maxAbsDiff(actual.targets, expected.targets) <= tolerances.targets_abs);

    expectedVariants = asCell(expected.variants);
    assert(numel(actual.variants) == numel(expectedVariants));
    for variantIdx = 1:numel(expectedVariants)
        actualVariant = actual.variants{variantIdx};
        expectedVariant = expectedVariants{variantIdx};
        assert(double(actualVariant.n_components) == double(expectedVariant.n_components));
        assert(abs(actualVariant.rmse - expectedVariant.rmse) <= tolerances.rmse_abs);
        assert(maxAbsDiff(actualVariant.predictions, expectedVariant.predictions) <= tolerances.predictions_abs);
    end
    assert(double(actual.selected.n_components) == double(expected.selected.n_components));
end

% Frozen B02 case inputs, with new assertions against the native owner route.
% The original fixture/oracle files and tolerances are unchanged.
for caseIdx = 1:2
    if caseIdx == 1
        snvParams = struct('with_std', false, 'ddof', 3);
        modelParams = struct('n_components', 2);
        components = 2;
        paths = {'.params.with_std', '.params.ddof'};
    else
        snvParams = struct();
        modelParams = struct('n_components', 5, 'scale', false);
        components = [1 2 3];
        paths = {'.params.scale'};
    end
    snv = struct('class', 'n4m.SNV', 'params', snvParams);
    model = struct('model', struct('class', 'sklearn.cross_decomposition.PLSRegression', 'params', modelParams));
    if caseIdx == 2
        model.param = 'n_components'; model.('_range_') = [1 3 1];
    end
    try
        actual = nirs4all.runPortablePipeline(struct('pipeline', {{snv, model}}), dataset);
    catch err
        named = false;
        for pathIdx = 1:numel(paths)
            named = named || ~isempty(strfind(err.message, paths{pathIdx}));
        end
        assert(named, err.message);
        continue
    end
    assert(strcmp(actual.evaluation.scope, 'training') && ~actual.evaluation.independent_test);
    assert(isequal(double(actual.split.trainIndices(:)), (0:(double(dataset.rows) - 1)).'));
    assert(isequal(actual.split.trainIndices, actual.split.testIndices));
    assert(isequal(double(actual.targets(:)), double(dataset.y(:))));
    assert(numel(actual.variants) == numel(components));
    for variantIdx = 1:numel(components)
        assert(actual.variants{variantIdx}.n_components == components(variantIdx));
    end
    if caseIdx == 1
        assert(~actual.preprocessing{1}.params.with_std && actual.preprocessing{1}.params.ddof == 3);
    end
    assert(~isempty(fieldnames(actual.owner.decision)));
end

disp('nirs4all MATLAB/Octave execution parity passed');
end

function data = readJson(path)
fid = fopen(path, 'r');
txt = fread(fid, Inf, 'uint8=>char')';
fclose(fid);
data = jsondecode(txt);
end

function cells = asCell(value)
if iscell(value)
    cells = value;
elseif isstruct(value)
    cells = num2cell(value);
else
    error('nirs4all:testFailed', 'expected JSON list value');
end
end

function diff = maxAbsDiff(actual, expected)
a = double(actual(:));
e = double(expected(:));
assert(numel(a) == numel(e));
if isempty(a)
    diff = 0;
else
    diff = max(abs(a - e));
end
end

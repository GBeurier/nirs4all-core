addpath(fullfile(fileparts(mfilename('fullpath')), '..'));

% The surrounding temporary directory may be longer than DAG's identifier
% budget. Only the unique filename token belongs in automatically assigned IDs.
temporaryId = nirs4all.temporaryRunId('run:smoke:');
assert(length(temporaryId) < 64);
assert(~isempty(regexp(temporaryId, '^run:smoke:[A-Za-z0-9_.:-]+$', 'once')));
assert(isempty(strfind(temporaryId, tempdir())));

items = nirs4all.upstreams();
assert(numel(items) == 6);
assert(strcmp(items(1).key, 'dag_ml'));
method_item = nirs4all.requireUpstream('methods');
assert(strcmp(method_item.key, 'methods'));
dag_ml_item = nirs4all.requireUpstream('dag_ml');
assert(strcmp(dag_ml_item.package, '+dagml'));

dag_ml_path = getenv('NIRS4ALL_DAG_ML_MATLAB_PATH');
if ~isempty(dag_ml_path)
    addpath(dag_ml_path);
    registry = nirs4all.localImplementationRegistry();
    assert(isa(registry, 'dagml.LocalImplementationRegistry'));
    registryMethods = methods(registry);
    assert(any(strcmp(registryMethods, 'registerLoss')));
    assert(any(strcmp(registryMethods, 'registerMetric')));
    assert(any(strcmp(registryMethods, 'invokeTrainingLoss')));
    assert(registry.count() == 0);
end

manifest = nirs4all.capabilityManifest();
assert(strcmp(manifest.schema, 'nirs4all-core.capabilities.v1'));
assert(isequal(nirs4all.runtimeSurfaces(), {'python', 'javascript_wasm', 'rust', 'matlab_octave'}));

try
    nirs4all.requireUpstream('missing');
    error('nirs4all:testFailed', 'missing upstream should fail');
catch err
    assert(strcmp(err.identifier, 'nirs4all:UnknownUpstream'));
end

fixture_dir = fullfile(fileparts(mfilename('fullpath')), 'parity', 'fixtures');
if exist(fixture_dir, 'dir') ~= 7
    fixture_dir = fullfile(fileparts(mfilename('fullpath')), '..', '..', '..', 'tests', 'parity', 'fixtures');
end
contractCases = jsondecode(fileread(fullfile(fixture_dir, 'execution_contract_cases.json')));
for idx = 1:numel(contractCases.invalid)
    if iscell(contractCases.invalid)
        contractCase = contractCases.invalid{idx};
    else
        contractCase = contractCases.invalid(idx);
    end
    refused = false;
    try
        nirs4all.runPortablePipeline(contractCase, struct());
    catch err
        % Validation runs before Methods discovery or dataset materialization.
        refused = any(strcmp(err.identifier, {'nirs4all:InvalidPipeline', 'nirs4all:UnsupportedOperator'}));
    end
    assert(refused, ['Accepted invalid contract: ' contractCase.name]);
end
fixture_files = dir(fullfile(fixture_dir, 'portable_*.json'));
assert(numel(fixture_files) >= 4);
for i = 1:numel(fixture_files)
    [~, fixture_name, ~] = fileparts(fixture_files(i).name);
    json_definition = nirs4all.loadPipelineDefinition(fullfile(fixture_dir, [fixture_name '.json']));
    yaml_definition = nirs4all.loadPipelineDefinition(fullfile(fixture_dir, [fixture_name '.yaml']));
    assert(numel(json_definition.pipeline) == numel(yaml_definition.pipeline));
    assert(numel(nirs4all.portableClassNames(json_definition)) > 0);
end

json_pipeline = nirs4all.loadPipelineDefinition(fullfile(fixture_dir, 'portable_methods_pipeline.json'));
yaml_pipeline = nirs4all.loadPipelineDefinition(fullfile(fixture_dir, 'portable_methods_pipeline.yaml'));

expected_classes = { ...
    'nirs4all.operators.splitters.KennardStoneSplitter', ...
    'nirs4all.operators.transforms.StandardNormalVariate', ...
    'nirs4all.operators.transforms.SavitzkyGolay', ...
    'sklearn.cross_decomposition.PLSRegression' ...
};
assert(isequal(nirs4all.portableClassNames(json_pipeline), expected_classes));
assert(isequal(nirs4all.portableClassNames(yaml_pipeline), expected_classes));
assert(numel(json_pipeline.pipeline) == numel(yaml_pipeline.pipeline));

sweep = json_pipeline.pipeline{4};
range = sweep.('_range_');
assert(strcmp(sweep.param, 'n_components'));
assert(isequal(range(:)', [2 11 2]));

from_steps = nirs4all.loadPipelineDefinition(struct('steps', {json_pipeline.pipeline}));
from_list = nirs4all.loadPipelineDefinition(json_pipeline.pipeline);
assert(numel(from_steps.pipeline) == numel(json_pipeline.pipeline));
assert(numel(from_list.pipeline) == numel(json_pipeline.pipeline));

try
    nirs4all.runPortablePipeline(struct('pipeline', {{struct('class', 'sklearn.ensemble.RandomForestRegressor')}}), struct());
    error('nirs4all:testFailed', 'unsupported operator should fail');
catch err
    assert(strcmp(err.identifier, 'nirs4all:UnsupportedOperator'));
end

% Execution profiles: structural comparison with the TOML-generated fixture.
% Exactly the native-workflow@1 and native-conformal@1 profiles are published.
expected_dir = fullfile(fileparts(mfilename('fullpath')), 'parity', 'expected');
if exist(expected_dir, 'dir') ~= 7
    expected_dir = fullfile(fileparts(mfilename('fullpath')), '..', '..', '..', 'tests', 'parity', 'expected');
end
profileFixture = jsondecode(fileread(fullfile(expected_dir, 'capability_profiles.json')));
profiles = nirs4all.executionProfiles();
assert(isequal(profiles, profileFixture));
assert(isequal(sort(fieldnames(manifest)), sort({'schema'; 'aggregate'; 'runtimeSurfaces'; 'executionProfiles'})));
assert(isequal(manifest.executionProfiles, profileFixture));
assert(strcmp(profiles.schema, 'nirs4all-core.execution-profiles.v1'));
assert(profiles.version == 1);
assert(isequal(profiles.runtime_surfaces(:)', {'python', 'javascript_wasm', 'rust', 'matlab_octave'}));
profileList = profiles.profiles;
expectedIds = {'native-workflow', 'native-conformal'};
assert(numel(profileList) == 2);
surfaces = {'python', 'javascript_wasm', 'rust', 'matlab_octave'};
operations = {'compile', 'fit', 'predict', 'import', 'export'};
S = 'supported'; U = 'unsupported';
expectedStatus = struct( ...
    'native_workflow', struct('compile', {{U, U, S, U}}, 'fit', {{S, S, S, S}}, 'predict', {{S, S, S, S}}, 'import', {{S, S, U, S}}, 'export', {{S, S, U, S}}), ...
    'native_conformal', struct('compile', {{U, U, U, U}}, 'fit', {{S, S, S, S}}, 'predict', {{S, S, S, S}}, 'import', {{S, S, U, S}}, 'export', {{S, S, U, S}}));
statusKeys = {'native_workflow', 'native_conformal'};
for profileIdx = 1:2
    if iscell(profileList), item = profileList{profileIdx}; else, item = profileList(profileIdx); end
    assert(strcmp(item.id, expectedIds{profileIdx}));
    assert(item.version == 1);
    assert(isequal(item.surfaces(:)', surfaces));
    assert(isequal(fieldnames(item.operations)', operations));
    for opIdx = 1:numel(operations)
        cells = item.operations.(operations{opIdx});
        assert(isequal(fieldnames(cells)', surfaces));
        wanted = expectedStatus.(statusKeys{profileIdx}).(operations{opIdx});
        for surfaceIdx = 1:numel(surfaces)
            cell = cells.(surfaces{surfaceIdx});
            assert(strcmp(cell.status, wanted{surfaceIdx}));
            if strcmp(cell.status, U)
                assert(~isempty(cell.reason) && ~isfield(cell, 'entrypoints'));
            else
                assert(~isempty(cell.entrypoints) && ~isfield(cell, 'reason'));
            end
        end
    end
end
if iscell(profileList), conformalProfile = profileList{2}; else, conformalProfile = profileList(2); end
if iscell(profileList), workflowProfile = profileList{1}; else, workflowProfile = profileList(1); end
assert(strcmp(workflowProfile.content_types.schema, 'nirs4all.native-pipeline.v1'));
for profileIdx = 1:2
    if iscell(profileList), item = profileList{profileIdx}; else, item = profileList(profileIdx); end
    assert(isequal(fieldnames(item.runtime_requirements)', surfaces));
    assert(~isempty(strfind(strjoin(item.runtime_requirements.javascript_wasm, ' '), 'no native library path and no CLI'))); %#ok<STREMP>
    assert(~isempty(strfind(strjoin(item.runtime_requirements.matlab_octave, ' '), 'nirs4all-core-archive CLI'))); %#ok<STREMP>
end
assert(strcmp(conformalProfile.content_types.schema, 'nirs4all.calibrated.v1'));
assert(strcmp(conformalProfile.content_types.owner, 'nirs4all-core'));
routes = conformalProfile.content_routes;
if ~iscell(routes), routes = num2cell(routes); end
assert(numel(routes) == 21);
assert(isempty(workflowProfile.content_routes));
for routeIdx = 1:numel(routes)
    route = routes{routeIdx};
    if strcmp(route.surface, 'javascript_wasm'), assert(~isfield(route, 'schema')); end
    if strcmp(route.route, 'archive_transfer')
        assert(route.archive_schema_version == 2 && ~isfield(route, 'schema'));
        assert(strcmp(route.calibration_in_archive, 'PortablePredictorPackage.conformal_calibration'));
    end
end
% R is federated externally, never a Core runtime surface.
assert(~any(strcmp(nirs4all.runtimeSurfaces(), 'r')));
federation = profiles.external_federation;
if iscell(federation), federation = federation{1}; else, federation = federation(1); end
assert(strcmp(federation.surface, 'r') && federation.core_runtime_surface == false);
assert(isequal(federation.profiles(:)', expectedIds));
% The removed capability APIs no longer exist in this package.
removed = {'runtimeContracts', 'artifactContracts', 'controllerCapabilities', 'requiredKeywordRegistryEntries'};
for removedIdx = 1:numel(removed)
    assert(exist(['nirs4all.' removed{removedIdx}]) == 0); %#ok<EXIST>
end
% Every supported MATLAB entrypoint resolves to a real source file.
repoRoot = fullfile(fileparts(mfilename('fullpath')), '..', '..', '..');
if exist(fullfile(repoRoot, 'bindings', 'matlab', '+nirs4all'), 'dir') == 7
    for profileIdx = 1:2
        if iscell(profileList), item = profileList{profileIdx}; else, item = profileList(profileIdx); end
        for opIdx = 1:numel(operations)
            entry = item.operations.(operations{opIdx}).matlab_octave;
            if strcmp(entry.status, 'supported')
                entrypoints = entry.entrypoints;
                for entryIdx = 1:numel(entrypoints)
                    if iscell(entrypoints), ep = entrypoints{entryIdx}; else, ep = entrypoints(entryIdx); end
                    assert(exist(fullfile(repoRoot, ep.source), 'file') == 2, ep.source);
                end
            end
        end
    end
end
% Independent copies: mutating a returned value never changes the next call.
mutated = nirs4all.executionProfiles();
mutated.schema = 'mutated';
fresh = nirs4all.executionProfiles();
assert(strcmp(fresh.schema, 'nirs4all-core.execution-profiles.v1'));

disp('nirs4all MATLAB/Octave smoke passed');

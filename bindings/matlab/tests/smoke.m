addpath(fullfile(fileparts(mfilename('fullpath')), '..'));

items = nirs4all.upstreams();
assert(numel(items) == 6);
assert(strcmp(items(1).key, 'dag_ml'));
method_item = nirs4all.requireUpstream('methods');
assert(strcmp(method_item.key, 'methods'));

try
    nirs4all.requireUpstream('missing');
    error('nirs4all:testFailed', 'missing upstream should fail');
catch err
    assert(strcmp(err.identifier, 'nirs4all:UnknownUpstream'));
end

fixture_dir = fullfile(fileparts(mfilename('fullpath')), '..', '..', '..', 'tests', 'parity', 'fixtures');
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
grid = sweep.('_grid_');
assert(strcmp(sweep.param, 'n_components'));
assert(isequal(grid.n_components, [2 4 6 8 10]));

from_steps = nirs4all.loadPipelineDefinition(struct('steps', {json_pipeline.pipeline}));
from_list = nirs4all.loadPipelineDefinition(json_pipeline.pipeline);
assert(numel(from_steps.pipeline) == numel(json_pipeline.pipeline));
assert(numel(from_list.pipeline) == numel(json_pipeline.pipeline));

try
    nirs4all.loadPipelineDefinition(struct('pipeline', {{struct('class', 'sklearn.ensemble.RandomForestRegressor')}}));
    error('nirs4all:testFailed', 'unsupported operator should fail');
catch err
    assert(strcmp(err.identifier, 'nirs4all:UnsupportedOperator'));
end

disp('nirs4all MATLAB/Octave smoke passed');

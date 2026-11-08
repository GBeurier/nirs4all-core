function result = runPortablePipeline(source, dataset)
%RUNPORTABLEPIPELINE Delegate portable evaluation to the native Core owner.
%   The owner admits the original document without fitting, then executes
%   training or one KS holdout. Results retain owner order and score scope.

if isstruct(source) && isscalar(source) && isfield(source, 'source') && ...
        isfield(source, 'original_root') && isfield(source, 'pipeline')
    definition = source;
else
    definition = nirs4all.loadPipelineDefinition(source);
end
recipeJson = documentJSON(definition);
planRequest = ['{"schema_version":1,"definition":' recipeJson '}'];
ownerCall('portable-pipeline-plan', planRequest, struct(), true);

library = getenv('N4M_LIBRARY_PATH');
if isempty(library), library = getenv('N4M_LIB_PATH'); end
if isempty(library), library = getenv('PLS4ALL_LIB_PATH'); end
if isempty(library)
    error('nirs4all:MissingMethods', ...
        'Portable execution requires a Methods library; set N4M_LIBRARY_PATH.');
end
if ~isstruct(dataset) || ~isscalar(dataset)
    error('nirs4all:InvalidDataset', 'Portable execution requires a struct dataset.');
end
if ~isfield(dataset, 'schema') && (~isfield(dataset, 'X') || ~isfield(dataset, 'y'))
    error('nirs4all:InvalidDataset', 'Dataset must contain X and y fields.');
end
% Preserve scalar/vector shape at the JSON boundary, without parsing IO data.
if isfield(dataset, 'X') && isnumeric(dataset.X) && isscalar(dataset.X)
    dataset.X = {dataset.X};
end
if isfield(dataset, 'y') && isnumeric(dataset.y) && isvector(dataset.y)
    dataset.y = num2cell(dataset.y(:).');
end
payload = jsonencode(dataset, 'ConvertInfAndNaN', false);
request = ['{"schema_version":1,"definition":' recipeJson ',"dataset":' payload '}'];
result = ownerCall('portable-pipeline-run', request, ...
    struct('methods_library', library), false);
% Keep the established MATLAB cell-array presentation for ordered entries.
if isstruct(result.variants), result.variants = num2cell(result.variants); end
if isstruct(result.preprocessing), result.preprocessing = num2cell(result.preprocessing); end
if isempty(result.preprocessing), result.preprocessing = {}; end
end

function text = documentJSON(definition)
original = definition.source;
if ischar(original)
    trimmed = strtrim(original);
    if ~isempty(trimmed) && any(trimmed(1) == '[{')
        text = original;
        return
    end
    original = definition.original_root;
end
% MATLAB structs represent both maps and singleton arrays. Only the known
% document sequence shape is adapted; the owner interprets all recipe keys.
if isstruct(original) && isscalar(original)
    for key = {'pipeline', 'steps'}
        if isfield(original, key{1}) && isstruct(original.(key{1}))
            original.(key{1}) = num2cell(original.(key{1}));
        end
    end
elseif isstruct(original)
    original = num2cell(original);
end
text = jsonencode(original, 'ConvertInfAndNaN', false);
end

function result = ownerCall(operation, request, flags, admission)
try
    result = nirs4all.workflowCli('', operation, request, flags);
catch err
    if ~strcmp(err.identifier, 'nirs4all:NativeWorkflow'), rethrow(err); end
    detail = err.message;
    if admission && ~isempty(strfind(detail, '.params.n_train'))
        error('nirs4all:UnsupportedOperator', ...
            'Unsupported portable KennardStone parameter ''n_train'': training-count splits are not supported by this execution profile.');
    end
    if admission
        if ~isempty(strfind(detail, 'unsupported')) || ...
                ~isempty(strfind(detail, 'owner_route_required'))
            error('nirs4all:UnsupportedOperator', '%s', detail);
        end
        error('nirs4all:InvalidPipeline', '%s', detail);
    end
    if ~isempty(strfind(detail, '$.dataset'))
        error('nirs4all:InvalidDataset', '%s', detail);
    end
    rethrow(err);
end
end

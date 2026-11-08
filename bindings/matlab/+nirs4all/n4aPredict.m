function prediction = n4aPredict(model, varargin)
%N4APREDICT Predict with a canonical V1 .n4a container without any refit.
%   PREDICTION = nirs4all.n4aPredict(MODEL, 'record', PATH, 'partition', P, ...)
%   MODEL comes from n4aProduce, n4aInspect or n4aRetrain. With 'record' the
%   file is an external IO dataset (entrypoint 'predict-external'); without it
%   the training record embedded in the container is predicted in-sample
%   (entrypoint 'predict', partition 'train'), which is not a held-out score.
%
%   Rows of `values` follow the physical row order of the record exactly once,
%   keyed by the owner-provided sample IDs (`sampleIds`); a missing, repeated
%   or extra ID is refused. `targetNames` and `targetUnits` come from the owner
%   output binding. `replay` and `receipt` are the owner objects as decoded by
%   jsondecode; `nativeJson` is the exact native text.
%
%   Name-value options: 'record', 'partition' ('predict' external, 'train'
%   stored), 'entrypoint', 'sourceId' ('spectra'), 'methodsLibrary',
%   'methodsLibrarySha256', 'requestId', 'runId' and 'cli'. The Methods library
%   path and SHA-256 are mandatory.
%   'memoryBudgetBytes' (default none) is an optional allocation budget in
%   bytes, forwarded as the exact decimal text --memory-budget-bytes; see
%   nirs4all.n4aMemoryBudget. Without it no flag is sent and this host adds no
%   cap.
p = inputParser();
addParameter(p, 'record', ''); addParameter(p, 'partition', ''); addParameter(p, 'entrypoint', '');
addParameter(p, 'sourceId', ''); addParameter(p, 'cli', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
addParameter(p, 'methodsLibrarySha256', getenv('N4M_EXPECTED_LIBRARY_SHA256'));
addParameter(p, 'requestId', nirs4all.temporaryRunId('req:matlab:n4a-predict:'));
addParameter(p, 'runId', nirs4all.temporaryRunId('run:matlab:n4a-predict:'));
addParameter(p, 'memoryBudgetBytes', []);
parse(p, varargin{:}); options = p.Results;
budget = nirs4all.n4aMemoryBudget(options.memoryBudgetBytes);
if ~isstruct(model) || ~isfield(model, 'archive') || ~isfield(model, 'archiveSha256')
    error('nirs4all:N4aArgument', 'MODEL must come from n4aProduce, n4aInspect or n4aRetrain');
end
library = options.methodsLibrary;
if isempty(library), library = getenv('N4M_LIB_PATH'); end
if isempty(library) || isempty(options.methodsLibrarySha256)
    error('nirs4all:MethodsLibrary', ...
        ['methodsLibrary and methodsLibrarySha256 are required (or set N4M_LIBRARY_PATH ' ...
         'and N4M_EXPECTED_LIBRARY_SHA256).']);
end
external = ~isempty(options.record);
entrypoint = options.entrypoint; partition = options.partition; sourceId = options.sourceId;
if isempty(entrypoint), if external, entrypoint = 'predict-external'; else, entrypoint = 'predict'; end, end
if isempty(partition), if external, partition = 'predict'; else, partition = 'train'; end, end
if isempty(sourceId), if isfield(model, 'sourceId'), sourceId = model.sourceId; else, sourceId = 'spectra'; end, end
cli = options.cli;
if isempty(cli) && isfield(model, 'cli'), cli = model.cli; end
flags = struct('archive', model.archive, 'expected_archive_sha256', model.archiveSha256, ...
    'entrypoint', entrypoint, 'source_id', sourceId, 'partition', partition, ...
    'request_id', options.requestId, 'run_id', options.runId, ...
    'methods_library', nirs4all.n4aAbsolutePath(library), ...
    'methods_library_sha256', char(options.methodsLibrarySha256));
if external
    if ~ischar(options.record) || exist(options.record, 'file') ~= 2
        error('nirs4all:N4aRecord', ['The record must be the path of an existing IO dataset JSON file. ' ...
            'Structs and dataset objects are refused: re-encoding would change the record bytes.']);
    end
    flags.input = nirs4all.n4aAbsolutePath(options.record);
end
if ~isempty(budget), flags.memory_budget_bytes = budget; end
[report, nativeJson, cli] = nirs4all.n4aCall(cli, 'n4a-predict', flags);
prediction = struct('sampleIds', {report.sample_ids}, 'targetNames', {report.target_names}, ...
    'targetUnits', {report.target_units}, 'values', report.values, 'sourceId', report.source_id, ...
    'partition', report.partition, 'inputMode', report.input_mode, ...
    'archiveSha256', report.archive_sha256, 'replay', report.replay, 'receipt', report.receipt, ...
    'cli', cli, 'nativeJson', nativeJson);
end

function fresh = n4aRetrain(model, recordPath, archive, varargin)
%N4ARETRAIN Retrain a canonical V1 .n4a container through the Core owner.
%   FRESH = nirs4all.n4aRetrain(MODEL, RECORDPATH, ARCHIVE, ...) asks the
%   owner to retrain from the recipe stored in MODEL's container on the IO
%   record file RECORDPATH. No recipe is accepted here: the owner retrieves and
%   validates the stored one and performs a single production. MODEL is left
%   untouched; ARCHIVE receives the new container and FRESH is its reference.
%
%   The source ID and the recipe both come from the validated package, so
%   neither is an argument. Owner errors are raised as nirs4all:N4aRefusal with
%   the native message; there is no host-side retraining and no fallback.
%
%   Name-value options as for n4aProduce: 'methodsLibrary',
%   'methodsLibrarySha256', 'runId', 'containerId', 'embedTrainingRecord'
%   and 'cli'.
%   'memoryBudgetBytes' (default none) is an optional allocation budget in
%   bytes, forwarded as the exact decimal text --memory-budget-bytes; see
%   nirs4all.n4aMemoryBudget. Without it no flag is sent and this host adds no
%   cap.
p = inputParser();
addParameter(p, 'cli', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
addParameter(p, 'methodsLibrarySha256', getenv('N4M_EXPECTED_LIBRARY_SHA256'));
addParameter(p, 'runId', nirs4all.temporaryRunId('run:matlab:n4a-retrain:'));
addParameter(p, 'containerId', nirs4all.temporaryRunId('matlab-n4a-'));
addParameter(p, 'embedTrainingRecord', false); addParameter(p, 'memoryBudgetBytes', []);
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
if ~ischar(recordPath) || exist(recordPath, 'file') ~= 2
    error('nirs4all:N4aRecord', ['The record must be the path of an existing IO dataset JSON file. ' ...
        'Structs and dataset objects are refused: re-encoding would change the record bytes.']);
end
if islogical(options.embedTrainingRecord) || isnumeric(options.embedTrainingRecord)
    if options.embedTrainingRecord, embed = 'true'; else, embed = 'false'; end
else
    error('nirs4all:N4aArgument', 'embedTrainingRecord must be a logical scalar');
end
cli = options.cli;
if isempty(cli) && isfield(model, 'cli'), cli = model.cli; end
archive = nirs4all.n4aAbsolutePath(archive);
flags = struct('archive', model.archive, 'expected_archive_sha256', model.archiveSha256, ...
    'input', nirs4all.n4aAbsolutePath(recordPath), ...
    'run_id', options.runId, 'container_id', options.containerId, 'destination', archive, ...
    'embed_training_record', embed, 'methods_library', nirs4all.n4aAbsolutePath(library), ...
    'methods_library_sha256', char(options.methodsLibrarySha256));
if ~isempty(budget), flags.memory_budget_bytes = budget; end
[report, nativeJson, cli] = nirs4all.n4aCall(cli, 'n4a-retrain', flags);
fresh = struct('archive', archive, 'archiveSha256', report.archive_sha256, 'cli', cli, ...
    'sourceId', report.training_source_id, 'sourceArchiveSha256', report.source_archive_sha256, ...
    'trainingOutcome', report.training_outcome, 'report', report, 'nativeJson', nativeJson);
end

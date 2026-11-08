function model = n4aProduce(recordPath, recipe, archive, varargin)
%N4APRODUCE Train once and seal a canonical V1 .n4a container natively.
%   MODEL = nirs4all.n4aProduce(RECORDPATH, RECIPE, ARCHIVE, ...) passes the
%   exact bytes of an IO dataset JSON file and a native recipe to the Core
%   owner. RECORDPATH must be a file: structs and PublicDataset objects are
%   refused because re-encoding would change the record bytes. RECIPE is a
%   recipe file path, recipe JSON text, or a struct with `steps` and
%   `candidates`. The owners decide which methods, targets and profiles are
%   supported; the first qualified profile is standard_scale + ridge with
%   several ordered targets, not universal support.
%
%   Name-value options: 'sourceId' ('spectra'), 'methodsLibrary'
%   (N4M_LIBRARY_PATH), 'methodsLibrarySha256' (N4M_EXPECTED_LIBRARY_SHA256),
%   'runId', 'containerId', 'embedTrainingRecord' (false; needed for stored
%   in-sample prediction) and 'cli' (NIRS4ALL_CORE_CLI). The Methods library
%   path and its SHA-256 are mandatory. ARCHIVE must not exist.
%   'memoryBudgetBytes' (default none) is an optional allocation budget in
%   bytes, forwarded as the exact decimal text --memory-budget-bytes; see
%   nirs4all.n4aMemoryBudget. Without it no flag is sent and this host adds no
%   cap.
p = inputParser();
addParameter(p, 'sourceId', 'spectra'); addParameter(p, 'cli', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
addParameter(p, 'methodsLibrarySha256', getenv('N4M_EXPECTED_LIBRARY_SHA256'));
addParameter(p, 'runId', nirs4all.temporaryRunId('run:matlab:n4a-produce:'));
addParameter(p, 'containerId', nirs4all.temporaryRunId('matlab-n4a-'));
addParameter(p, 'embedTrainingRecord', false); addParameter(p, 'memoryBudgetBytes', []);
parse(p, varargin{:}); options = p.Results;
budget = nirs4all.n4aMemoryBudget(options.memoryBudgetBytes);
[library, sha] = methodsRuntime(options);
recordPath = recordFile(recordPath);
archive = nirs4all.n4aAbsolutePath(archive);
directory = tempname(); mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's')); %#ok<NASGU>
if ischar(recipe) && exist(recipe, 'file') == 2
    recipePath = nirs4all.n4aAbsolutePath(recipe);
else
    recipePath = fullfile(directory, 'recipe.json');
    file = fopen(recipePath, 'w');
    if file < 0, error('nirs4all:N4aArgument', 'Cannot stage the recipe'); end
    fprintf(file, '%s', nirs4all.nativeRecipeJSON(recipe)); fclose(file);
end
flags = struct('input', recordPath, 'recipe', recipePath, 'source_id', options.sourceId, ...
    'methods_library', library, 'methods_library_sha256', sha, 'run_id', options.runId, ...
    'container_id', options.containerId, 'archive', archive, ...
    'embed_training_record', logicalText(options.embedTrainingRecord));
if ~isempty(budget), flags.memory_budget_bytes = budget; end
[report, nativeJson, cli] = nirs4all.n4aCall(options.cli, 'n4a-produce', flags);
model = struct('archive', archive, 'archiveSha256', report.archive_sha256, 'cli', cli, ...
    'sourceId', options.sourceId, 'trainingOutcome', report.training_outcome, ...
    'report', report, 'nativeJson', nativeJson);
end

function [library, sha] = methodsRuntime(options)
library = options.methodsLibrary;
if isempty(library), library = getenv('N4M_LIB_PATH'); end
sha = options.methodsLibrarySha256;
if isempty(library) || isempty(sha)
    error('nirs4all:MethodsLibrary', ...
        ['methodsLibrary and methodsLibrarySha256 are required (or set N4M_LIBRARY_PATH ' ...
         'and N4M_EXPECTED_LIBRARY_SHA256).']);
end
library = nirs4all.n4aAbsolutePath(library); sha = char(sha);
end

function path = recordFile(path)
if ~ischar(path) || exist(path, 'file') ~= 2
    error('nirs4all:N4aRecord', ['The record must be the path of an existing IO dataset JSON file. ' ...
        'Structs and dataset objects are refused: re-encoding would change the record bytes.']);
end
path = nirs4all.n4aAbsolutePath(path);
end

function text = logicalText(value)
if ~(islogical(value) || isnumeric(value)) || ~isscalar(value)
    error('nirs4all:N4aArgument', 'embedTrainingRecord must be a logical scalar');
end
if value, text = 'true'; else, text = 'false'; end
end

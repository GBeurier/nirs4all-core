function result = resultView(directory, cli)
%RESULTVIEW Reopen a checked native result projection from an experiment.
%   RESULT = nirs4all.resultView(DIRECTORY) verifies the fixed native-results
%   members and their JSON projection. Scores and predictions are retained
%   exactly as emitted by dag-ml; this function does not fit or score models.

if ~(ischar(directory) || (isstring(directory) && isscalar(directory)))
    error('nirs4all:resultView:Path', 'directory must be a path');
end
directory = char(directory);
if nargin < 2 || isempty(cli), cli = getenv('NIRS4ALL_CORE_CLI'); end
if isempty(cli), cli = 'nirs4all-core-archive'; end
index = readJson(fullfile(directory, 'experiment.json'));
if ~isstruct(index) || ~isfield(index, 'schema') || ...
        ~strcmp(index.schema, 'nirs4all.experiment.v1') || ...
        ~isfield(index, 'results') || ~isfield(index, 'result_view_sha256') || ...
        ~isfield(index, 'run_id') || ~isfield(index, 'winner_variant_id')
    error('nirs4all:resultView:Index', 'invalid experiment index');
end
names = {'manifest.json', 'score_set.json', 'predictions.parquet'};
for k = 1:numel(names)
    name = names{k};
    key = regexprep(name, '[^A-Za-z0-9_]', '_');
    if ~isfield(index.results, key)
        error('nirs4all:resultView:Index', 'missing result inventory entry');
    end
    checkHash(fullfile(directory, 'results', name), index.results.(key));
end
viewPath = fullfile(directory, 'result_view.json');
checkHash(viewPath, index.result_view_sha256);
view = readJson(viewPath);
manifest = readJson(fullfile(directory, 'results', 'manifest.json'));
scores = readJson(fullfile(directory, 'results', 'score_set.json'));
scoreHash = fileHash(fullfile(directory, 'results', 'score_set.json'));
if ~isequaln(view.manifest, manifest) || ~isequaln(view.score_set, scores) || ...
        manifest.schema_version ~= 2 || ~strcmp(manifest.engine, 'dag-ml') || ...
        ~strcmp(manifest.score_set_hash, scoreHash) || ...
        ~isfield(manifest, 'run_id') || ~strcmp(manifest.run_id, index.run_id) || ...
        ~isfield(manifest, 'selected_variant_id') || isempty(manifest.selected_variant_id) || ...
        ~strcmp(manifest.selected_variant_id, index.winner_variant_id)
    error('nirs4all:resultView:Native', 'native result projection is inconsistent');
end
if ~isfield(view, 'predictions') || ~isfield(scores, 'reports')
    error('nirs4all:resultView:Native', 'native result rows are missing');
end
rows = view.predictions;
reports = scores.reports;
variants = {};
for k = 1:numel(rows)
    row = rows(k);
    variants{end+1} = row.variant_id; %#ok<AGROW>
    if isfield(row, 'sample_ids') && ~isempty(row.sample_ids)
        ids = row.sample_ids;
        if numel(ids) ~= numel(row.sample_indices) || numel(unique(ids)) ~= numel(ids)
            error('nirs4all:resultView:Identity', 'prediction sample IDs are invalid');
        end
    end
end
for k = 1:numel(reports)
    if iscell(reports), report = reports{k}; else, report = reports(k); end
    if isfield(report, 'variant_id')
        variants{end+1} = report.variant_id; %#ok<AGROW>
    end
end
if ~any(strcmp(variants, index.winner_variant_id))
    error('nirs4all:resultView:Winner', 'winner is absent from native results');
end
modelArchive = '';
if isfield(index, 'model_archive') && ~isempty(index.model_archive)
    if ~strcmp(index.model_archive.path, 'model.n4a')
        error('nirs4all:resultView:Archive', 'invalid Core model archive reference');
    end
    modelArchive = fullfile(directory, 'model.n4a');
    checkHash(modelArchive, index.model_archive.sha256);
end
nativeOutput = [tempname() '.json'];
nativeCleanup = onCleanup(@() deleteIfExists(nativeOutput)); %#ok<NASGU>
args = {char(cli), 'experiment-open', '--input', directory, '--output', nativeOutput};
quoted = cellfun(@shellQuote, args, 'UniformOutput', false);
[status, detail] = system([strjoin(quoted, ' ') ' 2>&1']);
if status ~= 0
    error('nirs4all:resultView:Native', 'native experiment validation failed: %s', strtrim(detail));
end
native = readJson(nativeOutput);
if ~isequaln(view.manifest, native.manifest) || ...
        ~isequaln(view.score_set, native.score_set) || ...
        ~isequaln(view.predictions, native.predictions)
    error('nirs4all:resultView:Native', 'projection disagrees with native Parquet results');
end
validationLevel = 'native_results';
if ~isempty(modelArchive), validationLevel = 'native_results_and_model_prediction_closure'; end
result = struct('run_id', index.run_id, ...
    'winner_variant_id', index.winner_variant_id, ...
    'variant_ids', {unique(variants)}, ...
    'score_set', scores, 'predictions', rows, ...
    'model_archive', modelArchive, ...
    'validation_level', validationLevel, ...
    'unattested_display_fields', {{'dataset', 'task_type'}});
end

function deleteIfExists(path)
if exist(path, 'file'), delete(path); end
end

function value = shellQuote(value)
if ispc
    if any(value == '"') || any(value == '%') || any(value == newline)
        error('nirs4all:resultView:Argument', 'unsupported Windows argument character');
    end
    value = ['"' value '"'];
else
    value = ['''' strrep(value, '''', '''"''"''') ''''];
end
end

function value = readJson(path)
fid = fopen(path, 'rb');
if fid < 0
    error('nirs4all:resultView:File', 'experiment member is missing');
end
cleanup = onCleanup(@() fclose(fid));
bytes = fread(fid, Inf, '*uint8');
value = jsondecode(char(bytes.'));
end

function checkHash(path, expected)
if ~strcmp(fileHash(path), expected)
    error('nirs4all:resultView:Hash', 'experiment member hash mismatch');
end
end

function digest = fileHash(path)
fid = fopen(path, 'rb');
if fid < 0
    error('nirs4all:resultView:File', 'experiment member is missing');
end
cleanup = onCleanup(@() fclose(fid));
bytes = fread(fid, Inf, '*uint8');
if exist('hash', 'builtin') || exist('hash', 'file')
    digest = hash('sha256', char(bytes.'));
else
    md = java.security.MessageDigest.getInstance('SHA-256');
    md.update(bytes);
    raw = typecast(md.digest(), 'uint8');
    digest = lower(reshape(dec2hex(raw, 2).', 1, []));
end
end

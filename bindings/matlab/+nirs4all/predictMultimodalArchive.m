function result = predictMultimodalArchive(path, value, options)
%PREDICTMULTIMODALARCHIVE Native .n4a replay on a target-free IO dataset.
% IO assembles the cohort; DAG signs and schedules; Methods hydrates/predicts.
if nargin < 3, options = struct(); end
coreCli = option(options, 'core_cli', 'NIRS4ALL_CORE_CLI', 'nirs4all-core-archive');
dagCli = option(options, 'dag_cli', 'NIRS4ALL_DAG_CLI', 'dag-ml-cli');
octave = option(options, 'octave', 'NIRS4ALL_OCTAVE', 'octave');
adapterDir = option(options, 'adapter_directory', 'DAG_ML_OCTAVE_ADAPTER_PATH', '');
if isempty(adapterDir) || exist(fullfile(adapterDir, 'octave_methods_multimodal_adapter.m'), 'file') ~= 2
    error('nirs4all:RawReplay', 'An installed DAG Octave multimodal adapter directory is required');
end
if isa(value, 'nirs4all.PublicDataset'), ds = value; else, ds = nirs4all.dataset(value, coreCli); end
directory = tempname(); mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's')); %#ok<NASGU>
input = fullfile(directory, 'dataset.json'); writeText(input, ds.toJSON());
preparedPath = fullfile(directory, 'prepared.json'); nativeDir = fullfile(directory, 'native-inputs');
invoke({coreCli, 'multimodal-replay-inputs', '--archive', path, '--input', input, '--output', preparedPath, '--workdir', nativeDir});
prepared = jsondecode(fileread(preparedPath));
config = fullfile(nativeDir, 'config.json');
adapter = fullfile(directory, 'run-octave-adapter');
describe = '{"schema_version":1,"protocol":"dag-ml-process-adapter","adapter_id":"nirs4all-octave-core-multimodal","supported_modes":["jsonl"],"capabilities":["node_task_json_v1","node_result_json_v1","control_frames_v1","persistent_workers","worker_env","stateful_refit_artifacts","portable_artifact_bridge_v1"]}';
evalText = sprintf('addpath(''%s''); octave_methods_multimodal_adapter();', strrep(adapterDir, '''', ''''''));
text = sprintf('#!/bin/sh\nif [ "$1" = "--describe" ]; then printf ''%%s\\n'' %s; exit 0; fi\nif [ "$1" != "--jsonl" ]; then exit 2; fi\nexport DAGML_METHODS_MULTIMODAL_CONFIG=%s\nexec %s --quiet --no-gui --eval %s\n', ...
    quote(describe), quote(config), quote(octave), quote(evalText));
writeText(adapter, text); invoke({'chmod', '0755', adapter});
output = fullfile(directory, 'outcome.json');
invoke({coreCli, 'replay-process', '--archive', path, '--expected-archive-sha256', prepared.archive_sha256, ...
    '--dag-cli', dagCli, '--request', fullfile(nativeDir, 'request.json'), ...
    '--envelopes', fullfile(nativeDir, 'envelopes.json'), ...
    '--trusted-controllers', fullfile(nativeDir, 'trusted-controllers.json'), ...
    '--adapter', adapter, '--output', output, '--outcome-id', 'outcome:public.octave.multimodal', ...
    '--run-id', 'run:public.octave.multimodal', '--process-timeout-ms', '60000'});
outcome = jsondecode(fileread(output));
if numel(outcome.outputs) ~= 1 || numel(outcome.outputs.predictions) ~= 1
    error('nirs4all:RawReplay', 'One complete native sample prediction required');
end
block = outcome.outputs.predictions;
sampleIds = cellstr(prepared.sample_ids); nativeIds = cellstr(block.sample_ids);
[found, positions] = ismember(sampleIds, nativeIds);
if ~all(found) || numel(unique(nativeIds)) ~= numel(nativeIds) || numel(nativeIds) ~= numel(sampleIds)
    error('nirs4all:RawReplay', 'Native prediction identity coverage differs');
end
values = double(block.values); values = values(:);
if numel(values) ~= numel(sampleIds) || any(~isfinite(values)), error('nirs4all:RawReplay', 'Finite native scalar predictions required'); end
lines = strsplit(strtrim(fileread(fullfile(nativeDir, 'lifecycle.jsonl'))), '\n');
audit = cellfun(@jsondecode, lines, 'UniformOutput', false);
operations = cellfun(@(event) event.operation, audit, 'UniformOutput', false);
if any(ismember(operations, {'FIT_CV','REFIT','fit'})), error('nirs4all:RawReplay', 'Cold replay unexpectedly trained'); end
result = struct('sample_ids', {sampleIds}, 'target_names', {cellstr(block.target_names)}, 'values', values(positions), ...
    'training_performed', false, 'outcome', outcome, 'audit', {audit}, 'archive_sha256', prepared.archive_sha256);
end

function value = option(options, name, environment, fallback)
if isfield(options, name), value = options.(name); else, value = getenv(environment); end
if isempty(value), value = fallback; end
end
function invoke(args)
encoded = cellfun(@quote, args, 'UniformOutput', false);
[status, detail] = system([strjoin(encoded, ' ') ' 2>&1']);
if status ~= 0, error('nirs4all:RawReplay', 'Native replay refused: %s', detail); end
end
function text = quote(text)
if ~ischar(text) || any(text == char(0)), error('nirs4all:RawReplay', 'Invalid native argument'); end
text = ['''' strrep(text, '''', '''"''"''') ''''];
end
function writeText(path, text)
fid = fopen(path, 'wb'); if fid < 0, error('nirs4all:RawReplay', 'Cannot write transport'); end
cleanup = onCleanup(@() fclose(fid)); %#ok<NASGU>
fwrite(fid, unicode2native(text, 'UTF-8'), 'uint8');
end

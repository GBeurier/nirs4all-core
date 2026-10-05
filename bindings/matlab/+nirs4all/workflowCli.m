function [result, nativeJson] = workflowCli(cli, operation, record, flags)
%WORKFLOWCLI Transport strict JSON to the native aggregate workflow CLI.
if isempty(cli), cli = getenv('NIRS4ALL_CORE_CLI'); end
if isempty(cli), cli = 'nirs4all-core-archive'; end
directory = tempname(); mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's')); %#ok<NASGU>
input = fullfile(directory, 'input.json'); output = fullfile(directory, 'output.json');
file = fopen(input, 'w');
if file < 0, error('nirs4all:WorkflowIO', 'Cannot write workflow request'); end
if isa(record, 'nirs4all.PublicDataset'), payload = record.toJSON();
elseif ischar(record), payload = record;
else, payload = jsonencode(record); end
fprintf(file, '%s', payload); fclose(file);
args = {char(cli), operation, '--output', output};
if ~isempty(record), args=[args {'--input', input}]; end
names = fieldnames(flags);
for k = 1:numel(names)
    args{end+1} = ['--' strrep(names{k}, '_', '-')]; %#ok<AGROW>
    args{end+1} = char(flags.(names{k})); %#ok<AGROW>
end
quoted = cellfun(@shellQuote, args, 'UniformOutput', false);
[status, detail] = system([strjoin(quoted, ' ') ' 2>&1']);
if status ~= 0, error('nirs4all:NativeWorkflow', '%s', strtrim(detail)); end
nativeJson = fileread(output);
result = jsondecode(nativeJson);
end

function value = shellQuote(value)
if ispc
    if any(value == '"') || any(value == '%') || any(value == newline)
        error('nirs4all:WorkflowArgument', 'Unsupported Windows argument character');
    end
    value = ['"' value '"'];
else
    value = ['''' strrep(value, '''', '''"''"''') ''''];
end
end

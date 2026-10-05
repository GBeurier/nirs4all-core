classdef PublicDataset
    % Thin host projection; IO owns identity alignment and dataset validation.
    properties (SetAccess = private)
        record
        json
        coreCli
    end
    methods
        function obj = PublicDataset(value, coreCli)
            if nargin < 2, coreCli = 'nirs4all-core-archive'; end
            obj.coreCli = coreCli;
            obj.json = obj.invoke(value, 'dataset-normalize');
            obj.record = jsondecode(obj.json);
        end
        function result = sampleIds(obj)
            result = obj.record.dataset.sample_ids;
        end
        function result = rawSources(obj)
            result = jsondecode(obj.invoke(obj.json, 'dataset-u07-sources'));
        end
        function result = toJSON(obj)
            result = obj.json;
        end
        function result = invoke(obj, value, command)
            directory = tempname(); mkdir(directory);
            cleanup = onCleanup(@() rmdir(directory, 's')); %#ok<NASGU>
            input = fullfile(directory, 'input.json'); output = fullfile(directory, 'output.json');
            if ischar(value) && exist(value, 'file') == 2
                copyfile(value, input);
            else
                if isstruct(value), text = jsonencode(value); elseif ischar(value), text = value; else, error('nirs4all:dataset', 'Dataset must be an IO record, JSON text or JSON file'); end
                fid = fopen(input, 'wb'); if fid < 0, error('nirs4all:dataset', 'Cannot write dataset input'); end
                fileCleanup = onCleanup(@() fclose(fid)); fwrite(fid, unicode2native(text, 'UTF-8'), 'uint8'); clear fileCleanup;
            end
            args = sprintf('%s %s --input %s --output %s', quote(obj.coreCli), command, quote(input), quote(output));
            [status, detail] = system(args);
            if status ~= 0, error('nirs4all:dataset', 'Native IO validation refused: %s', detail); end
            result = fileread(output);
        end
    end
end
function result = quote(value)
% POSIX shell transport matching the existing native CLI requirement.
if ~ischar(value) || isempty(value) || any(value == char(0)), error('nirs4all:dataset', 'Invalid CLI path'); end
result = ['''' strrep(value, '''', '''"''"''') ''''];
end

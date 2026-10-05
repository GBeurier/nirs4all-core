classdef MultimodalPredictor < handle
    % Complete native N4MF predictor. Encoders and regression live in Methods.
    properties (SetAccess = private)
        recipe
        sourceSchemas
        targetNames
        coreCli
    end
    properties (Access = private)
        native
    end
    methods
        function result = predict(obj, value)
            ds = nirs4all.dataset(value, obj.coreCli);
            if ~isempty(ds.record.dataset.y) || any(~strcmp(ds.record.dataset.partitions.values, 'predict'))
                error('nirs4all:multimodal', 'Prediction requires a target-free predict cohort');
            end
            raw = ds.rawSources();
            current = raw.source_schemas; saved = obj.sourceSchemas;
            for collection = {'current','saved'}
                if strcmp(collection{1},'current'), schemas = current; else, schemas = saved; end
                names = fieldnames(schemas);
                for i = 1:numel(names), schemas.(names{i}).input_shape = num2cell(schemas.(names{i}).input_shape(:).'); end
                if strcmp(collection{1},'current'), current = schemas; else, saved = schemas; end
            end
            saved = jsondecode(ds.invoke(struct('current',current,'saved',saved), 'dataset-compatible-schemas'));
            values = obj.native.predict(blocks(raw), saved);
            result = struct('sample_ids', {raw.sample_ids}, 'target_names', {obj.targetNames}, 'values', values);
        end
        function result = export(obj, path)
            state = obj.native.exportState();
            schemas = obj.sourceSchemas; names = fieldnames(schemas);
            for i = 1:numel(names), schemas.(names{i}).input_shape = num2cell(schemas.(names{i}).input_shape(:).'); end
            recipeRecord = obj.recipe; encoders = fieldnames(recipeRecord.encoders);
            for i = 1:numel(encoders)
                name = encoders{i};
                if isfield(recipeRecord.encoders.(name), 'drop') && isempty(recipeRecord.encoders.(name).drop), recipeRecord.encoders.(name).drop = NaN; end
                for key = {'numeric_columns', 'categorical_columns'}
                    if isfield(recipeRecord.encoders.(name), key{1}), recipeRecord.encoders.(name).(key{1}) = num2cell(recipeRecord.encoders.(name).(key{1})(:).'); end
                end
            end
            record = struct('schema', 'nirs4all.multimodal-predictor.v1', 'schema_version', 1, ...
                'recipe', recipeRecord, 'source_schemas', schemas, 'state', double(state(:).'), 'target_names', {obj.targetNames});
            result = jsonencode(record);
            if nargin >= 2
                if exist(path, 'file') == 2, error('nirs4all:multimodal', 'Predictor output already exists'); end
                fid = fopen(path, 'wb'); if fid < 0, error('nirs4all:multimodal', 'Cannot create predictor output'); end
                cleanup = onCleanup(@() fclose(fid)); %#ok<NASGU>
                fwrite(fid, unicode2native(result, 'UTF-8'), 'uint8');
            end
        end
        function close(obj)
            if ~isempty(obj.native), obj.native.close(); obj.native = []; end
        end
        function delete(obj), obj.close(); end
    end
    methods (Static)
        function obj = fit(recipe, value, coreCli)
            if nargin < 3, coreCli = 'nirs4all-core-archive'; end
            ds = nirs4all.dataset(value, coreCli); record = ds.record.dataset;
            if isempty(record.y) || numel(record.y.shape) ~= 1 || ~all(record.target_mask.values) || ...
                    any(~isfinite(record.y.values)) || any(~strcmp(record.partitions.values, 'train'))
                error('nirs4all:multimodal', 'Full fit requires training rows and one finite observed numeric target');
            end
            raw = ds.rawSources();
            native = n4m.MultimodalPipeline(recipe, raw.source_schemas);
            try
                native.fit(blocks(raw), record.y.values);
                obj = nirs4all.MultimodalPredictor(); obj.native = native;
                obj.recipe = recipe; obj.sourceSchemas = raw.source_schemas; obj.targetNames = record.target_names; obj.coreCli = ds.coreCli;
            catch exception
                native.close(); rethrow(exception);
            end
        end
        function obj = load(value, coreCli)
            if nargin < 2, coreCli = 'nirs4all-core-archive'; end
            if ischar(value) && exist(value, 'file') == 2, record = jsondecode(fileread(value)); elseif ischar(value), record = jsondecode(value); else, record = value; end
            fields = {'schema'; 'schema_version'; 'recipe'; 'source_schemas'; 'state'; 'target_names'};
            if ~isstruct(record) || ~isequal(sort(fieldnames(record)), sort(fields)) || ~strcmp(record.schema, 'nirs4all.multimodal-predictor.v1') || ...
                record.schema_version ~= 1 || ~isnumeric(record.state) || isempty(record.state) || numel(record.state) > 67108864 || ...
                any(~isfinite(record.state(:)) | record.state(:) < 0 | record.state(:) > 255 | record.state(:) ~= fix(record.state(:))) || ...
                ~iscell(record.target_names) || numel(record.target_names) ~= 1 || ~ischar(record.target_names{1}) || isempty(strtrim(record.target_names{1}))
                error('nirs4all:multimodal', 'Invalid multimodal predictor envelope');
            end
            native = n4m.MultimodalPipeline.fromState(uint8(record.state), record.recipe, record.source_schemas);
            obj = nirs4all.MultimodalPredictor(); obj.native = native;
            obj.recipe = record.recipe; obj.sourceSchemas = record.source_schemas; obj.targetNames = record.target_names; obj.coreCli = coreCli;
        end
    end
end
function out = blocks(raw)
names = fieldnames(raw.sources); out = struct();
for i = 1:numel(names)
    name = names{i}; source = raw.sources.(name);
    if strcmp(name, 'metadata')
        rows = source.rows;
        if ~iscell(rows), error('nirs4all:multimodal', 'Mixed metadata cells required'); end
        if size(rows, 2) == 2 && ~iscell(rows{1}), out.(name) = rows;
        else
            out.(name) = cell(numel(rows), 2);
            for j = 1:numel(rows), out.(name)(j, :) = rows{j}(:).'; end
        end
    else
        shape = double(source.shape(:).');
        values = source.data; if strcmp(source.descriptor.dtype, 'float32'), values = single(values); else, values = double(values); end
        out.(name) = permute(reshape(values, fliplr(shape)), numel(shape):-1:1);
    end
end
end

classdef NativePipeline
    %NATIVEPIPELINE Native DAG CV/OOF/refit over a live Methods catalog recipe.
    % JSON Package V2 transport; this product envelope is distinct from .n4a.
    properties (SetAccess=private)
        nativeJson
        config
        outcome
        coreCli
    end
    methods (Access=private)
        function self = NativePipeline(nativeJson, cli)
            record = jsondecode(nativeJson);
            self.nativeJson = nativeJson; self.config = record.config;
            self.outcome = record.training_outcome; self.coreCli = cli;
        end
    end
    methods (Static)
        function self = fit(recipe, dataset, varargin)
            p = inputParser;
            addParameter(p, 'sourceId', 'spectra'); addParameter(p, 'cli', '');
            addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
            parse(p, varargin{:}); options = p.Results;
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
            if isa(dataset, 'nirs4all.PublicDataset'), dataset = dataset.toJSON();
            elseif ~ischar(dataset), dataset = jsonencode(dataset); end
            record = ['{"dataset":' dataset ',"pipeline":' jsonencode(recipe) '}'];
            [~, id] = fileparts(tempname());
            flags = struct('source_id', options.sourceId, 'methods_library', options.methodsLibrary, 'run_id', ['run:matlab:pipeline:' id]);
            [~, nativeJson, cli] = nirs4all.workflowCli(options.cli, 'pipeline-run', record, flags);
            self = nirs4all.NativePipeline(nativeJson, cli);
        end
        function self = load(path, varargin)
            p = inputParser; addParameter(p, 'cli', ''); parse(p, varargin{:});
            [~, nativeJson, cli] = nirs4all.workflowCli(p.Results.cli, 'pipeline-load', fileread(path), struct());
            self = nirs4all.NativePipeline(nativeJson, cli);
        end
    end
    methods
        function result = predict(self, X, varargin)
            p = inputParser; addParameter(p, 'sampleIds', {});
            addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
            parse(p, varargin{:}); options = p.Results;
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
            if isempty(options.sampleIds), options.sampleIds = arrayfun(@(i) sprintf('predict:%d', i), 1:size(X,1), 'UniformOutput', false); end
            rows = mat2cell(X, ones(size(X,1),1), size(X,2));
            record = ['{"model":' self.nativeJson ',"x":' jsonencode(rows) ',"sample_ids":' jsonencode(options.sampleIds) '}'];
            [~, id] = fileparts(tempname());
            result = nirs4all.workflowCli(self.coreCli, 'pipeline-predict', record, struct('methods_library', options.methodsLibrary, 'run_id', ['run:matlab:predict:' id]));
        end
        function path = export(self, path)
            nirs4all.workflowCli(self.coreCli, 'pipeline-export', self.nativeJson, struct('destination', path));
        end
        function model = retrain(self, dataset, varargin)
            model = nirs4all.NativePipeline.fit(self.config.pipeline, dataset, 'sourceId', self.config.source_id, 'cli', self.coreCli, varargin{:});
        end
    end
end

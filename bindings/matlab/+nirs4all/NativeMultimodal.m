classdef NativeMultimodal
    %NATIVEMULTIMODAL Native DAG CV/OOF/refit over a live Methods catalog recipe.
    % JSON Package V2 transport; this product envelope is distinct from .n4a.
    properties (SetAccess=private)
        nativeJson
        config
        outcomes
        coreCli
        methodsLibrary
    end
    methods (Access=private)
        function self = NativeMultimodal(nativeJson, cli, library)
            record = jsondecode(nativeJson);
            self.nativeJson = nativeJson; self.config = record.config;
            self.outcomes = arrayfun(@(entry) entry.model.training_outcome, record.target_models, 'UniformOutput', false); self.coreCli = cli; self.methodsLibrary = library;
        end
    end
    methods (Static)
        function self = fit(recipe, sourcePolicies, dataset, varargin)
            p = inputParser;
            addParameter(p, 'cli', '');
            addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
            parse(p, varargin{:}); options = p.Results;
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
            if isa(dataset, 'nirs4all.PublicDataset'), dataset = dataset.toJSON();
            elseif ~ischar(dataset), dataset = jsonencode(dataset); end
            if ~iscell(sourcePolicies), sourcePolicies = arrayfun(@(policy) policy, sourcePolicies, 'UniformOutput', false); end
            record = ['{"dataset":' dataset ',"pipeline":' nirs4all.nativeRecipeJSON(recipe) ',"source_policies":' jsonencode(sourcePolicies) '}'];
            [~, id] = fileparts(tempname());
            flags = struct('methods_library', options.methodsLibrary, 'run_id', ['run:matlab:pipeline:' id]);
            [~, nativeJson, cli] = nirs4all.workflowCli(options.cli, 'native-multimodal-run', record, flags);
            self = nirs4all.NativeMultimodal(nativeJson, cli, options.methodsLibrary);
        end
        function self = load(path, varargin)
            p = inputParser; addParameter(p, 'cli', '');
            addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH')); parse(p, varargin{:}); options = p.Results;
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
            [~, nativeJson, cli] = nirs4all.workflowCli(p.Results.cli, 'native-multimodal-load', fileread(path), struct());
            self = nirs4all.NativeMultimodal(nativeJson, cli, options.methodsLibrary);
        end
    end
    methods
        function result = predict(self, dataset, varargin)
            p = inputParser;
            addParameter(p, 'methodsLibrary', self.methodsLibrary);
            parse(p, varargin{:}); options = p.Results;
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIBRARY_PATH'); end
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
            if isa(dataset, 'nirs4all.PublicDataset'), dataset = dataset.toJSON();
            elseif ~ischar(dataset), dataset = jsonencode(dataset); end
            record = ['{"model":' self.nativeJson ',"dataset":' dataset '}'];
            [~, id] = fileparts(tempname());
            result = nirs4all.workflowCli(self.coreCli, 'native-multimodal-predict', record, struct('methods_library', options.methodsLibrary, 'run_id', ['run:matlab:predict:' id]));
        end
        function path = export(self, path)
            nirs4all.workflowCli(self.coreCli, 'native-multimodal-export', self.nativeJson, struct('destination', path));
        end
        function model = retrain(self, dataset, varargin)
            p = inputParser;
            addParameter(p, 'methodsLibrary', self.methodsLibrary);
            parse(p, varargin{:}); options = p.Results;
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIBRARY_PATH'); end
            if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
            if isa(dataset, 'nirs4all.PublicDataset'), dataset = dataset.toJSON();
            elseif ~ischar(dataset), dataset = jsonencode(dataset); end
            record = ['{"model":' self.nativeJson ',"dataset":' dataset '}'];
            [~, id] = fileparts(tempname());
            [~, nativeJson, cli] = nirs4all.workflowCli(self.coreCli, 'native-multimodal-retrain', record, ...
                struct('methods_library', options.methodsLibrary, 'run_id', ['run:matlab:retrain:' id]));
            model = nirs4all.NativeMultimodal(nativeJson, cli, options.methodsLibrary);
        end
    end
end

function result = robustness(model, X, y, sampleIds, varargin)
%ROBUSTNESS Audit frozen native observed/Gaussian replay with keyed truth.
p = inputParser(); addParameter(p, 'cli', '');
addParameter(p, 'sourceId', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
addParameter(p, 'scenarios', {struct('id','observed','kind','observed','severity',0,'seed',0), ...
    struct('id','gaussian','kind','spectral_noise','severity',0.01,'seed',1)});
parse(p, varargin{:}); o = p.Results;
if isempty(o.methodsLibrary), o.methodsLibrary = getenv('N4M_LIB_PATH'); end
if isstruct(model), archive = model.archive; else, archive = model; end
values = arrayfun(@(i) num2cell(y(i,:)), (1:size(y,1)).', 'UniformOutput', false);
if nargin < 4, sampleIds = {}; end
record = nirs4all.uncertaintyInput(X, sampleIds, o.sourceId, struct('truth',{values},'scenarios',{o.scenarios}), o.cli);
flags = struct('archive', archive, 'methods_library', o.methodsLibrary, ...
    'run_id', ['run:robustness:' strrep(tempname(), filesep, '_')]);
result = nirs4all.workflowCli(o.cli, 'robustness', record, flags);
end

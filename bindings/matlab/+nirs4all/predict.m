function outcome = predict(workflow, X, varargin)
%PREDICT Replay a native Archive V2 using the supplied feature matrix only.
p = inputParser(); addParameter(p, 'sampleIds', {}); addParameter(p, 'cli', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH')); parse(p, varargin{:}); options=p.Results;
if isempty(options.methodsLibrary), options.methodsLibrary=getenv('N4M_LIB_PATH'); end
if isempty(options.methodsLibrary), error('nirs4all:MethodsLibrary', 'methodsLibrary is required'); end
if isempty(options.sampleIds), options.sampleIds=arrayfun(@(i) sprintf('sample.%d',i-1),1:size(X,1),'UniformOutput',false); end
if ~isnumeric(X) || ndims(X)~=2 || any(~isfinite(X(:))), error('nirs4all:Features','Finite numeric matrix required'); end
% Cell rows preserve the samples-by-features JSON shape for one-row matrices.
rows = arrayfun(@(i) num2cell(double(X(i,:))), 1:size(X,1), 'UniformOutput',false);
record=struct('x',{rows},'sample_ids',{options.sampleIds});
flags=struct('archive',workflow.archive,'methods_library',options.methodsLibrary,'run_id','run:matlab:predict');
outcome=nirs4all.workflowCli(options.cli,'workflow-predict',record,flags);
end

function result = predictCalibrated(model, X, sampleIds, varargin)
%PREDICTCALIBRATED Replay native intervals for an independent identified cohort.
p = inputParser(); addParameter(p, 'cli', '');
addParameter(p, 'sourceId', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH')); parse(p, varargin{:});
o = p.Results;
if isempty(o.cli) && isstruct(model) && isfield(model,'cli'), o.cli = model.cli; end
if isempty(o.methodsLibrary), o.methodsLibrary = getenv('N4M_LIB_PATH'); end
if isstruct(model), archive = model.archive; else, archive = model; end
flags = struct('archive', archive, 'methods_library', o.methodsLibrary, ...
    'run_id', ['run:calibrated:predict:' strrep(tempname(), filesep, '_')]);
if nargin < 3, sampleIds = {}; end
record = nirs4all.uncertaintyInput(X, sampleIds, o.sourceId, struct(), o.cli);
[result, nativeJson] = nirs4all.workflowCli(o.cli, 'conformal-predict', record, flags);
result.native_json = nativeJson;
end

function result = calibrate(model, dataset, varargin)
%CALIBRATE Attach held-out native split calibration to a frozen archive.
p = inputParser();
addParameter(p, 'coverages', 0.9); addParameter(p, 'smallSamplePolicy', 'error');
addParameter(p, 'sourceId', 'spectra'); addParameter(p, 'cli', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
addParameter(p, 'archive', [tempname() '.n4a']);
parse(p, varargin{:}); o = p.Results;
if isempty(o.methodsLibrary), o.methodsLibrary = getenv('N4M_LIB_PATH'); end
if isstruct(model), original = model.archive; else, original = model; end
coverages = jsonencode(o.coverages(:).');
if isscalar(o.coverages), coverages = ['[' coverages ']']; end
flags = struct('archive', original, 'destination', o.archive, 'source_id', o.sourceId, ...
    'methods_library', o.methodsLibrary, 'run_id', ['run:calibrate:' strrep(tempname(), filesep, '_')], ...
    'coverages', coverages, 'small_sample_policy', jsonencode(o.smallSamplePolicy));
[native, nativeJson] = nirs4all.workflowCli(o.cli, 'conformal-calibrate', dataset, flags);
result = struct('schema', 'nirs4all.calibrated.v1', 'archive', o.archive, 'calibration', native.calibration, ...
    'native_json', nativeJson);
end

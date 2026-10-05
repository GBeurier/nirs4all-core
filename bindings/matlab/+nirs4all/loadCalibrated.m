function result = loadCalibrated(path, varargin)
%LOADCALIBRATED Validate and reopen a shared calibrated archive.
p = inputParser(); addParameter(p, 'cli', ''); parse(p, varargin{:});
[native, nativeJson] = nirs4all.workflowCli(p.Results.cli, 'conformal-load', [], struct('archive', path));
result = struct('schema', 'nirs4all.calibrated.v1', 'archive', path, 'calibration', native.calibration, ...
    'native_json', nativeJson);
end

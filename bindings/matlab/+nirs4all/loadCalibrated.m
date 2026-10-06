function result = loadCalibrated(path, varargin)
%LOADCALIBRATED Validate and reopen a shared calibrated archive.
p = inputParser(); addParameter(p, 'cli', ''); parse(p, varargin{:});
[native, nativeJson, cli] = nirs4all.workflowCli(p.Results.cli, 'conformal-load', [], struct('archive', path));
[ok, attributes] = fileattrib(path);
if ~ok, error('nirs4all:CalibrationArchive', 'Missing calibrated archive'); end
result = struct('schema', 'nirs4all.calibrated.v1', 'archive', attributes.Name, 'calibration', native.calibration, ...
    'native_json', nativeJson, 'cli', cli);
end

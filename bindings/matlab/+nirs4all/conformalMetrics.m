function result = conformalMetrics(model, prediction, y, sampleIds, varargin)
%CONFORMALMETRICS Evaluate identity-aligned truth using the native policy.
p = inputParser(); addParameter(p, 'cli', ''); parse(p, varargin{:});
% Numeric Nx1 matrices must remain JSON arrays of rows in Octave/MATLAB.
values = arrayfun(@(i) num2cell(y(i,:)), (1:size(y,1)).', 'UniformOutput', false);
record = struct('calibration_json', model.native_json, 'prediction_json', prediction.native_json, ...
    'truth', struct('sample_ids', {cellstr(sampleIds)}, 'values', {values}));
result = nirs4all.workflowCli(p.Results.cli, 'conformal-metrics', record, struct());
end

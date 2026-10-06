function workflow = run(dataset, varargin)
%RUN Native two-fold CV/OOF PLS selection and SNV/Savitzky-Golay full refit.
%   DATASET is the IO-owned nirs4all.dataset.v1 record. Native IO validates it.
p = inputParser();
addParameter(p, 'sourceId', 'spectra'); addParameter(p, 'components', [1 2]);
addParameter(p, 'preprocessing', 'snv_savgol'); addParameter(p, 'cli', '');
addParameter(p, 'methodsLibrary', getenv('N4M_LIBRARY_PATH'));
addParameter(p, 'archive', [tempname() '.n4a']);
[~, uniqueRun] = fileparts(tempname()); addParameter(p, 'runId', ['run:matlab:workflow:' uniqueRun]);
parse(p, varargin{:}); options = p.Results;
if isempty(options.cli) && isa(dataset, 'nirs4all.PublicDataset'), options.cli = dataset.coreCli; end
if isempty(options.methodsLibrary), options.methodsLibrary = getenv('N4M_LIB_PATH'); end
if isempty(options.methodsLibrary), error('nirs4all:MethodsLibrary', 'methodsLibrary is required'); end
counts = options.components;
if ~isnumeric(counts) || numel(counts)<2 || numel(counts)>32 || any(~isfinite(counts)) || ...
        any(counts<1 | counts~=floor(counts) | counts>2147483647) || numel(unique(counts))~=numel(counts)
    error('nirs4all:Components', 'components requires 2 to 32 distinct positive i32 integers');
end
if ~any(strcmp(options.preprocessing, {'raw', 'snv_savgol'})), error('nirs4all:Preprocessing', 'preprocessing must be raw or snv_savgol'); end
flags = struct('source_id', options.sourceId, 'components', jsonencode(counts(:).'), ...
    'preprocessing', options.preprocessing, 'methods_library', options.methodsLibrary, ...
    'archive', options.archive, 'run_id', options.runId, 'results_directory', [options.archive '.results']);
[outcome, ~, cli] = nirs4all.workflowCli(options.cli, 'workflow-run', dataset, flags);
workflow = struct('schema', 'nirs4all.workflow.v1', 'archive', outcome.model_archive, 'outcome', outcome, 'cli', cli, ...
    'config', struct('sourceId', options.sourceId, 'components', counts, 'preprocessing', options.preprocessing));
end

function result = saveExperiment(nativeDirectory, destination, modelArchive, cli)
%SAVEEXPERIMENT Save native result evidence and validate optional model closure.
if nargin < 3, modelArchive = ''; end
if nargin < 4, cli = ''; end
flags = struct('input', char(nativeDirectory), 'destination', char(destination));
if ~isempty(modelArchive), flags.archive = char(modelArchive); end
nirs4all.workflowCli(cli, 'experiment-save', [], flags);
result = nirs4all.resultView(destination, cli);
end

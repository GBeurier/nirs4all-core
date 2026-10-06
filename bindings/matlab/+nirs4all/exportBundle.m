function directory = exportBundle(archive, directory, metadata, metadataName, cli)
%EXPORTBUNDLE Stage the export and publish through Core's exclusive writer.
if exist(directory, 'file'), error('nirs4all:Export', 'Destination must be new'); end
if nargin < 5, cli = ''; end
payload = jsonencode(metadata);
parent = fileparts(directory);
if isempty(parent), parent = '.'; end
if ~exist(parent, 'dir'), mkdir(parent); end
stage = tempname(parent); mkdir(stage);
cleanup = onCleanup(@() cleanStage(stage)); %#ok<NASGU>
[ok, message] = copyfile(archive, fullfile(stage, 'model.n4a'));
if ~ok, error('nirs4all:Export', '%s', message); end
file = fopen(fullfile(stage, metadataName), 'w');
if file < 0, error('nirs4all:Export', 'Cannot write export metadata'); end
closeFile = onCleanup(@() fclose(file));
count = fprintf(file, '%s', payload);
if count < numel(payload), error('nirs4all:Export', 'Incomplete export metadata'); end
clear closeFile;
nirs4all.workflowCli(cli, 'publish-directory', [], struct('input', stage, 'destination', directory));
end

function cleanStage(stage)
if exist(stage, 'dir'), rmdir(stage, 's'); end
end

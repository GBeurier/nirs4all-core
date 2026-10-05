function workflow = loadWorkflow(directory, cli)
%LOADWORKFLOW Reopen an archive with native-validated outcome/configuration.
if nargin<2, cli=''; end
metadata=jsondecode(fileread(fullfile(directory,'workflow.json')));
if ~strcmp(metadata.schema,'nirs4all.workflow.v1') || ~strcmp(metadata.archive,'model.n4a')
    error('nirs4all:WorkflowExport','Invalid workflow metadata');
end
archive=fullfile(directory,'model.n4a');
if ~exist(archive,'file'), error('nirs4all:WorkflowExport','Missing native archive'); end
native=nirs4all.workflowCli(cli,'workflow-load',[],struct('archive',archive));
c=metadata.config;
if ~isfield(metadata,'training_outcome_fingerprint')
    error('nirs4all:WorkflowExport','Missing native training identity');
end
fingerprint=metadata.training_outcome_fingerprint;
if ~strcmp(native.training_outcome.outcome_fingerprint,fingerprint) || ...
        ~strcmp(native.config.source_id,c.sourceId) || ~strcmp(native.config.preprocessing,c.preprocessing) || ...
        ~isequal(native.config.components(:),c.components(:))
    error('nirs4all:WorkflowExport','Workflow metadata differs from native archive');
end
workflow=struct('schema','nirs4all.workflow.v1','archive',archive,'outcome',native, ...
    'config',struct('sourceId',native.config.source_id,'components',native.config.components, ...
                    'preprocessing',native.config.preprocessing));
end

function directory = exportWorkflow(workflow, directory)
%EXPORTWORKFLOW Copy the shared archive with path-free native identity metadata.
metadata=struct('schema','nirs4all.workflow.v1','archive','model.n4a', ...
    'config',workflow.config,'training_outcome_fingerprint', ...
    workflow.outcome.training_outcome.outcome_fingerprint);
directory = nirs4all.exportBundle(workflow.archive, directory, metadata, 'workflow.json');
end

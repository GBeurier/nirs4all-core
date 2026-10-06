function directory = exportTuning(result, directory)
%EXPORTTUNING Persist the native model and signed tuning options.
record=struct('schema','nirs4all.tuning.v1','config',result.config, ...
 'training_outcome_fingerprint',result.outcome.training_outcome.outcome_fingerprint, ...
 'archive_sha256',result.outcome.archive_sha256);
cli = ''; if isfield(result,'cli'), cli = result.cli; end
directory = nirs4all.exportBundle(result.archive, directory, record, 'tuning.json', cli);
end

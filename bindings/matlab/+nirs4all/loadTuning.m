function result = loadTuning(directory, cli)
%LOADTUNING Reload the archive and verify its native study options.
if nargin<2, cli=''; end
saved=jsondecode(fileread(fullfile(directory,'tuning.json')));
archive=fullfile(directory,'model.n4a');
[native, ~, cli]=nirs4all.workflowCli(cli,'tuning-load',[],struct('archive',archive));
c=saved.config;
if ~isnumeric(native.config.seed) || ~isscalar(native.config.seed) || ~isfinite(native.config.seed) || ...
 native.config.seed<0 || native.config.seed>flintmax-1 || native.config.seed~=fix(native.config.seed)
 error('nirs4all:TuningSeed','Native seed exceeds supported exact integer range 0..2^53-1');
end
if ~strcmp(saved.schema,'nirs4all.tuning.v1') || ...
 ~strcmp(saved.training_outcome_fingerprint,native.training_outcome.outcome_fingerprint) || ...
 ~strcmp(saved.archive_sha256,native.archive_sha256) || ...
 ~strcmp(c.source_id,native.config.source_id) || ~strcmp(c.metric,native.config.metric) || ...
 ~strcmp(c.sampler,native.config.sampler) || c.seed~=native.config.seed || c.trials~=native.config.trials
 error('nirs4all:TuningExport','Tuning metadata differs from native archive');
end
result=struct('archive',native.model_archive,'outcome',native,'config',native.config,'cli',cli);
end

fixture=getenv('NIRS4ALL_TUNING_DATASET');
assert(~isempty(fixture),'Native tuning fixture required');
directory=tempname(); mkdir(directory); cleanup=onCleanup(@() rmdir(directory,'s'));
choices=struct('shape',{{[2 3],[4 5]}},'selection',{{'one','two'}});
constraints=struct('exclude',{{{struct('dimension','shape','label','choice:1'),struct('dimension','selection','label','choice:1')}}});
assert(numel(nirs4all.generate(choices,'constraints',constraints))==3);
variants=nirs4all.generate(choices,'strategy','random','constraints',constraints,'count',2,'seed',17);
assert(numel(variants)==2);
for i=1:numel(variants), assert(ischar(variants(i).seed) && ~isempty(regexp(variants(i).seed,'^[0-9]+$','once'))); end
assert(isequal(variants,nirs4all.generate(choices,'strategy','random','constraints',constraints,'count',2,'seed',17)));
data=fileread(fixture); raw=jsondecode(data);
for invalidSeed = [-1 0.5 Inf flintmax]
 rejected=fullfile(directory,'invalid-seed.n4a'); failed=false;
 try, nirs4all.tune(data,'seed',invalidSeed,'archive',rejected); catch failure, failed=strcmp(failure.identifier,'nirs4all:TuningSeed'); end
 assert(failed && ~exist(rejected,'file'));
end
for seed = [1e5 3e9]
 seeded=nirs4all.tune(data,'trials',1,'seed',seed,'archive',fullfile(directory,sprintf('seed-%.0f.n4a',seed)), ...
  'methodsLibrary',getenv('N4M_LIBRARY_PATH'));
 assert(seeded.config.seed==seed && seeded.outcome.training_outcome.effective_plan.campaign.root_seed==seed);
end
first=nirs4all.tune(data,'trials',2,'archive',fullfile(directory,'first.n4a'), ...
 'methodsLibrary',getenv('N4M_LIBRARY_PATH'),'runId','run:octave:hpo:first');
resumed=nirs4all.resumeTuning(first,data,4,'archive',fullfile(directory,'resumed.n4a'), ...
 'methodsLibrary',getenv('N4M_LIBRARY_PATH'),'runId','run:octave:hpo:resumed');
old=first.outcome.training_outcome.methods_hpo_resume_state.terminal_trials;
new=resumed.outcome.training_outcome.methods_hpo_resume_state.terminal_trials;
retained=new(1:2); assert(numel(new)==4 && isequal(old(:),retained(:)));
nirs4all.exportTuning(resumed,fullfile(directory,'export'));
loaded=nirs4all.loadTuning(fullfile(directory,'export'));
X=raw.dataset.sources(1).array.values(1:3,:);
replay=nirs4all.predict(loaded,X,'sampleIds',{'cold:m:0','cold:m:1','cold:m:2'});
assert(isequal(replay.outputs(1).predictions(1).sample_ids(:),{'cold:m:0';'cold:m:1';'cold:m:2'}));
assert(all(strcmp({replay.lineage.phase},'PREDICT')));
unsafeDirectory=fullfile(directory,'u64-export'); mkdir(unsafeDirectory);
unsafe=nirs4all.workflowCli('','tuning-run',data,struct('trials','1','seed','9007199254740993', ...
 'sampler','random','metric','rmse','source_id','spectra','methods_library',getenv('N4M_LIBRARY_PATH'), ...
 'archive',fullfile(unsafeDirectory,'model.n4a'),'run_id','run:octave:u64:cross-host'));
saved=struct('schema','nirs4all.tuning.v1','config',struct('source_id','spectra','trials',1,'seed',0,'sampler','random','metric','rmse'), ...
 'archive_sha256',unsafe.archive_sha256,'training_outcome_fingerprint',unsafe.training_outcome.outcome_fingerprint);
encoded=strrep(jsonencode(saved),'"seed":0','"seed":9007199254740993');
fid=fopen(fullfile(unsafeDirectory,'tuning.json'),'w'); fwrite(fid,encoded); fclose(fid);
failed=false;
try, nirs4all.loadTuning(unsafeDirectory); catch failure, failed=strcmp(failure.identifier,'nirs4all:TuningSeed'); end
assert(failed,'Unsafe native u64 seed must be explicitly refused on load');
disp('OCTAVE_NATIVE_TUNING_RESUME_COLD_PREDICT_OK');

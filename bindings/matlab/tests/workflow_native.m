% Genuine native Core CLI workflow over deterministic public raw IO fixture.
root = fileparts(fileparts(mfilename('fullpath'))); addpath(root);
cli = getenv('NIRS4ALL_CORE_CLI'); library = getenv('N4M_LIBRARY_PATH');
if isempty(cli) || isempty(library), error('Explicit native runtime paths required'); end
directory = tempname(); mkdir(directory); cleanup = onCleanup(@() rmdir(directory,'s'));
fixture = fullfile(root,'tests','fixtures','workflow_dense.json');
dataset = nirs4all.dataset(fixture,cli);
testSourceRoot = fullfile(directory,'source'); mkdir(testSourceRoot);
workflow = nirs4all.run(dataset,'cli',cli,'methodsLibrary',library,'components',[1 2], ...
    'archive',fullfile(testSourceRoot,'model.n4a'));
outcome=workflow.outcome.training_outcome;
assert(numel(outcome.effective_plan.variants)==2);
assert(numel(outcome.oof_averages(1).predictions.unit_ids)==12);
assert(numel(outcome.execution_bundle.refit_artifacts)==1);
exported=fullfile(directory,'exported'); nirs4all.export(workflow,exported);
manifest=fileread(fullfile(exported,'workflow.json'));
assert(isempty(strfind(manifest,testSourceRoot)));
assert(isempty(strfind(manifest,'native_results_dir')));
metadata=jsondecode(manifest); assert(~isfield(metadata,'outcome'));
previous=rmfield(metadata,'training_outcome_fingerprint'); previous.outcome=workflow.outcome;
f=fopen(fullfile(exported,'workflow.json'),'w'); fprintf(f,'%s',jsonencode(previous)); fclose(f);
refused=false;
try nirs4all.load(exported); catch err, refused=~isempty(strfind(err.message,'Missing native training identity')); end
assert(refused);
f=fopen(fullfile(exported,'workflow.json'),'w'); fprintf(f,'%s',manifest); fclose(f);
relocated=fullfile(directory,'relocated'); movefile(exported,relocated); exported=relocated;
rmdir(testSourceRoot,'s'); loaded=nirs4all.load(exported);
metadata.training_outcome_fingerprint=repmat('0',1,64);
f=fopen(fullfile(exported,'workflow.json'),'w'); fprintf(f,'%s',jsonencode(metadata)); fclose(f);
refused=false;
try nirs4all.load(exported); catch err, refused=~isempty(strfind(err.message,'differs')); end
assert(refused);
f=fopen(fullfile(exported,'workflow.json'),'w'); fprintf(f,'%s',manifest); fclose(f);
X=dataset.record.dataset.sources(1).array.values(1:2,:);
prediction=nirs4all.predict(loaded,X,'cli',cli,'methodsLibrary',library,'sampleIds',{'fresh1','fresh2'});
assert(all(isfinite(prediction.outputs(1).predictions(1).values(:))));
assert(isequal(prediction.outputs(1).predictions(1).sample_ids,{'fresh1';'fresh2'}));
fresh=nirs4all.retrain(loaded,dataset,'cli',cli,'methodsLibrary',library, ...
    'archive',fullfile(directory,'fresh.n4a'));
assert(~strcmp(fresh.outcome.run_id,workflow.outcome.run_id));
assert(~strcmp(fresh.outcome.training_outcome.outcome_fingerprint,outcome.outcome_fingerprint));
fprintf('OCTAVE_WORKFLOW_NATIVE_PASS variants=2 oof_rows=12 refit=1 relocated_predict=1 retrain=1\n');

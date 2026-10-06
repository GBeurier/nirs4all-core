% Explicit runtime survives changes to the environment and export relocation.
root = fileparts(fileparts(mfilename('fullpath'))); addpath(root);
addpath(fullfile(root,'tests'));
cli = getenv('NIRS4ALL_CORE_CLI'); assert(~isempty(cli));
previous = cli; restore = onCleanup(@() setenv('NIRS4ALL_CORE_CLI', previous));
directory = [tempname() ' with spaces']; mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's'));
previousDirectory = pwd(); [cliDirectory,cliName,cliExtension] = fileparts(cli);
restoreDirectory = onCleanup(@() cd(previousDirectory));
cd(cliDirectory);
dataset = nirs4all.dataset(fullfile(root,'tests','fixtures','workflow_dense.json'), ...
    fullfile('.',[cliName cliExtension]));
cd(previousDirectory); clear restoreDirectory;
assert(strcmp(dataset.coreCli,cli));
model = nirs4all.run(dataset, 'cli',cli,'archive',fullfile(directory,'model.n4a'));
study = nirs4all.tune(dataset, 'cli',cli,'trials',1,'archive',fullfile(directory,'study.n4a'));
X = dataset.record.dataset.sources(1).array.values(1:2,:);
setenv('NIRS4ALL_CORE_CLI','missing-default-native-cli');
prediction = nirs4all.predict(model,X);
assert(all(isfinite(prediction.outputs(1).predictions(1).values(:))));
studyPrediction = nirs4all.predict(study,X); assert(~isempty(studyPrediction.outputs));
continued = nirs4all.resumeTuning(study,dataset,2,'archive',fullfile(directory,'continued.n4a'));
assert(strcmp(continued.cli,study.cli));
nirs4all.export(model,fullfile(directory,'export'));
nirs4all.exportTuning(study,fullfile(directory,'study-export'));
loaded = nirs4all.load(fullfile(directory,'export'),cli);
loadedPrediction = nirs4all.predict(loaded,X);
assert(isequal(loadedPrediction.outputs(1).predictions(1).values, ...
               prediction.outputs(1).predictions(1).values));
loadedStudy = nirs4all.loadTuning(fullfile(directory,'study-export'),cli);
loadedStudyPrediction = nirs4all.predict(loadedStudy,X); assert(~isempty(loadedStudyPrediction.outputs));
fresh = nirs4all.retrain(model,dataset,'archive',fullfile(directory,'fresh.n4a'));
assert(strcmp(fresh.cli,cli));
defaultDataset = nirs4all.dataset(fullfile(root,'tests','fixtures','workflow_dense.json'),cli);
datasetWorkflow = nirs4all.run(defaultDataset,'archive',fullfile(directory,'dataset-cli.n4a'));
assert(strcmp(datasetWorkflow.cli,cli));
calibrationData = nirs4all.dataset(fullfile(getenv('NIRS4ALL_BLOCK5_FIXTURE'),'dataset-calibration.json'),cli);
calibrated = nirs4all.calibrate(model,calibrationData,'coverages',0.8, ...
    'archive',fullfile(directory,'calibrated.n4a'));
independent = jsondecode(fileread(fullfile(getenv('NIRS4ALL_BLOCK5_FIXTURE'),'independent.json')));
intervals = nirs4all.predictCalibrated(calibrated,independent.x,independent.sample_ids);
nirs4all.conformalMetrics(calibrated,intervals,independent.y,independent.sample_ids);
nirs4all.robustness(calibrated,independent.x,independent.y,independent.sample_ids);
fprintf('OCTAVE_NATIVE_RUNTIME_PASS stored_cli resume export relocated_predict dataset_cli\n');
clear restore cleanup;
